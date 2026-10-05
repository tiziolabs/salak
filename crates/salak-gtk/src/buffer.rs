//! Draws the blocks of `layout` into a `gtk::TextBuffer`.
//!
//! Text styles are tags, links are tags named `link-N` (N indexes `Env::links`),
//! heading ids are marks, and code blocks, tables, rules and images are widgets
//! at child anchors.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::gdk;
use gtk::{gio, glib, pango};
use salak_core::files;
use salak_core::markdown::Link;

use crate::layout::{Align, Block, Para, Run, Style};
use crate::theme::Themer;

/// Space left and right of the text, in pixels. A tag's `left-margin` replaces
/// the one of the view, so paragraphs add it themselves.
pub const MARGIN: i32 = 40;

pub struct Env {
    pub root: PathBuf,
    /// Targets of the links of the document, by number.
    pub links: Rc<RefCell<Vec<Link>>>,
    /// Follows a link that was clicked in a table cell.
    pub activate: Rc<dyn Fn(&Link)>,
    /// Text width available to anchored widgets, kept up to date by the
    /// document.
    pub avail: Rc<Cell<i32>>,
    pub themer: Rc<Themer>,
}

/// How an anchored widget follows the width of the text.
#[derive(Clone)]
pub enum Fit {
    /// As wide as the text.
    Width,
    /// Its natural size, shrunk to the text width. Zero until loaded.
    Image(Rc<Cell<(i32, i32)>>),
    /// As wide as the text, as high as the grid is at that width.
    Table(gtk::Grid, Rc<RefCell<gtk::TextChildAnchor>>),
}

#[derive(Clone)]
pub struct Anchored {
    pub widget: gtk::Widget,
    pub fit: Fit,
}

impl Anchored {
    /// The text view does not measure a child again when its width changes,
    /// and cells that wrap make a table higher when it is narrow. So a table
    /// asks for the height its grid actually has at the width it gets.
    pub fn settle(&self) {
        let Fit::Table(grid, anchor) = &self.fit else {
            return;
        };
        // Not before the widget has the width `resize` asked for: the grid
        // is then still at its minimum width, and very high.
        if self.widget.width() != self.widget.width_request() {
            return;
        }
        let height = grid.height() + 1;
        if height > 1 && height != self.widget.height_request() {
            self.widget.set_height_request(height);
            // The view measures a widget once, when it is anchored: anchor it
            // again so that the line gets the new height.
            if let Some(view) = self
                .widget
                .ancestor(gtk::TextView::static_type())
                .and_downcast::<gtk::TextView>()
            {
                let buffer = view.buffer();
                let mut start = buffer.iter_at_child_anchor(&*anchor.borrow());
                let offset = start.offset();
                let mut end = start;
                end.forward_char();
                buffer.delete(&mut start, &mut end);
                let mut at = buffer.iter_at_offset(offset);
                let new = buffer.create_child_anchor(&mut at);
                view.add_child_at_anchor(&self.widget, &new);
                if let Some(tag) = buffer.tag_table().lookup("block") {
                    let mut after = buffer.iter_at_offset(offset);
                    after.forward_chars(2);
                    buffer.apply_tag(&tag, &buffer.iter_at_offset(offset), &after);
                }
                *anchor.borrow_mut() = new;
            }
        }
    }

    /// Child anchors do not follow the width of the text view by themselves.
    /// Requesting exactly the available width keeps wide content inside its
    /// own scrolled window instead of widening the page.
    pub fn resize(&self, avail: i32) {
        if avail <= 0 {
            return;
        }
        match &self.fit {
            Fit::Width => self.widget.set_width_request(avail),
            Fit::Table(..) => self.widget.set_size_request(avail, -1),
            Fit::Image(natural) => {
                let (width, height) = natural.get();
                if width > 0 {
                    let shown = width.min(avail);
                    let scaled = (height as f64 * shown as f64 / width as f64).round() as i32;
                    self.widget.set_size_request(shown, scaled.max(1));
                }
            }
        }
    }
}

