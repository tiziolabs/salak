//! User style sheet, layered on top of the default document style.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME/salak/style.css` (`~/.config/salak/style.css` by
/// default), or `%APPDATA%\salak\style.css` on Windows.
pub fn default_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let dir = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let dir = xdg_config_home(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"));
    dir.map(|dir| dir.join("salak").join("style.css"))
}

/// Per the XDG specification, a relative `XDG_CONFIG_HOME` is ignored.
#[cfg_attr(windows, allow(dead_code))]
fn xdg_config_home(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    xdg.map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))
}

/// Contents of the style sheet, or `None` if it does not exist.
pub fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// Real location of the style sheet, so that a symlink (e.g. into a
/// dotfiles repository) is followed. `None` when its folder does not exist.
pub fn watch_target(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok().or_else(|| {
        let dir = path.parent()?.canonicalize().ok()?;
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
        assert_eq!(xdg_config_home(Some("relative".into()), Some("/home/me".into())), expected);
        assert_eq!(xdg_config_home(Some("".into()), Some("/home/me".into())), expected);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let path = std::env::temp_dir().join("salak-no-such-style.css");
        assert_eq!(read(&path), Ok(None));
    }
}
