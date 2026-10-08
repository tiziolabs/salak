//! The view of one rendered Markdown document.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use salak_core::markdown::{is_openable, Link};

use crate::buffer::{self, Env, Filler};
use crate::layout;
use crate::theme::Themer;

/// What is drawn right away; the rest follows while the window stays usable.
const FIRST_CHUNK: Duration = Duration::from_millis(40);
const CHUNK: Duration = Duration::from_millis(30);

/// Progress of the drawing of a document.
#[derive(Default)]
struct Loading {
    done: Cell<bool>,
    /// An anchor to scroll to as soon as its heading is drawn.
    pending: RefCell<Option<String>>,
    /// A scroll position to restore once the whole document is drawn.
    restore: Cell<Option<f64>>,
}

pub struct Document {
    pub widget: gtk::ScrolledWindow,
    view: gtk::TextView,
    loading: Rc<Loading>,
}

impl Document {
    /// `base` is the folder of the document, `root` the opened folder.
    /// `on_open` receives the links the window has to follow: help pages and
    /// local Markdown files. The others are handled here.
    pub fn new(
        markdown: &str,
        base: &Path,
        root: &Path,
        themer: &Rc<Themer>,
        on_open: Rc<dyn Fn(Link)>,
    ) -> Document {
        let blocks = layout::build(markdown, base, root);

        let view = gtk::TextView::builder()
            .buffer(&gtk::TextBuffer::new(Some(&buffer::tag_table(themer))))
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(gtk::WrapMode::WordChar)
            .top_margin(24)
            .bottom_margin(24)
            .left_margin(buffer::MARGIN)
            .right_margin(buffer::MARGIN)
            .build();

        let loading = Rc::new(Loading::default());
        let weak = view.downgrade();
        let dispatch: Rc<dyn Fn(&Link)> = Rc::new({
            let loading = loading.clone();
            move |link| match link {
                Link::Anchor(id) => {
                    if let Some(view) = weak.upgrade() {
                        scroll_to(&view, &loading, id);
                    }
                }
                Link::External(url) => {
                    let parent = weak
                        .upgrade()
                        .and_then(|view| view.root())
                        .and_downcast::<gtk::Window>();
                    let launcher = gtk::UriLauncher::new(url);
                    glib::spawn_future_local(async move {
                        if let Err(err) = launcher.launch_future(parent.as_ref()).await {
                            eprintln!("salak: cannot open the link: {}", err.message());
                        }
                    });
                }
                Link::Local { path, .. } => {
                    if is_openable(path) {
                        on_open(link.clone());
                    }
                }
                Link::Help(_) => on_open(link.clone()),
                Link::Unsupported => {}
            }
        });

        let links = Rc::new(RefCell::new(Vec::new()));
        let avail = Rc::new(Cell::new(0));
        let env = Env {
            root: root.to_path_buf(),
            links: links.clone(),
            activate: dispatch.clone(),
            avail: avail.clone(),
            themer: themer.clone(),
        };
        let mut filler = Filler::new(blocks, env);
        let anchored = Rc::new(RefCell::new(Vec::new()));
        let (first, done) = filler.run(&view, FIRST_CHUNK);
        anchored.borrow_mut().extend(first);
        if done {
            loading.done.set(true);
        } else {
            let weak = view.downgrade();
            let anchored = anchored.clone();
            let avail = avail.clone();
            let loading = loading.clone();
            glib::idle_add_local(move || {
                let Some(view) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let (more, done) = filler.run(&view, CHUNK);
                for item in &more {
                    item.resize(avail.get());
                }
                anchored.borrow_mut().extend(more);
                if !done {
                    return glib::ControlFlow::Continue;
                }
                loading.done.set(true);
                if let Some(id) = loading.pending.take() {
                    scroll_to(&view, &loading, &id);
                } else if let Some(value) = loading.restore.take() {
                    if let Some(window) = scrolled(&view) {
                        window.vadjustment().set_value(value);
                    }
                }
                glib::ControlFlow::Break
            });
        }

        // The width of the anchored widgets follows the one of the text.
        let last_width = Cell::new(0);
        view.add_tick_callback({
            let anchored = anchored.clone();
            move |view, _| {
                let width = view.width();
                if width != last_width.replace(width) {
                    let text = (width - view.left_margin() - view.right_margin()).max(0);
                    avail.set(text);
                    for item in anchored.borrow().iter() {
                        item.resize(text);
                    }
                }
                for item in anchored.borrow().iter() {
                    item.settle();
                }
                glib::ControlFlow::Continue
            }
        });

        connect_links(&view, links, dispatch);

        let clamp = adw::Clamp::builder().child(&view).build();
        // Colours, fonts and width follow the theme and the mode.
        let (weak_view, weak_clamp) = (view.downgrade(), clamp.downgrade());
        themer.observe(move |themer| {
            let (Some(view), Some(clamp)) = (weak_view.upgrade(), weak_clamp.upgrade()) else {
                return false;
            };
            buffer::restyle(&view.buffer().tag_table(), themer);
            let width = themer.max_width(&view, buffer::MARGIN);
            clamp.set_maximum_size(width);
            clamp.set_tightening_threshold(width);
            true
        });

        let widget = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .css_classes(["md-page"])
            .child(&buffer::viewport(&clamp))
            .build();
        Document {
            widget,
            view,
            loading,
        }
    }