/// The tags every document has.
pub fn tag_table(themer: &Themer) -> gtk::TextTagTable {
    let table = gtk::TextTagTable::new();
    const SCALES: [f64; 6] = [2.0, 1.5, 1.25, 1.0, 0.875, 0.85];
    for (i, scale) in SCALES.iter().enumerate() {
        table.add(
            &gtk::TextTag::builder()
                .name(format!("h{}", i + 1))
                .scale(*scale)
                .weight(700)
                .pixels_above_lines(16)
                .build(),
        );
    }
    table.add(&gtk::TextTag::builder().name("bold").weight(700).build());
    table.add(
        &gtk::TextTag::builder()
            .name("italic")
            .style(pango::Style::Italic)
            .build(),
    );
    table.add(
        &gtk::TextTag::builder()
            .name("strike")
            .strikethrough(true)
            .build(),
    );
    table.add(&gtk::TextTag::builder().name("code").scale(0.92).build());
    table.add(
        &gtk::TextTag::builder()
            .name("sup")
            .scale(0.8)
            .rise(5 * pango::SCALE)
            .build(),
    );
    table.add(
        &gtk::TextTag::builder()
            .name("sub")
            .scale(0.8)
            .rise(-2 * pango::SCALE)
            .build(),
    );
    table.add(&gtk::TextTag::builder().name("quote").build());
    // The line of an anchored widget.
    table.add(
        &gtk::TextTag::builder()
            .name("block")
            .pixels_below_lines(12)
            .build(),
    );
    restyle(&table, themer);
    table
}

/// Sets the colours and fonts of the tags, at creation and when the theme or
/// the mode changes.
pub fn restyle(table: &gtk::TextTagTable, themer: &Themer) {
    let colors = themer.colors();
    if let Some(tag) = table.lookup("code") {
        tag.set_property("family", themer.mono_family());
        tag.set_property("background-rgba", colors.code_bg);
    }
    if let Some(tag) = table.lookup("quote") {
        tag.set_property("foreground-rgba", colors.muted);
        tag.set_property("paragraph-background-rgba", colors.quote_bg);
    }
    table.foreach(|tag| {
        if tag.name().is_some_and(|name| name.starts_with("link-")) {
            tag.set_property("foreground-rgba", colors.link);
        }
    });
}

/// Tag of the margins of a paragraph nested in quotes and lists.
fn format_tag(table: &gtk::TextTagTable, para: &Para, marker_width: i32) -> gtk::TextTag {
    let hanging = if para.marker.is_some() {
        marker_width.max(20)
    } else {
        0
    };
    let name = format!(
        "b{}-{}-{}-{}",
        para.quote,
        para.list,
        hanging,
        para.heading > 0
    );
    if let Some(tag) = table.lookup(&name) {
        return tag;
    }
    let mut left = MARGIN + para.quote as i32 * 20;
    if para.list > 0 {
        left += (para.list as i32 - 1) * 24;
        // Text of an item that has no marker lines up with the others.
        if para.marker.is_none() {
            left += 20;
        }
    }
    let below = match (para.heading, para.list) {
        (0, 0) => 12,
        (0, _) => 4,
        _ => 6,
    };
    let tag = gtk::TextTag::builder()
        .name(name)
        .left_margin(left)
        .indent(-hanging)
        .pixels_below_lines(below)
        .build();
    table.add(&tag);
    tag
}

fn link_tag(table: &gtk::TextTagTable, index: usize, themer: &Themer) -> gtk::TextTag {
    let tag = gtk::TextTag::builder()
        .name(format!("link-{index}"))
        .underline(pango::Underline::Single)
        .build();
    tag.set_property("foreground-rgba", themer.colors().link);
    table.add(&tag);
    tag
}

/// The target of a link as shown in tooltips.
pub fn link_title(link: &Link) -> String {
    match link {
        Link::Anchor(id) => format!("#{id}"),
        Link::Help(name) => format!("help:{name}"),
        Link::Local { path, fragment } if fragment.is_empty() => path.display().to_string(),
        Link::Local { path, fragment } => format!("{}#{fragment}", path.display()),
        Link::External(url) => url.clone(),
        Link::Unsupported => String::new(),
    }
}

/// Draws blocks into the buffer of a view a few at a time, so that a big
/// document does not freeze the window.
pub struct Filler {
    blocks: std::vec::IntoIter<Block>,
    env: Env,
}

impl Filler {
    pub fn new(blocks: Vec<Block>, env: Env) -> Filler {
        Filler {
            blocks: blocks.into_iter(),
            env,
        }
    }

    /// Draws blocks until `budget` is spent. Returns the widgets anchored
    /// meanwhile, and whether everything is drawn.
    pub fn run(&mut self, view: &gtk::TextView, budget: Duration) -> (Vec<Anchored>, bool) {
        let start = Instant::now();
        let buffer = view.buffer();
        let mut anchored = Vec::new();
        for block in self.blocks.by_ref() {
            match block {
                Block::Text(para) => text(view, &buffer, &para, &self.env),
                Block::Code { lang, text } => {
                    let widget = code_block(&text, lang.as_deref(), &self.env.themer);
                    let _ = embed(view, &buffer, &widget);
                    anchored.push(Anchored {
                        widget: widget.upcast(),
                        fit: Fit::Width,
                    });
                }
                Block::Table { align, rows } => {
                    let (widget, grid) = table(&align, &rows, &self.env);
                    let anchor = embed(view, &buffer, &widget);
                    anchored.push(Anchored {
                        widget: widget.upcast(),
                        fit: Fit::Table(grid, Rc::new(RefCell::new(anchor))),
                    });
                }
                Block::Rule => {
                    let widget = gtk::Separator::builder().css_classes(["md-rule"]).build();
                    let _ = embed(view, &buffer, &widget);
                    anchored.push(Anchored {
                        widget: widget.upcast(),
                        fit: Fit::Width,
                    });
                }
                Block::Image { link, alt } => {
                    let image = image(&link, &alt, &self.env);
                    let _ = embed(view, &buffer, &image.widget);
                    anchored.push(image);
                }
            }
            if start.elapsed() >= budget {
                break;
            }
        }
        let done = self.blocks.len() == 0;
        (anchored, done)
    }
}

