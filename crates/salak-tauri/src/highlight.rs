//! Syntax highlighting of fenced code blocks.
//!
//! Tokens are wrapped in `<span class="hl-…">` named after their TextMate
//! scopes (`hl-comment`, `hl-string`, `hl-keyword`…) rather than given inline
//! colors: the palette lives in `markdown.css`, so it follows the light and
//! dark themes and user style sheets can change it.

use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

/// Replaces the text of fenced code blocks in a known language with
/// highlighted HTML. Other code blocks are left untouched.
pub fn highlight(events: Vec<Event>) -> Vec<Event> {
    let mut out = Vec::with_capacity(events.len());
    let mut events = events.into_iter();
    while let Some(event) = events.next() {
        let syntax = match &event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => find_syntax(info),
            _ => None,
        };
        out.push(event);
        let Some(syntax) = syntax else { continue };

        let mut code = String::new();
        for event in events.by_ref() {
            match event {
                Event::Text(text) => code.push_str(&text),
                end @ Event::End(TagEnd::CodeBlock) => {
                    out.push(Event::Html(CowStr::from(to_html(&code, syntax))));
                    out.push(end);
                    break;
                }
                // Code blocks only contain text.
                other => out.push(other),
            }
        }
    }
    out
}

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Looks up the language of an info string such as `sh` or
/// `rust title="main.rs"`, by name or file extension.
fn find_syntax(info: &str) -> Option<&'static SyntaxReference> {
    let lang = info.split_whitespace().next()?;
    let lang = match lang.to_ascii_lowercase().as_str() {
        "shell" | "console" | "zsh" | "shell-session" => "bash",
        "jsonc" => "json",
        _ => lang,
    };
    syntaxes().find_syntax_by_token(lang)
}

fn to_html(code: &str, syntax: &SyntaxReference) -> String {
    let mut generator =
        ClassedHTMLGenerator::new_with_class_style(syntax, syntaxes(), CLASS_STYLE);
    for line in LinesWithEndings::from(code) {
        if generator.parse_html_for_line_which_includes_newline(line).is_err() {
            // A grammar failed on this input: fall back to plain text.
            return escape(code);
        }
    }
    generator.finalize()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_common_languages() {
        for lang in ["sh", "bash", "shell", "console", "rust", "rs", "yaml", "json", "py", "css"] {
            assert!(find_syntax(lang).is_some(), "{lang}");
        }
        assert!(find_syntax("rust title=\"main.rs\"").is_some());
        assert!(find_syntax("not-a-language").is_none());
        assert!(find_syntax("").is_none());
    }
}
