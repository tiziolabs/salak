//! Applies the user theme (colours, fonts, width) to documents.
//!
//! What CSS can express goes into one style provider; what only text tags can
//! express (inline code, quotes, links) is set on the tag tables by the
//! documents, which are told about every change.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk::RGBA;
use gtk::pango;
use salak_core::theme::{Palette, Rgba, Theme};

/// Widest text, in pixels, until the theme sets a width in characters.
const DEFAULT_MAX_WIDTH: i32 = 820;

/// The colours of the text tags, for the current mode.
#[derive(Clone, Copy)]
pub struct Colors {
    pub muted: RGBA,
    pub code_bg: RGBA,
    pub quote_bg: RGBA,
    pub link: RGBA,
}

fn gdk(color: Rgba) -> RGBA {
    RGBA::new(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

/// Defaults for the keys a theme leaves out; the tones come from the GNOME
/// palette, the links from the accent colour of the system.
fn defaults(dark: bool, accent: RGBA) -> Colors {
    let rgb = |r: u8, g: u8, b: u8, a: f32| {
        RGBA::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
    };
    if dark {
        Colors {
            muted: rgb(0x9a, 0x99, 0x96, 1.0),
            code_bg: rgb(0xff, 0xff, 0xff, 0.1),
            quote_bg: rgb(0xff, 0xff, 0xff, 0.05),
            link: accent,
        }
    } else {
        Colors {
            muted: rgb(0x5e, 0x5c, 0x64, 1.0),
            code_bg: rgb(0x00, 0x00, 0x00, 0.07),
            quote_bg: rgb(0x00, 0x00, 0x00, 0.04),
            link: accent,
        }
    }
}

fn palette(theme: &Theme, dark: bool) -> &Palette {
    if dark {
        &theme.dark
    } else {
        &theme.light
    }
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

/// The CSS of `theme` for one mode. Only what the theme sets is there: the
/// rest keeps the look of `document::install_css`, which comes first.
pub fn css(theme: &Theme, dark: bool) -> String {
    let palette = palette(theme, dark);
    let mut rules = Vec::new();
    let mut rule = |selector: &str, declarations: Vec<String>| {
        if !declarations.is_empty() {
            rules.push(format!("{selector} {{ {} }}", declarations.join(" ")));
        }
    };

    let mut page = Vec::new();
    if let Some(color) = palette.bg {
        page.push(format!("background-color: {};", color.to_css()));
    }
    if let Some(color) = palette.fg {
        page.push(format!("color: {};", color.to_css()));
    }
    if let Some(font) = theme.font.as_deref().and_then(quote) {
        page.push(format!("font-family: {font};"));
    }
    if let Some(size) = theme.font_size {
        page.push(format!("font-size: {size}px;"));
    }
    rule(".md-page", page);

    if let Some(font) = theme.mono_font.as_deref().and_then(quote) {
        rule(
            ".md-page .code-block textview",
            vec![format!("font-family: {font};")],
        );
    }
    if let Some(color) = palette.subtle_bg {
        let background = vec![format!("background: {};", color.to_css())];
        rule(
            ".md-page .code-block, .md-page .md-head, .md-page .md-odd",
            background,
        );
    }
    if let Some(color) = palette.border {
        let color = color.to_css();
        rule(
            ".md-page .md-table, .md-page .md-cell",
            vec![format!("border-color: {color};")],
        );
        rule(
            ".md-page .md-rule",
            vec![format!("background-color: {color}; color: {color};")],
        );
    }
    rules.join("\n")
}

/// Reads the theme file. A missing or unreadable one gives the default theme.
/// Warnings go to stderr, and the first one is returned to be shown as well.
pub fn read(path: Option<&Path>) -> (Theme, Option<String>) {
    let Some(path) = path else {
        return (Theme::default(), None);
    };
    match salak_core::theme::load(path) {
        Ok(Some((theme, warnings))) => {
            for warning in &warnings {
                eprintln!("salak: {}: {warning}", path.display());
            }
            let first = warnings.into_iter().next();
            (theme, first.map(|w| format!("Theme: {w}")))
        }
        Ok(None) => (Theme::default(), None),
        Err(err) => {
            eprintln!("salak: {err}");
            (Theme::default(), Some(format!("Theme: {err}")))
        }
    }
}

type Observer = Box<dyn Fn(&Themer) -> bool>;

/// The current theme, and the documents that follow it.
pub struct Themer {
    theme: RefCell<Theme>,
    provider: gtk::CssProvider,
    /// Restyle a document or a code block; `false` once it is gone.
    observers: RefCell<Vec<Observer>>,
}

impl Themer {
    /// Installs the theme for the whole display, and follows the light or
    /// dark mode and the accent colour of the system.
    pub fn new(theme: Theme) -> Rc<Themer> {
        let this = Rc::new(Themer {
            theme: RefCell::new(theme),
            provider: gtk::CssProvider::new(),
            observers: RefCell::default(),
        });
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &this.provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
        let manager = adw::StyleManager::default();
        let weak = Rc::downgrade(&this);
        let follow = move || {
            if let Some(this) = weak.upgrade() {
                this.apply();
            }
        };
        manager.connect_dark_notify({
            let follow = follow.clone();
            move |_| follow()
        });
        manager.connect_accent_color_rgba_notify(move |_| follow());
        this.apply();
        this
    }

    /// Replaces the theme, as soon as the file is saved.
    pub fn set(&self, theme: Theme) {
        if *self.theme.borrow() != theme {
            *self.theme.borrow_mut() = theme;
            self.apply();
        }
    }

    fn apply(&self) {
        self.provider
            .load_from_string(&css(&self.theme.borrow(), self.dark()));
        // A callback may add observers (a document drawing code blocks).
        let observers = self.observers.take();
        let mut alive: Vec<_> = observers
            .into_iter()
            .filter(|observer| observer(self))
            .collect();
        alive.append(&mut self.observers.borrow_mut());
        *self.observers.borrow_mut() = alive;
    }

    /// `observer` is called now, then at every change of the theme or the
    /// mode, until it returns `false`.
    pub fn observe(&self, observer: impl Fn(&Themer) -> bool + 'static) {
        if observer(self) {
            self.observers.borrow_mut().push(Box::new(observer));
        }
    }

    pub fn dark(&self) -> bool {
        adw::StyleManager::default().is_dark()
    }

    pub fn colors(&self) -> Colors {
        let dark = self.dark();
        let mut colors = defaults(dark, adw::StyleManager::default().accent_color_rgba());
        let theme = self.theme.borrow();
        let palette = palette(&theme, dark);
        if let Some(color) = palette.muted {
            colors.muted = gdk(color);
        }
        if let Some(color) = palette.code_bg {
            colors.code_bg = gdk(color);
        }
        if let Some(color) = palette.subtle_bg {
            colors.quote_bg = gdk(color);
        }
        if let Some(color) = palette.link {
            colors.link = gdk(color);
        }
        colors
    }

    /// Family of inline code.
    pub fn mono_family(&self) -> String {
        self.theme
            .borrow()
            .mono_font
            .clone()
            .unwrap_or_else(|| "monospace".into())
    }

    /// GtkSourceView style scheme asked for the current mode.
    #[cfg(feature = "highlight")]
    pub fn code_scheme(&self) -> Option<String> {
        let theme = self.theme.borrow();
        if self.dark() {
            theme.code_scheme_dark.clone()
        } else {
            theme.code_scheme_light.clone()
        }
    }

    /// Widest document, in pixels, margins included: `max-width` is a number
    /// of characters, measured with the font the text will have.
    pub fn max_width(&self, view: &gtk::TextView, margins: i32) -> i32 {
        let theme = self.theme.borrow();
        let Some(chars) = theme.max_width else {
            return DEFAULT_MAX_WIDTH;
        };
        // Not read back from the view, whose style is only updated later.
        let mut font = gtk::Settings::default()
            .and_then(|settings| settings.gtk_font_name())
            .map(|name| pango::FontDescription::from_string(&name))
            .unwrap_or_default();
        if let Some(family) = &theme.font {
            font.set_family(family);
        }
        if let Some(size) = theme.font_size {
            font.set_absolute_size(size as f64 * pango::SCALE as f64);
        } else {
            // The default size of `document::install_css`.
            font.set_size((font.size() as f64 * 1.1).round() as i32);
        }
        let layout = view.create_pango_layout(Some("0"));
        layout.set_font_description(Some(&font));
        (layout.pixel_size().0 * chars as i32 + 2 * margins).max(margins * 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use salak_core::theme::parse;

    #[test]
    fn an_empty_theme_has_no_css() {
        assert_eq!(css(&Theme::default(), false), "");
    }

    #[test]
    fn colours_follow_the_mode() {
        let (theme, _) = parse("fg = #111\n[light]\nbg = #fff\n[dark]\nbg = #000\n");
        let light = css(&theme, false);
        assert!(light.contains("background-color: #ffffff;") && light.contains("color: #111111;"));
        let dark = css(&theme, true);
        assert!(dark.contains("background-color: #000000;"));
    }

    #[test]
    fn fonts_cannot_break_out_of_the_rule() {
        let (theme, _) = parse("font = X\"; } * { color: red\nmono-font = M\nfont-size = 17\n");
        let css = css(&theme, false);
        assert!(css.contains("font-family: \"X"));
        assert!(css.contains("font-size: 17px;"));
        assert!(css.contains(".code-block textview { font-family: \"M\"; }"));
        assert_eq!((css.matches('{').count(), css.matches('}').count()), (2, 2));
        assert!(!css.contains("red;"));
    }

    #[test]
    fn a_font_made_of_forbidden_characters_is_ignored() {
        let (theme, _) = parse("font = \";{}\n");
        assert_eq!(css(&theme, false), "");
    }

    #[test]
    fn borders_and_stripes() {
        let (theme, _) = parse("border = #ccc\nsubtle-bg = #eee\n");
        let css = css(&theme, false);
        assert!(css.contains(".md-cell { border-color: #cccccc; }"));
        assert!(css.contains(".md-rule { background-color: #cccccc; color: #cccccc; }"));
        assert!(css.contains(".md-odd { background: #eeeeee; }"));
    }
}