    /// The document shown by the content of a tab.
    pub fn of(page: &gtk::Widget) -> Option<Document> {
        let widget = page.clone().downcast::<gtk::ScrolledWindow>().ok()?;
        // The clamp is not scrollable: the scrolled window wrapped it in a viewport.
        let clamp = widget
            .child()?
            .downcast::<gtk::Viewport>()
            .ok()?
            .child()?
            .downcast::<adw::Clamp>()
            .ok()?;
        let view = clamp.child()?.downcast::<gtk::TextView>().ok()?;
        // A document still being drawn when it is found again simply does not
        // scroll to a heading that is not there yet.
        let loading = Rc::new(Loading::default());
        loading.done.set(true);
        Some(Document {
            widget,
            view,
            loading,
        })
    }

    /// Draws `markdown` again in place, at the same scroll position.
    pub fn reload(
        &self,
        markdown: &str,
        base: &Path,
        root: &Path,
        themer: &Rc<Themer>,
        on_open: Rc<dyn Fn(Link)>,
    ) {
        let value = self.widget.vadjustment().value();
        let fresh = Document::new(markdown, base, root, themer, on_open);
        let content = fresh.widget.child();
        fresh.widget.set_child(None::<&gtk::Widget>);
        self.widget.set_child(content.as_ref());
        if fresh.loading.done.get() {
            // The new content has no size before its first layout.
            let widget = self.widget.clone();
            glib::timeout_add_local_once(Duration::from_millis(50), move || {
                widget.vadjustment().set_value(value);
            });
        } else {
            fresh.loading.restore.set(Some(value));
        }
    }

    pub fn focus(&self) {
        self.view.grab_focus();
    }

    /// The text view of the document.
    pub fn view(&self) -> &gtk::TextView {
        &self.view
    }

    /// The text views to search: the document, then the code blocks embedded
    /// in it, with the offset of the anchor that holds each.
    pub fn text_views(&self) -> Vec<(gtk::TextView, Option<i32>)> {
        let mut views = vec![(self.view.clone(), None)];
        let buffer = self.view.buffer();
        let mut iter = buffer.start_iter();
        loop {
            if let Some(anchor) = iter.child_anchor() {
                for widget in anchor.widgets() {
                    nested_views(&widget, iter.offset(), &mut views);
                }
            }
            if !iter.forward_find_char(|c| c == '\u{FFFC}', None) {
                break;
            }
        }
        views
    }

    /// Scrolls by `delta` pixels.
    pub fn scroll_by(&self, delta: f64) {
        let adjustment = self.widget.vadjustment();
        let end = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
        adjustment.set_value((adjustment.value() + delta).clamp(adjustment.lower(), end));
    }

