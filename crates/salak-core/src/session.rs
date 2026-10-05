//! The folder being browsed and the rules to open files and folders in it.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::canonicalize;

pub struct Session {
    /// Folder shown in the tree. Every file access is confined to it.
    /// `None` until a file or a folder is opened.
    pub root: Option<PathBuf>,
    /// File given on the command line, opened at startup.
    pub initial: Option<PathBuf>,
}

/// Result of [`Session::open`].
#[derive(Debug, PartialEq)]
pub struct Opened {
    pub root: PathBuf,
    pub root_changed: bool,
    pub file: Option<PathBuf>,
}

/// Splits a file or a folder given by the user into the folder to browse
/// and the file to open, as on the command line.
pub fn split_target(path: PathBuf) -> (PathBuf, Option<PathBuf>) {
    if path.is_dir() {
        (path, None)
    } else {
        let root = path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| path.clone());
        (root, Some(path))
    }
}

/// Name of a folder: its last component, or the full path for `/`.
fn folder_name(root: &Path) -> String {
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// `<name> - Salak`, or `Salak` without a name. Shown by sway in the title
/// bar / tab of the container.
pub fn window_title(name: Option<&OsStr>) -> String {
    match name {
        Some(name) => format!("{} - Salak", name.to_string_lossy()),
        None => "Salak".into(),
    }
}

impl Session {
    /// Without a path, the welcome page invites to open one.
    pub fn from_args(path: Option<PathBuf>) -> Result<Session, String> {
        let Some(path) = path else {
            return Ok(Session {
                root: None,
                initial: None,
            });
        };
        let path = canonicalize(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        let (root, initial) = split_target(path);
        Ok(Session {
            root: Some(root),
            initial,
        })
    }

    /// Opens a file or a folder chosen by the user. A file inside the folder
    /// already opened keeps it; any other file opens its own folder.
    pub fn open(&mut self, path: &str) -> Result<Opened, String> {
        let path = canonicalize(Path::new(path)).map_err(|err| format!("{path}: {err}"))?;
        let (root, file) = match self.root.as_deref() {
            Some(current) if path.is_file() && path.starts_with(current) => {
                (current.to_path_buf(), Some(path))
            }
            _ => split_target(path),
        };
        let root_changed = self.root.as_deref() != Some(root.as_path());
        self.root = Some(root.clone());
        Ok(Opened {
            root,
            root_changed,
            file,
        })
    }

    /// Name of the opened folder.
    pub fn root_name(&self) -> Option<String> {
        self.root.as_deref().map(folder_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("salak-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.md"), "").unwrap();
        std::fs::write(dir.join("sub/b.md"), "").unwrap();
        canonicalize(&dir).unwrap()
    }

    fn session(root: &Path) -> Session {
        Session {
            root: Some(root.to_path_buf()),
            initial: None,
        }
    }

    #[test]
    fn file_inside_the_root_keeps_it() {
        let dir = temp("inside");
        let mut session = session(&dir);
        let file = dir.join("sub/b.md");
        let opened = session.open(file.to_str().unwrap()).unwrap();
        assert_eq!(
            opened,
            Opened {
                root: dir.clone(),
                root_changed: false,
                file: Some(file)
            }
        );
        assert_eq!(session.root, Some(dir.clone()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn file_outside_the_root_opens_its_folder() {
        let dir = temp("outside");
        let mut session = session(&dir.join("sub"));
        let file = dir.join("a.md");
        let opened = session.open(file.to_str().unwrap()).unwrap();
        assert_eq!(
            opened,
            Opened {
                root: dir.clone(),
                root_changed: true,
                file: Some(file)
            }
        );
        assert_eq!(session.root, Some(dir.clone()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn folder_replaces_the_root() {
        let dir = temp("folder");
        let mut session = Session {
            root: None,
            initial: None,
        };
        let sub = dir.join("sub");
        let opened = session.open(sub.to_str().unwrap()).unwrap();
        assert_eq!(
            opened,
            Opened {
                root: sub.clone(),
                root_changed: true,
                file: None
            }
        );
        assert_eq!(session.root_name().as_deref(), Some("sub"));
        // Opening a folder inside the root still changes it.
        let again = session.open(dir.to_str().unwrap()).unwrap();
        assert_eq!(again.root, dir);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_path_is_an_error_and_changes_nothing() {
        let dir = temp("missing");
        let mut session = session(&dir);
        let missing = dir.join("nope.md");
        assert!(session.open(missing.to_str().unwrap()).is_err());
        assert_eq!(session.root, Some(dir.clone()));
        assert!(Session::from_args(Some(missing)).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn from_args_splits_files_and_folders() {
        let dir = temp("args");
        let none = Session::from_args(None).unwrap();
        assert_eq!((none.root, none.initial), (None, None));
        let folder = Session::from_args(Some(dir.clone())).unwrap();
        assert_eq!((folder.root, folder.initial), (Some(dir.clone()), None));
        let file = Session::from_args(Some(dir.join("sub/../a.md"))).unwrap();
        assert_eq!(
            (file.root, file.initial),
            (Some(dir.clone()), Some(dir.join("a.md")))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_and_titles() {
        assert_eq!(folder_name(Path::new("/home/me/notes")), "notes");
        assert_eq!(folder_name(Path::new("/")), "/");
        assert_eq!(window_title(Some(OsStr::new("a.md"))), "a.md - Salak");
        assert_eq!(window_title(None), "Salak");
    }
}
