//! User theme: colours, fonts and width of documents, in a small INI-like
//! file shared by every frontend.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME/salak/theme.ini` (`~/.config/salak/theme.ini` by
/// default), or `%APPDATA%\salak\theme.ini` on Windows.
pub fn default_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let dir = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let dir = xdg_config_home(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    );
    dir.map(|dir| dir.join("salak").join("theme.ini"))
}

/// Per the XDG specification, a relative `XDG_CONFIG_HOME` is ignored.
#[cfg_attr(windows, allow(dead_code))]
fn xdg_config_home(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    xdg.map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))
}

/// A colour, written `#rgb`, `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub fn parse(text: &str) -> Option<Rgba> {
        let digits = text.strip_prefix('#')?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let nibble = |i: usize| u8::from_str_radix(&digits[i..=i], 16).unwrap();
        let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).unwrap();
        match digits.len() {
            // `#abc` is `#aabbcc`, as in CSS.
            3 => Some(Rgba {
                r: nibble(0) * 17,
                g: nibble(1) * 17,
                b: nibble(2) * 17,
                a: 255,
            }),
            6 | 8 => Some(Rgba {
                r: byte(0),
                g: byte(2),
                b: byte(4),
                a: if digits.len() == 8 { byte(6) } else { 255 },
            }),
            _ => None,
        }
    }

    pub fn to_css(&self) -> String {
        let Rgba { r, g, b, a } = *self;
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

/// Declares `Palette`, with one colour per key of the theme file.
macro_rules! palette {
    ($($field:ident => $key:literal),* $(,)?) => {
        /// The colours of one mode (light or dark). Unset ones keep the
        /// default of the frontend.
        #[derive(Debug, Default, Clone, PartialEq)]
        pub struct Palette {
            $(pub $field: Option<Rgba>,)*
        }

        impl Palette {
            fn is_key(key: &str) -> bool {
                matches!(key, $($key)|*)
            }

            fn set(&mut self, key: &str, color: Rgba) {
                match key {
                    $($key => self.$field = Some(color),)*
                    _ => unreachable!("{key} is not a colour key"),
                }
            }

            /// The colours that are set, with the name of their key.
            pub fn colors(&self) -> Vec<(&'static str, Rgba)> {
                [$(($key, self.$field)),*]
                    .into_iter()
                    .filter_map(|(key, color)| Some((key, color?)))
                    .collect()
            }
        }
    };
}

palette! {
    fg => "fg",
    muted => "muted",
    bg => "bg",
    border => "border",
    subtle_bg => "subtle-bg",
    code_bg => "code-bg",
    link => "link",
    mark => "mark",
    hl_comment => "hl-comment",
    hl_keyword => "hl-keyword",
    hl_string => "hl-string",
    hl_constant => "hl-constant",
    hl_function => "hl-function",
    hl_type => "hl-type",
    hl_variable => "hl-variable",
    hl_tag => "hl-tag",
    hl_inserted => "hl-inserted",
    hl_deleted => "hl-deleted",
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Theme {
    pub light: Palette,
    pub dark: Palette,
    pub font: Option<String>,
    pub mono_font: Option<String>,
    /// In points or pixels, as the frontend sees fit.
    pub font_size: Option<f32>,
    /// Width of the text column, in characters.
    pub max_width: Option<u32>,
    /// GtkSourceView style scheme of code blocks.
    pub code_scheme_light: Option<String>,
    pub code_scheme_dark: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Top,
    Light,
    Dark,
    /// Already reported: its keys are skipped without more warnings.
    Unknown,
}

/// Parses a theme file. It never fails: what cannot be understood is left
/// out and reported in the warnings.
pub fn parse(text: &str) -> (Theme, Vec<String>) {
    let mut theme = Theme::default();
    let mut warnings = Vec::new();
    let mut section = Section::Top;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        let at = |message: String| format!("line {}: {message}", index + 1);
        if let Some(name) = line.strip_prefix('[') {
            section = match name.strip_suffix(']').map(str::trim) {
                Some("light") => Section::Light,
                Some("dark") => Section::Dark,
                _ => {
                    warnings.push(at(format!("unknown section {line:?}")));
                    Section::Unknown
                }
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warnings.push(at("expected \"key = value\"".into()));
            continue;
        };
        if section == Section::Unknown {
            continue;
        }
        let (key, value) = (key.trim(), strip_comment(value.trim()));
        if value.is_empty() {
            warnings.push(at(format!("{key}: missing value")));
            continue;
        }
        let in_section = section != Section::Top;
        let mut set = |value: &str| -> Result<(), String> {
            match key {
                "code-scheme" => {
                    let scheme = Some(value.to_string());
                    if section != Section::Dark {
                        theme.code_scheme_light = scheme.clone();
                    }
                    if section != Section::Light {
                        theme.code_scheme_dark = scheme;
                    }
                }
                "font" | "mono-font" | "font-size" | "max-width" if in_section => {
                    return Err(format!("{key} cannot be set in a section"));
                }
                "font" => theme.font = Some(value.to_string()),
                "mono-font" => theme.mono_font = Some(value.to_string()),
                "font-size" => match value.parse::<f32>() {
                    Ok(size) if size.is_finite() && size > 0.0 => theme.font_size = Some(size),
                    _ => return Err(format!("invalid font size {value:?}")),
                },
                "max-width" => match value.parse::<u32>() {
                    Ok(width) if width > 0 => theme.max_width = Some(width),
                    _ => return Err(format!("invalid width {value:?}")),
                },
                _ if Palette::is_key(key) => {
                    let color = Rgba::parse(value).ok_or(format!("invalid color {value:?}"))?;
                    // Keys outside a section fill both palettes.
                    if section != Section::Dark {
                        theme.light.set(key, color);
                    }
                    if section != Section::Light {
                        theme.dark.set(key, color);
                    }
                }
                _ => return Err(format!("unknown key {key:?}")),
            }
            Ok(())
        };
        if let Err(message) = set(value) {
            warnings.push(at(message));
        }
    }
    (theme, warnings)
}

/// A `#` or `;` preceded by a space starts a comment. At the start of a
/// value, `#` is a colour.
fn strip_comment(value: &str) -> &str {
    let mut previous_space = false;
    for (index, c) in value.char_indices() {
        if previous_space && (c == '#' || c == ';') {
            return value[..index].trim_end();
        }
        previous_space = c.is_whitespace();
    }
    value
}

/// A theme file with its warnings, or `None` if it does not exist.
pub fn load(path: &Path) -> Result<Option<(Theme, Vec<String>)>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(parse(&String::from_utf8_lossy(&bytes)))),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// A theme given on the command line, which must exist.
pub fn explicit_path(path: &Path) -> Result<PathBuf, String> {
    crate::canonicalize(path).map_err(|err| format!("{}: {err}", path.display()))
}

/// Real location of the theme file, so that a symlink (e.g. into a
/// dotfiles repository) is followed. `None` when its folder does not exist.
pub fn watch_target(path: &Path) -> Option<PathBuf> {
    crate::canonicalize(path).ok().or_else(|| {
        let dir = crate::canonicalize(path.parent()?).ok()?;
        Some(dir.join(path.file_name()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_xdg_config_home_when_absolute() {
        let dir = xdg_config_home(Some("/xdg".into()), Some("/home/me".into()));
        assert_eq!(dir, Some(PathBuf::from("/xdg")));
    }

    #[test]
    fn falls_back_to_home() {
        let expected = Some(PathBuf::from("/home/me/.config"));
        assert_eq!(xdg_config_home(None, Some("/home/me".into())), expected);
        assert_eq!(
            xdg_config_home(Some("relative".into()), Some("/home/me".into())),
            expected
        );
        assert_eq!(
            xdg_config_home(Some("".into()), Some("/home/me".into())),
            expected
        );
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let path = std::env::temp_dir().join("salak-no-such-theme.ini");
        assert_eq!(load(&path), Ok(None));
    }

    fn rgb(r: u8, g: u8, b: u8) -> Option<Rgba> {
        Some(Rgba { r, g, b, a: 255 })
    }

    #[test]
    fn parses_colors() {
        assert_eq!(Rgba::parse("#fa0"), rgb(0xff, 0xaa, 0x00));
        assert_eq!(Rgba::parse("#1e66F5"), rgb(0x1e, 0x66, 0xf5));
        let translucent = Rgba::parse("#f9e2af40").unwrap();
        assert_eq!(translucent.a, 0x40);
        assert_eq!(translucent.to_css(), "#f9e2af40");
        assert_eq!(Rgba::parse("#fa0").unwrap().to_css(), "#ffaa00");
        for bad in ["fa0", "#", "#ab", "#abcd", "#ggg", "#éé1", "#1234567"] {
            assert_eq!(Rgba::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn empty_file_gives_the_default_theme() {
        assert_eq!(parse(""), (Theme::default(), vec![]));
        assert_eq!(
            parse("\n  \n# only\n; comments\n"),
            (Theme::default(), vec![])
        );
    }

    #[test]
    fn top_level_keys_fill_both_palettes_and_sections_override() {
        let (theme, warnings) =
            parse("fg = #111\nlink = #222\n[light]\nbg = #eee\nlink = #333\n[dark]\nbg = #000\n");
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(theme.light.fg, rgb(0x11, 0x11, 0x11));
        assert_eq!(theme.dark.fg, rgb(0x11, 0x11, 0x11));
        assert_eq!(theme.light.link, rgb(0x33, 0x33, 0x33));
        assert_eq!(theme.dark.link, rgb(0x22, 0x22, 0x22));
        assert_eq!(theme.light.bg, rgb(0xee, 0xee, 0xee));
        assert_eq!(theme.dark.bg, rgb(0, 0, 0));
        assert_eq!(theme.dark.colors().len(), 3);
    }

    #[test]
    fn parses_layout_and_code_scheme() {
        let (theme, warnings) = parse(
            "font = Iosevka Aile\nmono-font = Iosevka\nfont-size = 17.5\nmax-width = 72  # chars\n\
             code-scheme = classic\n[dark]\ncode-scheme = Adwaita-dark ; night\n",
        );
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(theme.font.as_deref(), Some("Iosevka Aile"));
        assert_eq!(theme.mono_font.as_deref(), Some("Iosevka"));
        assert_eq!(theme.font_size, Some(17.5));
        assert_eq!(theme.max_width, Some(72));
        assert_eq!(theme.code_scheme_light.as_deref(), Some("classic"));
        assert_eq!(theme.code_scheme_dark.as_deref(), Some("Adwaita-dark"));
    }

    #[test]
    fn color_value_is_not_a_comment() {
        let (theme, warnings) = parse("fg = #abc # grey\nbg=#fff;white\nlink = #\n");
        assert_eq!(theme.light.fg, rgb(0xaa, 0xbb, 0xcc));
        // No space before `;`: it is not a comment, so the colour is invalid.
        assert_eq!(theme.light.bg, None);
        assert_eq!(
            warnings,
            [
                "line 2: invalid color \"#fff;white\"",
                "line 3: invalid color \"#\""
            ]
        );
    }

    #[test]
    fn warns_and_goes_on() {
        let (theme, warnings) = parse(
            "colour = #fff\nfg = red\nfont-size = -3\nmax-width = wide\nfont =\nnonsense\n\
             [sepia]\nfg = #fff\n[light]\nfont = Serif\nlink = #00f\n",
        );
        assert_eq!(
            warnings,
            [
                "line 1: unknown key \"colour\"",
                "line 2: invalid color \"red\"",
                "line 3: invalid font size \"-3\"",
                "line 4: invalid width \"wide\"",
                "line 5: font: missing value",
                "line 6: expected \"key = value\"",
                "line 7: unknown section \"[sepia]\"",
                "line 10: font cannot be set in a section",
            ]
        );
        // The skipped section did not leak, the following one still works.
        assert_eq!(theme.light.fg, None);
        assert_eq!(theme.light.link, rgb(0, 0, 0xff));
        assert_eq!(theme.font, None);
    }

    #[test]
    fn accepts_crlf_and_trailing_spaces() {
        let (theme, warnings) = parse("[dark]\r\nfg = #fff   \r\nfont = x\r\n");
        assert_eq!(theme.dark.fg, rgb(255, 255, 255));
        assert_eq!(warnings, ["line 3: font cannot be set in a section"]);
        let (theme, _) = parse("font = Iosevka   \r\n");
        assert_eq!(theme.font.as_deref(), Some("Iosevka"));
    }

    #[test]
    fn equals_sign_in_a_font_name() {
        let (theme, warnings) = parse("font = Weird = Font\n");
        assert!(warnings.is_empty());
        assert_eq!(theme.font.as_deref(), Some("Weird = Font"));
    }
}