    /// Scrolls by a fraction of the visible height.
    pub fn scroll_page(&self, fraction: f64) {
        self.scroll_by(self.widget.vadjustment().page_size() * fraction);
    }

    pub fn scroll_edge(&self, top: bool) {
        self.scroll_by(if top { f64::MIN } else { f64::MAX });
    }

    /// Scrolls to a heading or an anchor, once it is drawn.
    pub fn scroll_to(&self, id: &str) {
        scroll_to(&self.view, &self.loading, id);
    }
}

fn nested_views(widget: &gtk::Widget, at: i32, views: &mut Vec<(gtk::TextView, Option<i32>)>) {
    if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        views.push((view.clone(), Some(at)));
        return;
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        nested_views(&widget, at, views);
        child = widget.next_sibling();
    }
}

fn scrolled(view: &gtk::TextView) -> Option<gtk::ScrolledWindow> {
    view.ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
}

fn scroll_to(view: &gtk::TextView, loading: &Loading, id: &str) {
    match view.buffer().mark(id) {
        Some(mark) => view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0),
        None if !loading.done.get() => *loading.pending.borrow_mut() = Some(id.to_string()),
        None => {}
    }
}

/// The link under a point of the view.
fn link_at(view: &gtk::TextView, links: &RefCell<Vec<Link>>, x: f64, y: f64) -> Option<Link> {
    let (x, y) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let (iter, _) = view.iter_at_position(x, y)?;
    let index = iter.tags().iter().find_map(|tag| {
        tag.name()?
            .strip_prefix("link-")
            .and_then(|index| index.parse::<usize>().ok())
    })?;
    links.borrow().get(index).cloned()
}

/// Clicks, pointer cursor and tooltips of the links of the text.
fn connect_links(view: &gtk::TextView, links: Rc<RefCell<Vec<Link>>>, dispatch: Rc<dyn Fn(&Link)>) {
    let click = gtk::GestureClick::builder().button(1).build();
    click.connect_released({
        let links = links.clone();
        move |gesture, _, x, y| {
            let Some(view) = gesture.widget().and_downcast::<gtk::TextView>() else {
                return;
            };
            // The end of a drag that selected text is not a click.
            if view.buffer().has_selection() {
                return;
            }
            if let Some(link) = link_at(&view, &links, x, y) {
                dispatch(&link);
            }
        }
    });
    view.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion({
        let links = links.clone();
        move |controller, x, y| {
            let Some(view) = controller.widget().and_downcast::<gtk::TextView>() else {
                return;
            };
            let over_link = link_at(&view, &links, x, y).is_some();
            view.set_cursor_from_name(Some(if over_link { "pointer" } else { "text" }));
        }
    });
    view.add_controller(motion);

    view.set_has_tooltip(true);
    view.connect_query_tooltip(move |view, x, y, keyboard, tooltip| {
        if keyboard {
            return false;
        }
        match link_at(view, &links, x as f64, y as f64) {
            Some(link) => {
                tooltip.set_text(Some(&buffer::link_title(&link)));
                true
            }
            None => false,
        }
    });
}

/// Style of the widgets drawn inside documents.
pub fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        .md-page { background-color: @view_bg_color; color: @view_fg_color; font-size: 110%; }
        .md-page textview, .md-page textview text { background: none; color: inherit; }
        .code-block { background: alpha(currentColor, 0.07); border-radius: 6px; }
        .code-block textview, .code-block textview text { background: none; }
        .md-table { border: 1px solid alpha(currentColor, 0.2); }
        .md-cell { padding: 6px 12px; border-right: 1px solid alpha(currentColor, 0.2);
                   border-bottom: 1px solid alpha(currentColor, 0.2); }
        .md-head { font-weight: bold; background: alpha(currentColor, 0.07); }
        .md-odd { background: alpha(currentColor, 0.035); }
        ",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
