//! Find in the document of the selected tab: the search bar, the highlights
//! and the jump from one match to the next.
//!
//! The text of the document and the code blocks embedded in it are searched.
//! The cells of the tables are labels, which are not.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::gdk;
use gtk::glib::{self, clone};

use crate::document::Document;
use crate::window::Window;

const MATCH: &str = "search-match";
const CURRENT: &str = "search-current";
/// A search that matches this much is no longer a search.
const MAX_HITS: usize = 10_000;

/// One occurrence of the query.
struct Hit {
    view: gtk::TextView,
    /// The text view of the document and the offset of the anchor holding
    /// `view`, when it is a code block.
    outer: Option<(gtk::TextView, i32)>,
    start: i32,
    end: i32,
}

impl Hit {
    /// Where the hit comes in the document.
    fn order(&self) -> (i32, i32) {
        (self.outer.as_ref().map_or(self.start, |(_, at)| *at), self.start)
    }

    fn mark(&self, tag: &str, on: bool) {
        let buffer = self.view.buffer();
        let (start, end) = (
            buffer.iter_at_offset(self.start),
            buffer.iter_at_offset(self.end),
        );
        if on {
            buffer.apply_tag_by_name(tag, &start, &end);
        } else {
            buffer.remove_tag_by_name(tag, &start, &end);
        }
    }
}

pub struct Search {
    pub bar: gtk::SearchBar,
    entry: gtk::SearchEntry,
    count: gtk::Label,
    previous: gtk::Button,
    next: gtk::Button,
    hits: RefCell<Vec<Hit>>,
    current: Cell<usize>,
    scrolled: glib::WeakRef<gtk::ScrolledWindow>,
}

impl Search {
    pub fn new() -> Rc<Search> {
        let entry = gtk::SearchEntry::builder()
            .hexpand(true)
            .placeholder_text("Find in document")
            .build();
        let count = gtk::Label::builder()
            .css_classes(["dim-label", "numeric"])
            .build();
        let previous = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text("Previous Match (Shift+Enter)")
            .css_classes(["flat"])
            .sensitive(false)
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text("Next Match (Enter)")
            .css_classes(["flat"])
            .sensitive(false)
            .build();
        let row = gtk::Box::builder().spacing(6).build();
        row.append(&entry);
        row.append(&count);
        row.append(&previous);
        row.append(&next);
        let clamp = adw::Clamp::builder()
            .maximum_size(520)
            .hexpand(true)
            .child(&row)
            .build();
        // No key capture: single-letter keys belong to the document.
        let bar = gtk::SearchBar::builder()
            .show_close_button(true)
            .child(&clamp)
            .build();
        bar.connect_entry(&entry);
        Rc::new(Search {
            bar,
            entry,
            count,
            previous,
            next,
            hits: RefCell::default(),
            current: Cell::new(0),
            scrolled: glib::WeakRef::new(),
        })
    }

    /// Shows the bar, with the focus in the entry.
    pub fn open(&self) {
        self.bar.set_search_mode(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    pub fn close(&self) {
        self.bar.set_search_mode(false);
    }

    pub fn is_open(&self) -> bool {
        self.bar.is_search_mode()
    }

    /// Looks for the query in `document` again, from its first occurrence.
    pub fn update(&self, document: Option<&Document>) {
        self.clear();
        let query = self.entry.text();
        let Some(document) = document.filter(|_| self.is_open() && !query.is_empty()) else {
            return;
        };
        self.scrolled.set(Some(&document.widget));

        let mut hits = Vec::new();
        'views: for (view, anchor) in document.text_views() {
            let buffer = view.buffer();
            ensure_tags(&buffer);
            let outer = anchor.map(|at| (document.view().clone(), at));
            let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY;
            let mut from = buffer.start_iter();
            while let Some((start, end)) = from.forward_search(&query, flags, None) {
                let hit = Hit {
                    view: view.clone(),
                    outer: outer.clone(),
                    start: start.offset(),
                    end: end.offset(),
                };
                hit.mark(MATCH, true);
                hits.push(hit);
                if hits.len() >= MAX_HITS {
                    break 'views;
                }
                from = end;
            }
        }
        hits.sort_by_key(Hit::order);

        let found = !hits.is_empty();
        *self.hits.borrow_mut() = hits;
        self.previous.set_sensitive(found);
        self.next.set_sensitive(found);
        if found {
            self.show(0);
        } else {
            self.count.set_label("No results");
            self.entry.add_css_class("error");
        }
    }

    /// Removes the highlights and the count.
    pub fn clear(&self) {
        for hit in self.hits.take() {
            hit.mark(MATCH, false);
            hit.mark(CURRENT, false);
        }
        self.current.set(0);
        self.count.set_label("");
        self.entry.remove_css_class("error");
        self.previous.set_sensitive(false);
        self.next.set_sensitive(false);
    }

    /// Moves to the next match, or the previous one, from the last to the first.
    pub fn step(&self, forward: bool) {
        let len = self.hits.borrow().len();
        if len == 0 {
            return;
        }
        let current = self.current.get();
        self.show(if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        });
    }