fn text(view: &gtk::TextView, buffer: &gtk::TextBuffer, para: &Para, env: &Env) {
    let table = buffer.tag_table();
    let mut end = buffer.end_iter();
    let start = end.offset();
    // Left gravity: the mark stays before the text inserted at its position.
    if let Some(id) = para.id.as_deref().filter(|id| buffer.mark(id).is_none()) {
        buffer.create_mark(Some(id), &end, true);
    }

    let mut marker_width = 0;
    if let Some(marker) = &para.marker {
        let marker = format!("{marker} ");
        marker_width = view.create_pango_layout(Some(&marker)).pixel_size().0;
        buffer.insert(&mut end, &marker);
    }
    for run in &para.runs {
        if let Some(name) = run
            .anchor
            .as_deref()
            .filter(|name| buffer.mark(name).is_none())
        {
            buffer.create_mark(Some(name), &end, true);
        }
        let mut tags = style_tags(&table, run.style);
        if let Some(link) = &run.link {
            let index = {
                let mut links = env.links.borrow_mut();
                links.push(link.clone());
                links.len() - 1
            };
            tags.push(link_tag(&table, index, &env.themer));
        }
        let tags: Vec<&gtk::TextTag> = tags.iter().collect();
        // A newline would end the paragraph; U+2028 only breaks the line.
        buffer.insert_with_tags(&mut end, &run.text.replace('\n', "\u{2028}"), &tags);
    }
    buffer.insert(&mut end, "\n");

    let from = buffer.iter_at_offset(start);
    let mut paragraph = vec![format_tag(&table, para, marker_width)];
    if para.heading > 0 {
        paragraph.extend(table.lookup(&format!("h{}", para.heading)));
    }
    if para.quote > 0 {
        paragraph.extend(table.lookup("quote"));
    }
    for tag in paragraph {
        buffer.apply_tag(&tag, &from, &end);
    }
}

fn style_tags(table: &gtk::TextTagTable, style: Style) -> Vec<gtk::TextTag> {
    [
        (Style::BOLD, "bold"),
        (Style::ITALIC, "italic"),
        (Style::STRIKE, "strike"),
        (Style::CODE, "code"),
        (Style::SUP, "sup"),
        (Style::SUB, "sub"),
    ]
    .iter()
    .filter(|(flag, _)| style.has(*flag))
    .filter_map(|(_, name)| table.lookup(name))
    .collect()
}

/// Puts a widget on a line of its own.
fn embed(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    widget: &impl IsA<gtk::Widget>,
) -> gtk::TextChildAnchor {
    let mut end = buffer.end_iter();
    let start = end.offset();
    let anchor = buffer.create_child_anchor(&mut end);
    buffer.insert(&mut end, "\n");
    view.add_child_at_anchor(widget, &anchor);
    if let Some(tag) = buffer.tag_table().lookup("block") {
        buffer.apply_tag(&tag, &buffer.iter_at_offset(start), &buffer.end_iter());
    }
    anchor
}

/// The viewport a scrolled window would make, without following the focus: a
/// click in selectable content gives it the focus, and the page would jump to
/// show the whole widget while the user is selecting.
pub fn viewport(child: &impl IsA<gtk::Widget>) -> gtk::Viewport {
    gtk::Viewport::builder()
        .scroll_to_focus(false)
        .child(child)
        .build()
}

/// A read-only block of code. Long lines scroll inside the block.
fn code_block(code: &str, lang: Option<&str>, themer: &Rc<Themer>) -> gtk::ScrolledWindow {
    #[cfg(feature = "highlight")]
    let view = crate::highlight::view(code, lang, themer);
    #[cfg(not(feature = "highlight"))]
    let view = {
        let _ = (lang, themer);
        let view = gtk::TextView::new();
        view.buffer().set_text(code);
        view
    };
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.set_left_margin(14);
    view.set_right_margin(14);
    view.set_top_margin(10);
    view.set_bottom_margin(10);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .css_classes(["code-block"])
        .child(&viewport(&view))
        .build()
}

