//! Turns a theme file into CSS for the shadow root of the document.

use salak_core::theme::{Palette, Theme};

/// CSS variables of `markdown.css` set by `theme`.
///
/// The light palette is only applied when the system is not dark, rather
/// than on `:host` alone: it comes after the default dark palette of
/// `markdown.css` and would otherwise win over it.
pub fn from_theme(theme: &Theme) -> String {
    let mut layout = Vec::new();
    if let Some(font) = theme.font.as_deref().and_then(quote) {
        layout.push(format!("--font: {font}, sans-serif;"));
    }
    if let Some(font) = theme.mono_font.as_deref().and_then(quote) {
        layout.push(format!("--mono-font: {font}, monospace;"));
    }
    if let Some(size) = theme.font_size {
        layout.push(format!("--font-size: {size}px;"));
    }
    if let Some(width) = theme.max_width {
        layout.push(format!("--max-width: {width}ch;"));
    }
    let mut css = String::new();
    if !layout.is_empty() {
        css.push_str(&format!(":host {{ {} }}\n", layout.join(" ")));
    }
    css.push_str(&palette_css(
        &theme.light,
        "not all and (prefers-color-scheme: dark)",
    ));
    css.push_str(&palette_css(&theme.dark, "(prefers-color-scheme: dark)"));
    css
}

fn palette_css(palette: &Palette, media: &str) -> String {
    let variables: Vec<String> = palette
        .colors()
        .into_iter()
        .map(|(key, color)| format!("--{key}: {};", color.to_css()))
        .collect();
    if variables.is_empty() {
        return String::new();
    }
    format!("@media {media} {{ :host {{ {} }} }}\n", variables.join(" "))
}

/// A font family as a CSS string. What could end the string or the rule is
/// removed, so that a theme cannot inject CSS. `None` if nothing is left.
fn quote(font: &str) -> Option<String> {
    let name: String = font
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | ';' | '{' | '}') && !c.is_control())
        .collect();
    let name = name.trim();
    (!name.is_empty()).then(|| format!("\"{name}\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use salak_core::theme::parse;

    #[test]
    fn empty_theme_gives_no_css() {
        assert_eq!(from_theme(&Theme::default()), "");
    }

    #[test]
    fn palettes_go_in_their_own_mode() {
        let (theme, _) =
            parse("fg = #111\n[light]\nbg = #eee\n[dark]\nbg = #000\nmark = #f9e2af40\n");
        let css = from_theme(&theme);
        assert_eq!(
            css,
            "@media not all and (prefers-color-scheme: dark) { :host { --fg: #111111; --bg: #eeeeee; } }\n\
             @media (prefers-color-scheme: dark) { :host { --fg: #111111; --bg: #000000; --mark: #f9e2af40; } }\n"
        );
    }

    #[test]
    fn layout_keys_become_variables() {
        let (theme, _) =
            parse("font = Iosevka Aile\nmono-font = Iosevka\nfont-size = 17\nmax-width = 72\n");
        assert_eq!(
            from_theme(&theme),
            ":host { --font: \"Iosevka Aile\", sans-serif; --mono-font: \"Iosevka\", monospace; \
             --font-size: 17px; --max-width: 72ch; }\n"
        );
    }

    #[test]
    fn font_names_cannot_break_out_of_the_css() {
        let (theme, _) = parse("font = x\"; } body { display: none } a { b: \"\n");
        let css = from_theme(&theme);
        assert_eq!(
            css,
            ":host { --font: \"x  body  display: none  a  b:\", sans-serif; }\n"
        );
        assert!(!css.contains("} body"));
        assert_eq!(quote(r#""\;{}"#), None);
        assert_eq!(quote("a\u{0}b\nc"), Some("\"abc\"".into()));
    }
}
