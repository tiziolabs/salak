//! Turns Markdown into a flat list of blocks.
//!
//! Pure Rust with no GTK type, so that the tests need no display (see D2 of
//! docs/gtk-migration-plan.md). `buffer.rs` draws the blocks.

use std::collections::HashMap;
use std::path::Path;

use salak_core::markdown::{self, resolve_link, Link};
use salak_core::pulldown_cmark::{Alignment, CodeBlockKind, Event, Tag, TagEnd};

/// Inline styles, as a bit set.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Style(u8);

impl Style {
    pub const BOLD: Style = Style(1);
    pub const ITALIC: Style = Style(2);
    pub const STRIKE: Style = Style(4);
    pub const CODE: Style = Style(8);
    pub const SUP: Style = Style(16);
    pub const SUB: Style = Style(32);

    pub fn has(self, flag: Style) -> bool {
        self.0 & flag.0 != 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// A piece of text with a single style.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Run {
    pub text: String,
    pub style: Style,
    pub link: Option<Link>,
    /// Name of a mark to put before the text (footnote references).
    pub anchor: Option<String>,
}

/// A paragraph, heading or list item, with the blocks it is nested in.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Para {
    pub runs: Vec<Run>,
    /// 1 to 6, 0 for a paragraph.
    pub heading: u8,
    /// Name of a mark to put at the start (heading ids, footnote definitions).
    pub id: Option<String>,
    /// Block quote depth.
    pub quote: u8,
    /// List depth.
    pub list: u8,
    /// Bullet or number of the list item this paragraph starts.
    pub marker: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Block {
    Text(Para),
    Code {
        lang: Option<String>,
        text: String,
    },
    /// The first row is the header.
    Table {
        align: Vec<Align>,
        rows: Vec<Vec<Vec<Run>>>,
    },
    Image {
        link: Link,
        alt: String,
    },
    Rule,
}

/// Lays out `markdown`. Relative links are relative to `base`, the folder of
/// the document, and `/`-rooted ones to `root`.
pub fn build(markdown: &str, base: &Path, root: &Path) -> Vec<Block> {
    let mut builder = Builder::new(base, root);
    for event in markdown::events(markdown) {
        builder.event(event);
    }
    builder.finish()
}

const BULLETS: [&str; 3] = ["•", "◦", "▪"];

#[derive(Default)]
struct Table {
    align: Vec<Align>,
    rows: Vec<Vec<Vec<Run>>>,
    row: Vec<Vec<Run>>,
}

struct Builder<'a> {
    base: &'a Path,
    root: &'a Path,
    blocks: Vec<Block>,
    /// Inline content of the paragraph or table cell being read.
    runs: Vec<Run>,
    heading: Option<(u8, Option<String>)>,
    quote: u8,
    /// Next number of each open list, `None` for bullets.
    lists: Vec<Option<u64>>,
    /// Marker of an item that has not got its first paragraph yet.
    marker: Option<String>,
    /// Open bold, italic, strike, code, sup and sub, by bit of `Style`.
    open: [u32; 6],
    links: Vec<Link>,
    image: Option<(Link, String)>,
    code: Option<(Option<String>, String)>,
    table: Option<Table>,
    footnote_numbers: HashMap<String, usize>,
    footnotes: Vec<(String, Vec<Block>)>,
    /// Blocks of the enclosing flow while a footnote definition is read.
    outer: Vec<(String, Vec<Block>)>,
    in_comment: bool,
}

impl<'a> Builder<'a> {
    fn new(base: &'a Path, root: &'a Path) -> Self {
        Builder {
            base,
            root,
            blocks: Vec::new(),
            runs: Vec::new(),
            heading: None,
            quote: 0,
            lists: Vec::new(),
            marker: None,
            open: [0; 6],
            links: Vec::new(),
            image: None,
            code: None,
            table: None,
            footnote_numbers: HashMap::new(),
            footnotes: Vec::new(),
            outer: Vec::new(),
            in_comment: false,
        }
    }

