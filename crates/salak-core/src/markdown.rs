//! Markdown parsing and link resolution, shared by the frontends.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::files;

pub const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS);

/// Parses `markdown`, giving the headings their ids.
pub fn events(markdown: &str) -> Vec<Event<'_>> {
    let mut events: Vec<Event> = Parser::new_ext(markdown, OPTIONS).collect();
    add_heading_ids(&mut events);
    events
}

/// Where a link or an image points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// `#section`, percent-decoded, without the `#`.
    Anchor(String),
    /// `help:<name>`.
    Help(String),
    /// A file of the computer, with its percent-decoded `#fragment` (without
    /// the `#`, empty if none).
    Local { path: PathBuf, fragment: String },
    /// `http`, `https` and `mailto`: opened in the default application.
    External(String),
    /// Any other scheme, or an empty or protocol-relative URL.
    Unsupported,
}

/// Resolves a URL found in the document `base` is the folder of. A relative
/// URL (`../doc.md#usage`) is relative to `base`; a leading `/`
/// (`/img/logo.png`) means the opened folder `root`, as on forges.
pub fn resolve_link(url: &str, base: &Path, root: &Path) -> Link {
    if let Some(fragment) = url.strip_prefix('#') {
        return Link::Anchor(percent_decode(fragment));
    }
    if url.is_empty() || url.starts_with("//") {
        return Link::Unsupported;
    }
    if has_scheme(url) {
        let scheme = &url[..url.find(':').unwrap_or_default()];
        return match scheme.to_ascii_lowercase().as_str() {
            "help" => Link::Help(url["help:".len()..].to_string()),
            "http" | "https" | "mailto" => Link::External(url.to_string()),
            _ => Link::Unsupported,
        };
    }
    let (rest, fragment) = url.find('#').map_or((url, ""), |i| url.split_at(i));
    let path = rest.split('?').next().unwrap_or(rest);
    let path = percent_decode(path);
    if path.is_empty() {
        return Link::Unsupported;
    }
    let path = match path.strip_prefix('/') {
        Some(from_root) => root.join(from_root),
        None => base.join(path),
    };
    Link::Local {
        path: crate::canonicalize(&path).unwrap_or(path),
        fragment: percent_decode(fragment.trim_start_matches('#')),
    }
}

/// Whether a local link is opened in a tab: only Markdown files are.
pub fn is_openable(path: &Path) -> bool {
    files::is_markdown(path)
}

fn has_scheme(url: &str) -> bool {
    match url.find(':') {
        // A single letter is a Windows drive (`C:`), not a scheme.
        Some(i) if i > 1 => {
            let scheme = &url[..i];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }
        _ => false,
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Gives headings GitHub-like ids so that `#section` links work.
fn add_heading_ids(events: &mut [Event]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for i in 0..events.len() {
        if !matches!(events[i], Event::Start(Tag::Heading { id: None, .. })) {
            continue;
        }
        let mut text = String::new();
        for event in &events[i + 1..] {
            match event {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Text(t) | Event::Code(t) => text.push_str(t),
                _ => {}
            }
        }
        let slug = slugify(&text);
        let count = seen.entry(slug.clone()).or_insert(0);
        let unique = if *count == 0 {
            slug
        } else {
            format!("{slug}-{count}")
        };
        *count += 1;
        if let Event::Start(Tag::Heading { id, .. }) = &mut events[i] {
            *id = Some(CowStr::from(unique));
        }
    }
}

pub fn slugify(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            c if c.is_whitespace() => Some('-'),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading_ids(markdown: &str) -> Vec<String> {
        events(markdown)
            .into_iter()
            .filter_map(|event| match event {
                Event::Start(Tag::Heading { id, .. }) => id.map(|id| id.to_string()),
                _ => None,
            })
            .collect()
    }

    fn resolve(url: &str) -> Link {
        // Nothing exists in these folders, so paths are only joined.
        resolve_link(url, Path::new("/notes/sub"), Path::new("/notes"))
    }

    fn local(path: &str, fragment: &str) -> Link {
        Link::Local {
            path: PathBuf::from(path),
            fragment: fragment.into(),
        }
    }

    #[test]
    fn heading_ids_are_unique() {
        assert_eq!(
            heading_ids("# Hello World\n## Hello World\n"),
            ["hello-world", "hello-world-1"]
        );
        assert_eq!(heading_ids("# `Code` & more!\n"), ["code--more"]);
    }

    #[test]
    fn detects_schemes() {
        assert!(has_scheme("https://example.org"));
        assert!(has_scheme("mailto:a@b.c"));
        assert!(!has_scheme("C:/doc.md"));
        assert!(!has_scheme("docs/a.md"));
    }

    #[test]
    fn resolves_relative_and_rooted_paths() {
        assert_eq!(resolve("a.md"), local("/notes/sub/a.md", ""));
        assert_eq!(
            resolve("../b.md#usage"),
            local("/notes/sub/../b.md", "usage")
        );
        assert_eq!(resolve("/img/logo.png"), local("/notes/img/logo.png", ""));
        assert_eq!(resolve("a.md?raw=1"), local("/notes/sub/a.md", ""));
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(
            resolve("other%20file.md#h%C3%A9"),
            local("/notes/sub/other file.md", "hé")
        );
        assert_eq!(resolve("#a%20b"), Link::Anchor("a b".into()));
        // Not an escape: kept as is.
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn resolves_fragments_and_special_schemes() {
        assert_eq!(resolve("#usage"), Link::Anchor("usage".into()));
        assert_eq!(resolve("help:theming"), Link::Help("theming".into()));
        assert_eq!(
            resolve("https://example.org/#x"),
            Link::External("https://example.org/#x".into())
        );
        assert_eq!(
            resolve("mailto:a@b.c"),
            Link::External("mailto:a@b.c".into())
        );
        assert_eq!(resolve("javascript:alert(1)"), Link::Unsupported);
        assert_eq!(resolve("file:///etc/passwd"), Link::Unsupported);
        assert_eq!(resolve("//example.org/a"), Link::Unsupported);
        assert_eq!(resolve(""), Link::Unsupported);
        assert_eq!(resolve("?q=1"), Link::Unsupported);
    }

    #[test]
    fn windows_drive_is_a_path() {
        assert!(matches!(resolve("C:/doc.md"), Link::Local { .. }));
    }

    #[test]
    fn only_markdown_is_openable() {
        assert!(is_openable(Path::new("a/B.MD")));
        assert!(!is_openable(Path::new("a/logo.png")));
    }
}
