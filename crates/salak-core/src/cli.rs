use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "\
Usage: salak [OPTIONS] [PATH]

Reads the Markdown files of a folder. PATH is a folder to browse or a file
to open, whose folder is then browsed. Without PATH, a welcome page offers
to open one.

Options:
  --theme FILE   Theme applied on top of the default look
                 (default: ~/.config/salak/theme.ini)
  -h, --help     Print this help
  -V, --version  Print the version";

const CSS_REMOVED: &str = "--css was replaced by --theme (the theme format changed, \
see Help › Theming Guide)";

#[derive(Debug, PartialEq)]
pub enum Command {
    Run {
        path: Option<PathBuf>,
        theme: Option<PathBuf>,
    },
    Help,
    Version,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut path = None;
    let mut theme = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
            Some("--theme") => {
                let file = args.next().ok_or("--theme: missing file")?;
                theme = Some(PathBuf::from(file));
            }
            Some(option) if option.starts_with("--theme=") => {
                theme = Some(PathBuf::from(&option["--theme=".len()..]));
            }
            Some(option) if option == "--css" || option.starts_with("--css=") => {
                return Err(CSS_REMOVED.into());
            }
            Some(option) if option.starts_with('-') && option != "-" => {
                return Err(format!("unknown option: {option}"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err("only one PATH can be given".into()),
        }
    }
    Ok(Command::Run { path, theme })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_path_and_theme() {
        let expected = || Command::Run {
            path: Some("notes".into()),
            theme: Some("dark.ini".into()),
        };
        assert_eq!(parse_str(&["--theme", "dark.ini", "notes"]), Ok(expected()));
        assert_eq!(parse_str(&["notes", "--theme=dark.ini"]), Ok(expected()));
    }

    #[test]
    fn css_option_points_to_theme() {
        for args in [&["--css", "dark.css"][..], &["--css=dark.css"]] {
            let err = parse_str(args).unwrap_err();
            assert!(err.contains("--css was replaced by --theme"), "{err}");
        }
    }

    #[test]
    fn defaults_to_nothing() {
        assert_eq!(
            parse_str(&[]),
            Ok(Command::Run {
                path: None,
                theme: None
            })
        );
    }

    #[test]
    fn rejects_bad_usage() {
        assert!(parse_str(&["--theme"]).is_err());
        assert!(parse_str(&["--nope"]).is_err());
        assert!(parse_str(&["a", "b"]).is_err());
        assert_eq!(parse_str(&["a", "--help"]), Ok(Command::Help));
    }
}
