use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "\
Usage: salak [OPTIONS] [PATH]

Reads the Markdown files of a folder. PATH is a folder to browse or a file
to open, whose folder is then browsed. Defaults to the current directory.

Options:
  --css FILE     Style sheet applied on top of the default style
                 (default: ~/.config/salak/style.css)
  -h, --help     Print this help
  -V, --version  Print the version";

#[derive(Debug, PartialEq)]
pub enum Command {
    Run { path: Option<PathBuf>, css: Option<PathBuf> },
    Help,
    Version,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut path = None;
    let mut css = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
            Some("--css") => {
                let file = args.next().ok_or("--css: missing file")?;
                css = Some(PathBuf::from(file));
            }
            Some(option) if option.starts_with("--css=") => {
                css = Some(PathBuf::from(&option["--css=".len()..]));
            }
            Some(option) if option.starts_with('-') && option != "-" => {
                return Err(format!("unknown option: {option}"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err("only one PATH can be given".into()),
        }
    }
    Ok(Command::Run { path, css })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_path_and_css() {
        let expected = Command::Run {
            path: Some("notes".into()),
            css: Some("dark.css".into()),
        };
        assert_eq!(parse_str(&["--css", "dark.css", "notes"]), Ok(expected));
        let expected = Command::Run {
            path: Some("notes".into()),
            css: Some("dark.css".into()),
        };
        assert_eq!(parse_str(&["notes", "--css=dark.css"]), Ok(expected));
    }

    #[test]
    fn defaults_to_nothing() {
        assert_eq!(parse_str(&[]), Ok(Command::Run { path: None, css: None }));
    }

    #[test]
    fn rejects_bad_usage() {
        assert!(parse_str(&["--css"]).is_err());
        assert!(parse_str(&["--nope"]).is_err());
        assert!(parse_str(&["a", "b"]).is_err());
        assert_eq!(parse_str(&["a", "--help"]), Ok(Command::Help));
    }
}
