//! Markdown to sanitized HTML.
//!
//! Local links and images are rewritten to absolute locations so that the
//! frontend does not have to resolve paths:
//! - links to local files become `salak:<percent-encoded absolute path>`,
//! - images become URLs of Tauri's asset protocol.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

#[cfg(windows)]
const ASSET_PREFIX: &str = "http://asset.localhost/";
#[cfg(not(windows))]
const ASSET_PREFIX: &str = "asset://localhost/";

pub fn render(markdown: &str, file: &Path, root: &Path) -> String {
    let base = file.parent().unwrap_or(root);
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    let mut events: Vec<Event> = Parser::new_ext(markdown, options)
        .map(|event| rewrite_urls(event, base, root))
        .collect();
    add_heading_ids(&mut events);
    #[cfg(feature = "highlight")]
    let events = crate::highlight::highlight(events);

    let mut out = String::with_capacity(markdown.len() * 3 / 2);
    html::push_html(&mut out, events.into_iter());
    sanitizer().clean(&out).to_string()
}

fn rewrite_urls<'a>(event: Event<'a>, base: &Path, root: &Path) -> Event<'a> {
    match event {
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            let dest_url = match local_target(&dest_url, base, root) {
                Some((path, fragment)) => format!("salak:{}{fragment}", encode(&path)).into(),
                None => dest_url,
            };
            Event::Start(Tag::Link { link_type, dest_url, title, id })
        }
        Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
            let dest_url = match local_target(&dest_url, base, root) {
                Some((path, _)) => format!("{ASSET_PREFIX}{}", encode(&path)).into(),
                None => dest_url,
            };
            Event::Start(Tag::Image { link_type, dest_url, title, id })
        }
        other => other,
    }
}

/// Resolves a relative URL (`../doc.md#usage`, `/img/logo.png`) to an
/// absolute path and its `#fragment`. Returns `None` for URLs with a scheme
/// and for pure fragments. Leading `/` means the opened folder, as on forges.
fn local_target<'u>(url: &'u str, base: &Path, root: &Path) -> Option<(PathBuf, &'u str)> {
    if url.is_empty() || url.starts_with('#') || url.starts_with("//") || has_scheme(url) {
        return None;
    }
    let (rest, fragment) = url.find('#').map_or((url, ""), |i| url.split_at(i));
    let path = rest.split('?').next().unwrap_or(rest);
    let path = percent_decode(path);
    if path.is_empty() {
        return None;
    }
    let path = match path.strip_prefix('/') {
        Some(from_root) => root.join(from_root),
        None => base.join(path),
    };
    Some((path.canonicalize().unwrap_or(path), fragment))
}

fn has_scheme(url: &str) -> bool {
    match url.find(':') {
        // A single letter is a Windows drive (`C:`), not a scheme.
        Some(i) if i > 1 => {
            let scheme = &url[..i];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }
        _ => false,
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
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

/// Same as JavaScript's `encodeURIComponent`.
fn encode(path: &Path) -> String {
    let mut out = String::new();
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
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
        let unique = if *count == 0 { slug } else { format!("{slug}-{count}") };
        *count += 1;
        if let Event::Start(Tag::Heading { id, .. }) = &mut events[i] {
            *id = Some(CowStr::from(unique));
        }
    }
}

fn slugify(text: &str) -> String {
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

/// Markdown files may embed raw HTML: everything is filtered through an
/// allow-list, on top of the CSP.
fn sanitizer() -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::default();
    builder
        .add_url_schemes(&["salak", "asset", "help"])
        // Task lists.
        .add_tags(&["input"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        // Anchors and footnotes.
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("h5", &["id"])
        .add_tag_attributes("h6", &["id"])
        .add_tag_attributes("div", &["id", "class"])
        .add_tag_attributes("sup", &["class"])
        // Highlighted tokens.
        .add_tag_attributes("span", &["class"])
        // `language-xxx`, for syntax highlighting.
        .add_tag_attributes("code", &["class"])
        // Column alignment.
        .add_tag_attributes("th", &["style"])
        .add_tag_attributes("td", &["style"])
        .attribute_filter(|element, attribute, value| match (element, attribute) {
            ("input", "type") if value != "checkbox" => None,
            ("th" | "td", "style") if !is_text_align(value) => None,
            _ => Some(Cow::Owned(value.to_owned())),
        });
    builder
}

fn is_text_align(style: &str) -> bool {
    matches!(
        style.trim().trim_end_matches(';'),
        "text-align: left" | "text-align: center" | "text-align: right"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_in(markdown: &str) -> String {
        let root = std::env::temp_dir();
        render(markdown, &root.join("doc.md"), &root)
    }

    #[test]
    fn strips_scripts() {
        let html = render_in("hello <script>alert(1)</script> <img src=x onerror=alert(1)>");
        assert!(!html.contains("script"));
        assert!(!html.contains("onerror"));
    }

    #[test]
    fn rewrites_local_links_and_images() {
        let html = render_in("[doc](other%20file.md#usage) ![img](img/a.png) [web](https://example.org)");
        assert!(html.contains("href=\"salak:"), "{html}");
        assert!(html.contains("other%20file.md#usage"), "{html}");
        assert!(html.contains(&format!("src=\"{ASSET_PREFIX}")), "{html}");
        assert!(html.contains("href=\"https://example.org\""), "{html}");
    }

    #[test]
    fn heading_ids_are_unique() {
        let html = render_in("# Hello World\n## Hello World\n");
        assert!(html.contains("id=\"hello-world\""), "{html}");
        assert!(html.contains("id=\"hello-world-1\""), "{html}");
    }

    #[test]
    fn keeps_task_lists_and_alignment() {
        let html = render_in("- [x] done\n\n| a |\n|:-:|\n| b |\n");
        assert!(html.contains("type=\"checkbox\""), "{html}");
        assert!(html.contains("text-align: center"), "{html}");
    }

    #[cfg(feature = "highlight")]
    #[test]
    fn highlights_known_languages_only() {
        let html = render_in("```sh\necho \"<hi>\" # note\n```\n\n```nope\n<b>x</b>\n```\n");
        assert!(html.contains("<code class=\"language-sh\">"), "{html}");
        assert!(html.contains("<span class=\"hl-comment"), "{html}");
        assert!(html.contains("&lt;hi&gt;"), "{html}");
        assert!(html.contains("<code class=\"language-nope\">&lt;b&gt;x&lt;/b&gt;"), "{html}");
    }

    #[test]
    fn keeps_help_links() {
        let html = render_in("[guide](help:theming)");
        assert!(html.contains("href=\"help:theming\""), "{html}");
    }

    #[test]
    fn detects_schemes() {
        assert!(has_scheme("https://example.org"));
        assert!(has_scheme("mailto:a@b.c"));
        assert!(!has_scheme("C:/doc.md"));
        assert!(!has_scheme("docs/a.md"));
    }
}
