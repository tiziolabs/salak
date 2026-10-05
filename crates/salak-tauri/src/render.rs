//! Markdown to sanitized HTML.
//!
//! Local links and images are rewritten to absolute locations so that the
//! frontend does not have to resolve paths:
//! - links to local files become `salak:<percent-encoded absolute path>`,
//! - images become URLs of Tauri's asset protocol.

use std::borrow::Cow;
use std::path::Path;

use pulldown_cmark::{html, Event, Tag};
use salak_core::markdown::{self, Link};

#[cfg(windows)]
const ASSET_PREFIX: &str = "http://asset.localhost/";
#[cfg(not(windows))]
const ASSET_PREFIX: &str = "asset://localhost/";

pub fn render(markdown: &str, file: &Path, root: &Path) -> String {
    let base = file.parent().unwrap_or(root);
    let events: Vec<Event> = markdown::events(markdown)
        .into_iter()
        .map(|event| rewrite_urls(event, base, root))
        .collect();
    #[cfg(feature = "highlight")]
    let events = crate::highlight::highlight(events);

    let mut out = String::with_capacity(markdown.len() * 3 / 2);
    html::push_html(&mut out, events.into_iter());
    sanitizer().clean(&out).to_string()
}

fn rewrite_urls<'a>(event: Event<'a>, base: &Path, root: &Path) -> Event<'a> {
    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = match markdown::resolve_link(&dest_url, base, root) {
                Link::Local { path, fragment } => {
                    let fragment = if fragment.is_empty() {
                        String::new()
                    } else {
                        format!("#{}", encode(&fragment))
                    };
                    format!("salak:{}{fragment}", encode(&path.to_string_lossy())).into()
                }
                _ => dest_url,
            };
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = match markdown::resolve_link(&dest_url, base, root) {
                Link::Local { path, .. } => {
                    format!("{ASSET_PREFIX}{}", encode(&path.to_string_lossy())).into()
                }
                _ => dest_url,
            };
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        other => other,
    }
}

/// Same as JavaScript's `encodeURIComponent`.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
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
        let html =
            render_in("[doc](other%20file.md#usage) ![img](img/a.png) [web](https://example.org)");
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
    fn keeps_anchors_and_unsupported_links() {
        let html = render_in("[a](#usage) [b](javascript:alert(1))");
        assert!(html.contains("href=\"#usage\""), "{html}");
        assert!(!html.contains("javascript"), "{html}");
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
        assert!(
            html.contains("<code class=\"language-nope\">&lt;b&gt;x&lt;/b&gt;"),
            "{html}"
        );
    }

    #[test]
    fn keeps_help_links() {
        let html = render_in("[guide](help:theming)");
        assert!(html.contains("href=\"help:theming\""), "{html}");
    }
}
