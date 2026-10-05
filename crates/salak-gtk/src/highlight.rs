//! Syntax highlighting of code blocks with GtkSourceView.

use std::rc::Rc;

use gtk::prelude::*;
use sourceview5::prelude::*;
use sourceview5::{Buffer, Language, LanguageManager, StyleSchemeManager};

use crate::theme::Themer;

/// The language of a fenced block, from the first word of its info string.
/// Without one, or with a name GtkSourceView does not know, the code stays
/// plain.
fn language(lang: &str) -> Option<Language> {
    let id = alias(lang);
    let manager = LanguageManager::default();
    // The name may be an id (`rust`) or an extension (`rs`, `py`).
    manager
        .language(&id)
        .or_else(|| manager.guess_language(Some(&format!("x.{id}")), None))
        .or_else(|| manager.guess_language(Some(&format!("x.{lang}")), None))
}

/// The names that fenced blocks use for languages GtkSourceView calls
/// otherwise.
fn alias(lang: &str) -> String {
    let lang = lang.to_ascii_lowercase();
    match lang.as_str() {
        "shell" | "console" | "zsh" | "shell-session" | "bash" => "sh",
        "jsonc" => "json",
        "rs" => "rust",
        "py" => "python",
        "yml" => "yaml",
        other => other,
    }
    .to_string()
}

/// A read-only view of `code`, highlighted when its language is known, with
/// the style scheme of the theme and the mode.
pub fn view(code: &str, lang: Option<&str>, themer: &Rc<Themer>) -> gtk::TextView {
    let buffer = Buffer::new(None);
    buffer.set_text(code);
    if let Some(language) = lang.and_then(language) {
        buffer.set_language(Some(&language));
        buffer.set_highlight_syntax(true);
        // Only highlighted code follows a scheme; plain code keeps the text
        // colour of the document.
        let weak = buffer.downgrade();
        themer.observe(move |themer| {
            let Some(buffer) = weak.upgrade() else {
                return false;
            };
            buffer.set_style_scheme(scheme(themer).as_ref());
            true
        });
    }
    // Neither the line under the cursor nor line numbers: it is not an editor.
    sourceview5::View::with_buffer(&buffer).upcast()
}

fn scheme(themer: &Themer) -> Option<sourceview5::StyleScheme> {
    let manager = StyleSchemeManager::default();
    let default = if themer.dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };
    themer
        .code_scheme()
        .and_then(|id| manager.scheme(&id))
        .or_else(|| manager.scheme(default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_fenced_blocks_become_ids() {
        assert_eq!(alias("Shell"), "sh");
        assert_eq!(alias("console"), "sh");
        assert_eq!(alias("rs"), "rust");
        assert_eq!(alias("yml"), "yaml");
        assert_eq!(alias("css"), "css");
    }
}