    fn style(&self) -> Style {
        Style(
            (0..6)
                .filter(|bit| self.open[*bit] > 0)
                .fold(0, |bits, bit| bits | 1 << bit),
        )
    }

    fn toggle(&mut self, style: Style, on: bool) {
        let bit = style.0.trailing_zeros() as usize;
        self.open[bit] = if on {
            self.open[bit] + 1
        } else {
            self.open[bit].saturating_sub(1)
        };
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => match (&mut self.image, &mut self.code) {
                (Some((_, alt)), _) => alt.push_str(&text),
                (_, Some((_, code))) => code.push_str(&text),
                _ => self.push(&text, self.style()),
            },
            Event::Code(text) => {
                let style = Style(self.style().0 | Style::CODE.0);
                self.push(&text, style);
            }
            Event::SoftBreak => self.push(" ", self.style()),
            Event::HardBreak => self.push("\n", self.style()),
            Event::Rule => {
                self.flush();
                self.flush_marker();
                self.blocks.push(Block::Rule);
            }
            Event::TaskListMarker(checked) => {
                if self.marker.is_some() {
                    self.marker = Some(if checked { "☑" } else { "☐" }.into());
                }
            }
            Event::FootnoteReference(name) => {
                let next = self.footnote_numbers.len() + 1;
                let first = !self.footnote_numbers.contains_key(&*name);
                let number = *self
                    .footnote_numbers
                    .entry(name.to_string())
                    .or_insert(next);
                self.runs.push(Run {
                    text: number.to_string(),
                    style: Style::SUP,
                    link: Some(Link::Anchor(format!("fn:{name}"))),
                    anchor: first.then(|| format!("fnref:{name}")),
                });
            }
            Event::Html(html) | Event::InlineHtml(html) => self.html(&html),
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph | Tag::HtmlBlock => {}
            Tag::Heading { level, id, .. } => {
                self.flush();
                self.heading = Some((level as u8, id.map(|id| id.to_string())));
            }
            Tag::BlockQuote => {
                self.flush();
                self.flush_marker();
                self.quote += 1;
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.flush_marker();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(str::to_string)
                    }
                    CodeBlockKind::Indented => None,
                };
                self.code = Some((lang, String::new()));
            }
            Tag::List(start) => {
                self.flush();
                self.flush_marker();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len();
                self.marker = Some(match self.lists.last_mut() {
                    Some(Some(next)) => {
                        let marker = format!("{next}.");
                        *next += 1;
                        marker
                    }
                    _ => BULLETS[depth.saturating_sub(1) % BULLETS.len()].into(),
                });
            }
            Tag::Table(align) => {
                self.flush();
                self.flush_marker();
                self.table = Some(Table {
                    align: align
                        .iter()
                        .map(|a| match a {
                            Alignment::None => Align::None,
                            Alignment::Left => Align::Left,
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                        })
                        .collect(),
                    ..Table::default()
                });
            }
            Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.toggle(Style::ITALIC, true),
            Tag::Strong => self.toggle(Style::BOLD, true),
            Tag::Strikethrough => self.toggle(Style::STRIKE, true),
            Tag::Link { dest_url, .. } => {
                let link = resolve_link(&dest_url, self.base, self.root);
                self.links.push(link);
            }
            Tag::Image { dest_url, .. } => {
                let link = resolve_link(&dest_url, self.base, self.root);
                // Inside a table cell the alt text stands in for the image.
                if self.table.is_none() {
                    self.flush();
                    self.flush_marker();
                }
                self.image = Some((link, String::new()));
            }
            Tag::FootnoteDefinition(name) => {
                self.flush();
                let outer = std::mem::take(&mut self.blocks);
                self.outer.push((name.to_string(), outer));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::HtmlBlock => self.flush(),
            TagEnd::BlockQuote => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some((lang, mut text)) = self.code.take() {
                    if text.ends_with('\n') {
                        text.pop();
                    }
                    self.blocks.push(Block::Code { lang, text });
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                self.flush_marker();
            }
            TagEnd::TableCell => {
                self.trim();
                let cell = std::mem::take(&mut self.runs);
                if let Some(table) = &mut self.table {
                    table.row.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = &mut self.table {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.blocks.push(Block::Table {
                        align: table.align,
                        rows: table.rows,
                    });
                }
            }
            TagEnd::Emphasis => self.toggle(Style::ITALIC, false),
            TagEnd::Strong => self.toggle(Style::BOLD, false),
            TagEnd::Strikethrough => self.toggle(Style::STRIKE, false),
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                if let Some((link, alt)) = self.image.take() {
                    if self.table.is_some() {
                        self.push(&alt, self.style());
                    } else {
                        self.blocks.push(Block::Image { link, alt });
                    }
                }
            }
            TagEnd::FootnoteDefinition => {
                self.flush();
                if let Some((name, outer)) = self.outer.pop() {
                    let blocks = std::mem::replace(&mut self.blocks, outer);
                    self.footnotes.push((name, blocks));
                }
            }
            _ => {}
        }
    }

    /// Appends text to the current paragraph, merging it with the previous
    /// run when nothing differs.
    fn push(&mut self, text: &str, style: Style) {
        if text.is_empty() {
            return;
        }
        let link = self.links.last().cloned();
        if let Some(last) = self.runs.last_mut() {
            if last.style == style && last.link == link {
                last.text.push_str(text);
                return;
            }
        }
        self.runs.push(Run {
            text: text.to_string(),
            style,
            link,
            anchor: None,
        });
    }

    /// Drops the spaces around the inline content, left by soft breaks and
    /// raw HTML.
    fn trim(&mut self) {
        if let Some(first) = self.runs.first_mut() {
            first.text = first.text.trim_start_matches([' ', '\n']).to_string();
        }
        if let Some(last) = self.runs.last_mut() {
            last.text = last.text.trim_end_matches([' ', '\n']).to_string();
        }
        self.runs
            .retain(|run| !run.text.is_empty() || run.anchor.is_some());
    }

    /// Ends the paragraph being read.
    fn flush(&mut self) {
        let heading = self.heading.take();
        if self.table.is_some() {
            return;
        }
        self.trim();
        if self.runs.is_empty() {
            return;
        }
        let (level, id) = heading.unwrap_or((0, None));
        self.blocks.push(Block::Text(Para {
            runs: std::mem::take(&mut self.runs),
            heading: level,
            id,
            quote: self.quote,
            list: self.lists.len() as u8,
            marker: self.marker.take(),
        }));
    }

    /// An item that starts with something else than text still shows its
    /// bullet.
    fn flush_marker(&mut self) {
        if let Some(marker) = self.marker.take() {
            self.blocks.push(Block::Text(Para {
                quote: self.quote,
                list: self.lists.len() as u8,
                marker: Some(marker),
                ..Para::default()
            }));
        }
    }

    /// Raw HTML: only `<br>`, `<kbd>`, `<sup>` and `<sub>` are interpreted.
    /// Other tags are dropped and the text between them is kept.
    fn html(&mut self, mut rest: &str) {
        loop {
            if self.in_comment {
                match rest.find("-->") {
                    Some(end) => {
                        rest = &rest[end + 3..];
                        self.in_comment = false;
                    }
                    None => return,
                }
            }
            let Some(open) = rest.find('<') else {
                return self.html_text(rest);
            };
            self.html_text(&rest[..open]);
            let tail = &rest[open..];
            if let Some(comment) = tail.strip_prefix("<!--") {
                self.in_comment = true;
                rest = comment;
            } else if let Some(close) = tail.find('>') {
                self.html_tag(&tail[1..close]);
                rest = &tail[close + 1..];
            } else {
                return self.html_text(tail);
            }
        }
    }

    fn html_text(&mut self, text: &str) {
        let text = text
            .replace('\n', " ")
            .replace("&nbsp;", "\u{a0}")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&");
        let text = if self.runs.is_empty() {
            text.trim_start()
        } else {
            &text
        };
        self.push(text, self.style());
    }

    fn html_tag(&mut self, tag: &str) {
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        match name.as_str() {
            "br" => self.push("\n", self.style()),
            "kbd" => self.toggle(Style::CODE, !closing),
            "sup" => self.toggle(Style::SUP, !closing),
            "sub" => self.toggle(Style::SUB, !closing),
            // Elements that stand on their own line.
            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "tr" | "ul" | "ol"
            | "pre" | "blockquote" | "table" => self.flush(),
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<Block> {
        self.flush();
        self.flush_marker();
        if self.footnotes.is_empty() {
            return self.blocks;
        }

        // Numbered in order of reference, as on GitHub.
        let mut next = self.footnote_numbers.len();
        let mut definitions = Vec::new();
        for (name, blocks) in std::mem::take(&mut self.footnotes) {
            let number = *self
                .footnote_numbers
                .entry(name.clone())
                .or_insert_with(|| {
                    next += 1;
                    next
                });
            definitions.push((number, name, blocks));
        }
        definitions.sort_by_key(|(number, ..)| *number);

        self.blocks.push(Block::Rule);
        for (number, name, mut blocks) in definitions {
            for block in &mut blocks {
                if let Block::Text(para) = block {
                    para.list = para.list.max(1);
                }
            }
            if !matches!(blocks.first(), Some(Block::Text(_))) {
                blocks.insert(
                    0,
                    Block::Text(Para {
                        list: 1,
                        ..Para::default()
                    }),
                );
            }
            if let Some(Block::Text(first)) = blocks.first_mut() {
                first.marker = Some(format!("{number}."));
                first.id = Some(format!("fn:{name}"));
            }
            if let Some(Block::Text(last)) = blocks
                .iter_mut()
                .rev()
                .find(|block| matches!(block, Block::Text(_)))
            {
                last.runs.push(Run {
                    text: " ↩".into(),
                    style: Style::default(),
                    link: Some(Link::Anchor(format!("fnref:{name}"))),
                    anchor: None,
                });
            }
            self.blocks.extend(blocks);
        }
        self.blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(markdown: &str) -> Vec<Block> {
        build(
            markdown,
            Path::new("/nonexistent/docs"),
            Path::new("/nonexistent"),
        )
    }

    fn paras(markdown: &str) -> Vec<Para> {
        layout(markdown)
            .into_iter()
            .filter_map(|block| match block {
                Block::Text(para) => Some(para),
                _ => None,
            })
            .collect()
    }

    fn text(para: &Para) -> String {
        para.runs.iter().map(|run| run.text.as_str()).collect()
    }

    #[test]
    fn emphasis_makes_runs() {
        let blocks = layout("a *b*");
        assert_eq!(blocks.len(), 1);
        let Block::Text(para) = &blocks[0] else {
            panic!("not a paragraph");
        };
        assert_eq!(para.runs.len(), 2);
        assert_eq!(para.runs[0].text, "a ");
        assert_eq!(para.runs[0].style, Style::default());
        assert_eq!(para.runs[1].text, "b");
        assert!(para.runs[1].style.has(Style::ITALIC));
    }

    #[test]
    fn styles_nest_and_close() {
        let para = &paras("**a *b* c** `d` ~~e~~")[0];
        let styles: Vec<_> = para.runs.iter().map(|run| run.style).collect();
        assert!(styles[0].has(Style::BOLD) && !styles[0].has(Style::ITALIC));
        assert!(styles[1].has(Style::BOLD) && styles[1].has(Style::ITALIC));
        assert!(styles[2].has(Style::BOLD) && !styles[2].has(Style::ITALIC));
        assert!(styles[4].has(Style::CODE) && !styles[4].has(Style::BOLD));
        assert!(styles[6].has(Style::STRIKE));
    }

    #[test]
    fn headings_have_unique_ids() {
        let paras = paras("# Hello World\n\n## Hello World\n\ntext");
        assert_eq!(paras[0].heading, 1);
        assert_eq!(paras[0].id.as_deref(), Some("hello-world"));
        assert_eq!(paras[1].heading, 2);
        assert_eq!(paras[1].id.as_deref(), Some("hello-world-1"));
        assert_eq!(paras[2].heading, 0);
    }

    #[test]
    fn soft_and_hard_breaks() {
        let para = &paras("a\nb  \nc")[0];
        assert_eq!(text(para), "a b\nc");
    }

    #[test]
    fn bullets_follow_the_depth() {
        let markers: Vec<_> = paras("- a\n  - b\n    - c\n      - d")
            .iter()
            .map(|para| (para.list, para.marker.clone().unwrap()))
            .collect();
        assert_eq!(
            markers,
            [
                (1, "•".into()),
                (2, "◦".into()),
                (3, "▪".into()),
                (4, "•".into())
            ]
        );
    }

    #[test]
    fn ordered_lists_keep_their_start() {
        let markers: Vec<_> = paras("3. a\n4. b\n\n- c")
            .iter()
            .map(|para| para.marker.clone().unwrap())
            .collect();
        assert_eq!(markers, ["3.", "4.", "•"]);
    }

    #[test]
    fn loose_items_and_continuations() {
        let paras = paras("- a\n\n  more\n\n- b");
        assert_eq!(paras[0].marker.as_deref(), Some("•"));
        assert_eq!(paras[1].marker, None);
        assert_eq!(paras[1].list, 1);
        assert_eq!(paras[2].marker.as_deref(), Some("•"));
    }

    #[test]
    fn task_lists_replace_the_bullet() {
        let markers: Vec<_> = paras("- [ ] a\n- [x] b")
            .iter()
            .map(|para| para.marker.clone().unwrap())
            .collect();
        assert_eq!(markers, ["☐", "☑"]);
    }

    #[test]
    fn lists_in_quotes_and_quotes_in_lists() {
        let in_quote = &paras("> - a")[0];
        assert_eq!((in_quote.quote, in_quote.list), (1, 1));
        let in_list = &paras("- a\n\n  > b\n\n  > > c")[1..];
        assert_eq!((in_list[0].quote, in_list[0].list), (1, 1));
        assert_eq!((in_list[1].quote, in_list[1].list), (2, 1));
        let after = &paras("> a\n\nb")[1];
        assert_eq!(after.quote, 0);
    }

    #[test]
    fn an_item_starting_with_code_keeps_its_bullet() {
        let blocks = layout("- ```\n  x\n  ```");
        assert!(
            matches!(&blocks[0], Block::Text(para) if para.runs.is_empty()
            && para.marker.as_deref() == Some("•"))
        );
        assert!(matches!(&blocks[1], Block::Code { .. }));
    }

    #[test]
    fn code_blocks_keep_the_first_word_of_the_info_string() {
        let blocks = layout("```rust title=x\nfn main() {}\n```\n\n    indented\n");
        assert_eq!(
            blocks[0],
            Block::Code {
                lang: Some("rust".into()),
                text: "fn main() {}".into()
            }
        );
        assert_eq!(
            blocks[1],
            Block::Code {
                lang: None,
                text: "indented".into()
            }
        );
    }

    #[test]
    fn tables_keep_alignment_and_inline_content() {
        let blocks = layout("| a | `b` |\n|:-:|--:|\n| [c](#d) | e |");
        let Block::Table { align, rows } = &blocks[0] else {
            panic!("not a table");
        };
        assert_eq!(align, &[Align::Center, Align::Right]);
        assert_eq!(rows.len(), 2);
        assert!(rows[0][1][0].style.has(Style::CODE));
        assert_eq!(rows[1][0][0].link, Some(Link::Anchor("d".into())));
    }

    #[test]
    fn links_are_resolved() {
        let para = &paras("[a](#x%20y) [b](other.md#s) [c](https://e.org) [d](javascript:1)")[0];
        let links: Vec<_> = para
            .runs
            .iter()
            .filter_map(|run| run.link.clone())
            .collect();
        assert_eq!(links[0], Link::Anchor("x y".into()));
        assert_eq!(
            links[1],
            Link::Local {
                path: "/nonexistent/docs/other.md".into(),
                fragment: "s".into()
            }
        );
        assert_eq!(links[2], Link::External("https://e.org".into()));
        assert_eq!(links[3], Link::Unsupported);
    }

    #[test]
    fn images_are_blocks_and_split_the_paragraph() {
        let blocks = layout("before ![the *alt*](pic.png) after");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], Block::Text(para) if text(para) == "before"));
        assert!(matches!(&blocks[1], Block::Image { alt, .. } if alt == "the alt"));
        assert!(matches!(&blocks[2], Block::Text(para) if text(para) == "after"));
    }

    #[test]
    fn images_in_tables_become_their_alt_text() {
        let blocks = layout("| a |\n|---|\n| ![pic](p.png) |");
        let Block::Table { rows, .. } = &blocks[0] else {
            panic!("not a table");
        };
        assert_eq!(rows[1][0][0].text, "pic");
    }

    #[test]
    fn footnotes_go_to_the_end_in_order_of_reference() {
        let blocks = layout("a[^b] c[^a]\n\n[^a]: first\n\n[^b]: second\n\nlast");
        let Block::Text(body) = &blocks[0] else {
            panic!("not a paragraph");
        };
        let reference = &body.runs[1];
        assert_eq!(reference.text, "1");
        assert!(reference.style.has(Style::SUP));
        assert_eq!(reference.link, Some(Link::Anchor("fn:b".into())));
        assert_eq!(reference.anchor.as_deref(), Some("fnref:b"));

        assert!(matches!(&blocks[1], Block::Text(para) if text(para) == "last"));
        assert_eq!(blocks[2], Block::Rule);
        let Block::Text(first) = &blocks[3] else {
            panic!("no definition");
        };
        assert_eq!(first.marker.as_deref(), Some("1."));
        assert_eq!(first.id.as_deref(), Some("fn:b"));
        assert_eq!(text(first), "second ↩");
        let back = first.runs.last().unwrap();
        assert_eq!(back.link, Some(Link::Anchor("fnref:b".into())));
        let Block::Text(second) = &blocks[4] else {
            panic!("no definition");
        };
        assert_eq!(second.marker.as_deref(), Some("2."));
    }

    #[test]
    fn rules() {
        assert_eq!(layout("a\n\n---\n\nb")[1], Block::Rule);
    }

    #[test]
    fn html_line_breaks_and_comments() {
        assert_eq!(text(&paras("a<br>b<br/>c")[0]), "a\nb\nc");
        assert_eq!(text(&paras("a<!-- hidden -->b")[0]), "ab");
        let multi = paras("<!--\nhidden\n-->\n\nshown");
        assert_eq!(multi.len(), 1);
        assert_eq!(text(&multi[0]), "shown");
    }

    #[test]
    fn html_inline_tags() {
        let para = &paras("<kbd>Ctrl</kbd>+x H<sub>2</sub>O x<sup>2</sup>")[0];
        assert!(para.runs[0].style.has(Style::CODE));
        assert_eq!(para.runs[0].text, "Ctrl");
        assert!(!para.runs[1].style.has(Style::CODE));
        assert!(para
            .runs
            .iter()
            .any(|r| r.style.has(Style::SUB) && r.text == "2"));
        assert!(para
            .runs
            .iter()
            .any(|r| r.style.has(Style::SUP) && r.text == "2"));
    }

    #[test]
    fn other_html_is_dropped_but_its_text_is_kept() {
        let paras = paras("<p align=\"center\">\n<b>bold</b> &amp; <a href=\"x\">link</a>\n</p>\n\n<div>one</div><div>two</div>");
        let all: Vec<_> = paras.iter().map(text).collect();
        assert_eq!(all, ["bold & link", "one", "two"]);
    }
}
