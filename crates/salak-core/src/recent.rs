//! The files and folders opened last, shown by the welcome page.
//!
//! They are kept one path per line in `recent` next to the state of the
//! user, most recent first. Mistakes (no file, no permission) are ignored:
//! the list is a convenience and must never get in the way.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Entries kept, and shown.
pub const LIMIT: usize = 10;

/// `$XDG_STATE_HOME/salak/recent` (`~/.local/state/salak/recent` by default),
/// or `%LOCALAPPDATA%\salak\recent` on Windows.
pub fn default_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let dir = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let dir = xdg_state_home(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"));
    dir.map(|dir| dir.join("salak").join("recent"))
}

/// Per the XDG specification, a relative `XDG_STATE_HOME` is ignored.
#[cfg_attr(windows, allow(dead_code))]
fn xdg_state_home(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    xdg.map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(".local").join("state")))
}

/// Moves `path` to the front of `list`, without duplicate, and truncates it.
fn push(list: &mut Vec<PathBuf>, path: PathBuf) {
    list.retain(|entry| *entry != path);
    list.insert(0, path);
    list.truncate(LIMIT);
}

fn parse(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .take(LIMIT)
        .collect()
}

/// The entries of the file at `store`, most recent first, that still exist.
pub fn load_from(store: &Path) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(store).unwrap_or_default();
    parse(&text)
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

/// Records `path` in the file at `store`.
pub fn record_in(store: &Path, path: &Path) {
    // One line per path: a name with a line break cannot be stored.
    if path.to_string_lossy().contains(['\n', '\r']) {
        return;
    }
    let mut list = parse(&std::fs::read_to_string(store).unwrap_or_default());
    push(&mut list, path.to_path_buf());
    let text: String = list
        .iter()
        .map(|path| format!("{}\n", path.display()))
        .collect();
    if let Some(dir) = store.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(store, text);
}

/// The entries of the default list, see [`load_from`].
pub fn load() -> Vec<PathBuf> {
    default_path().map_or_else(Vec::new, |store| load_from(&store))
}

/// Records `path` in the default list, see [`record_in`].
pub fn record(path: &Path) {
    if let Some(store) = default_path() {
        record_in(&store, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("salak-recent-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn most_recent_first_without_duplicate() {
        let dir = temp("order");
        let store = dir.join("state/recent");
        let (a, b) = (dir.join("a.md"), dir.join("b.md"));
        std::fs::write(&a, "").unwrap();
        std::fs::write(&b, "").unwrap();
        record_in(&store, &a);
        record_in(&store, &b);
        record_in(&store, &a);
        assert_eq!(load_from(&store), vec![a, b]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_the_limit() {
        let mut list = Vec::new();
        for i in 0..LIMIT + 5 {
            push(&mut list, PathBuf::from(format!("/{i}")));
        }
        assert_eq!(list.len(), LIMIT);
        assert_eq!(list[0], PathBuf::from(format!("/{}", LIMIT + 4)));
    }

    #[test]
    fn skips_what_no_longer_exists() {
        let dir = temp("gone");
        let store = dir.join("recent");
        let kept = dir.join("kept.md");
        std::fs::write(&kept, "").unwrap();
        record_in(&store, &dir.join("gone.md"));
        record_in(&store, &kept);
        assert_eq!(load_from(&store), vec![kept]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_store_is_empty() {
        assert!(load_from(Path::new("/nonexistent/salak/recent")).is_empty());
    }

    #[test]
    fn relative_state_home_is_ignored() {
        let home = Some(OsString::from("/home/u"));
        assert_eq!(
            xdg_state_home(Some("rel".into()), home.clone()),
            Some(PathBuf::from("/home/u/.local/state"))
        );
        assert_eq!(
            xdg_state_home(Some("/s".into()), home),
            Some(PathBuf::from("/s"))
        );
    }
}