    fn show(&self, index: usize) {
        let hits = self.hits.borrow();
        if let Some(old) = hits.get(self.current.get()) {
            old.mark(CURRENT, false);
        }
        let Some(hit) = hits.get(index) else { return };
        self.current.set(index);
        hit.mark(CURRENT, true);
        self.count
            .set_label(&format!("{} of {}", index + 1, hits.len()));
        self.reveal(hit);
    }

    /// Scrolls to the hit, unless it is already well inside the page.
    fn reveal(&self, hit: &Hit) {
        let buffer = hit.view.buffer();
        match &hit.outer {
            None => {
                let mut start = buffer.iter_at_offset(hit.start);
                hit.view.scroll_to_iter(&mut start, 0.15, false, 0.0, 0.0);
            }
            Some((outer, at)) => {
                // First the block, then the line of the block: a block can be
                // taller than the page.
                let mut anchor = outer.buffer().iter_at_offset(*at);
                outer.scroll_to_iter(&mut anchor, 0.15, false, 0.0, 0.0);
                let (view, scrolled) = (hit.view.clone(), self.scrolled.clone());
                let start = hit.start;
                glib::timeout_add_local_once(Duration::from_millis(50), move || {
                    if let Some(scrolled) = scrolled.upgrade() {
                        scroll_inside(&view, start, &scrolled);
                    }
                });
            }
        }
    }
}

/// Scrolls the page so that the line of `view` at `offset` is visible.
fn scroll_inside(view: &gtk::TextView, offset: i32, scrolled: &gtk::ScrolledWindow) {
    let rect = view.iter_location(&view.buffer().iter_at_offset(offset));
    let (x, y) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
    let Some(point) = view.compute_point(scrolled, &gtk::graphene::Point::new(x as f32, y as f32))
    else {
        return;
    };
    let adjustment = scrolled.vadjustment();
    let page = adjustment.page_size();
    let top = f64::from(point.y());
    if top < page * 0.1 || top + f64::from(rect.height()) > page * 0.9 {
        let end = (adjustment.upper() - page).max(adjustment.lower());
        adjustment.set_value((adjustment.value() + top - page * 0.3).clamp(adjustment.lower(), end));
    }
}

/// The tags of the highlights, in the table of each buffer searched. The
/// current match comes last, so that it wins over the other matches.
fn ensure_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();
    for (name, color) in [
        (MATCH, gdk::RGBA::new(1.0, 0.8, 0.0, 0.35)),
        (CURRENT, gdk::RGBA::new(1.0, 0.5, 0.0, 0.65)),
    ] {
        if table.lookup(name).is_none() {
            let tag = gtk::TextTag::builder().name(name).build();
            tag.set_property("background-rgba", color);
            table.add(&tag);
        }
    }
}

/// Connects the entry and the bar to the window.
pub fn install(window: &Rc<Window>) {
    let search = &window.search;
    let weak = Rc::downgrade(window);
    let with_window = move |action: fn(&Rc<Window>)| {
        let weak = weak.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                action(&window);
            }
        }
    };

    let changed = with_window(|window| window.research());
    search.entry.connect_search_changed(move |_| changed());
    let closed = with_window(|window| {
        if !window.search.is_open() {
            window.search.clear();
            window.focus_document();
        }
    });
    search
        .bar
        .connect_search_mode_enabled_notify(move |_| closed());

    search.entry.connect_activate(clone!(
        #[weak]
        search,
        move |_| search.step(true)
    ));
    search.entry.connect_next_match(clone!(
        #[weak]
        search,
        move |_| search.step(true)
    ));
    search.entry.connect_previous_match(clone!(
        #[weak]
        search,
        move |_| search.step(false)
    ));
    search.next.connect_clicked(clone!(
        #[weak]
        search,
        move |_| search.step(true)
    ));
    search.previous.connect_clicked(clone!(
        #[weak]
        search,
        move |_| search.step(false)
    ));

    // Shift+Enter goes back, as in most browsers.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(clone!(
        #[weak]
        search,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, state| {
            let enter = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter);
            if enter && state.contains(gdk::ModifierType::SHIFT_MASK) {
                search.step(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    ));
    search.entry.add_controller(keys);
}