/// Pango markup of runs. Links are `<a href="N">`, N indexing `Env::links`.
fn markup(runs: &[Run], env: &Env) -> String {
    let mut out = String::new();
    for run in runs {
        let mut piece = glib::markup_escape_text(&run.text).to_string();
        for (flag, tag) in [
            (Style::CODE, "tt"),
            (Style::BOLD, "b"),
            (Style::ITALIC, "i"),
            (Style::STRIKE, "s"),
            (Style::SUP, "sup"),
            (Style::SUB, "sub"),
        ] {
            if run.style.has(flag) {
                piece = format!("<{tag}>{piece}</{tag}>");
            }
        }
        if let Some(link) = &run.link {
            let index = {
                let mut links = env.links.borrow_mut();
                links.push(link.clone());
                links.len() - 1
            };
            let title = glib::markup_escape_text(&link_title(link));
            piece = format!("<a href=\"{index}\" title=\"{title}\">{piece}</a>");
        }
        out.push_str(&piece);
    }
    out
}

fn table(align: &[Align], rows: &[Vec<Vec<Run>>], env: &Env) -> (gtk::ScrolledWindow, gtk::Grid) {
    let grid = gtk::Grid::builder()
        .css_classes(["md-table"])
        .halign(gtk::Align::Start)
        // Not stretched to the height asked for, or it could never shrink.
        .valign(gtk::Align::Start)
        .build();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let xalign = match align.get(c) {
                Some(Align::Center) => 0.5,
                Some(Align::Right) => 1.0,
                _ => 0.0,
            };
            let class = match r {
                0 => "md-head",
                r if r % 2 == 0 => "md-odd",
                _ => "md-even",
            };
            let label = gtk::Label::builder()
                .use_markup(true)
                .label(markup(cell, env))
                .selectable(true)
                .wrap(true)
                .max_width_chars(60)
                .wrap_mode(pango::WrapMode::WordChar)
                .xalign(xalign)
                .css_classes(["md-cell", class])
                .build();
            // The label's own tooltip is the one of the link under the pointer.
            let links = env.links.clone();
            let activate = env.activate.clone();
            label.connect_activate_link(move |_, href| {
                let link = href
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| links.borrow().get(index).cloned());
                if let Some(link) = link {
                    activate(&link);
                }
                glib::Propagation::Stop
            });
            grid.attach(&label, c as i32, r as i32, 1, 1);
        }
    }
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::External)
        // `External`: with `Never` the height of the grid at its minimum width
        // would be the minimum height, and `settle` could not go below it.
        .child(&viewport(&grid))
        .build();
    (scrolled, grid)
}

/// A placeholder, replaced by the picture once it is loaded. A picture that
/// fails to load stays as its alt text.
fn image(link: &Link, alt: &str, env: &Env) -> Anchored {
    let natural = Rc::new(Cell::new((0, 0)));
    let holder = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Start)
        .build();
    let label = if alt.is_empty() { "image" } else { alt };
    let placeholder = gtk::Label::builder()
        .label(label)
        .css_classes(["dim-label"])
        .build();
    holder.append(&placeholder);
    if !alt.is_empty() {
        holder.set_tooltip_text(Some(alt));
    }

    let link = link.clone();
    let root = env.root.clone();
    let avail = env.avail.clone();
    let anchored = Anchored {
        widget: holder.clone().upcast(),
        fit: Fit::Image(natural.clone()),
    };
    let sizer = anchored.clone();
    glib::spawn_future_local(async move {
        let Some(texture) = load_image(&link, root).await else {
            return;
        };
        let picture = gtk::Picture::builder()
            .paintable(&texture)
            .can_shrink(true)
            .content_fit(gtk::ContentFit::ScaleDown)
            .build();
        holder.remove(&placeholder);
        holder.append(&picture);
        natural.set((texture.width(), texture.height()));
        sizer.resize(avail.get());
    });
    anchored
}

/// Local images stay inside the opened folder; remote ones must be `https`.
async fn load_image(link: &Link, root: PathBuf) -> Option<gdk::Texture> {
    let file = match link {
        Link::Local { path, .. } => {
            let path = path.to_string_lossy().into_owned();
            let resolved = gio::spawn_blocking(move || files::resolve_in_root(&root, &path))
                .await
                .ok()?
                .ok()?;
            gio::File::for_path(resolved)
        }
        Link::External(url) if url.to_ascii_lowercase().starts_with("https:") => {
            gio::File::for_uri(url)
        }
        _ => return None,
    };
    let (bytes, _) = file.load_bytes_future().await.ok()?;
    gdk::Texture::from_bytes(&bytes).ok()
}
