use std::path::{Path, PathBuf};

pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn"];

pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| MARKDOWN_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Canonicalizes `path` and makes sure it does not escape `root`.
pub fn resolve_in_root(root: &Path, path: &str) -> Result<PathBuf, String> {
    let resolved = crate::canonicalize(Path::new(path)).map_err(|err| format!("{path}: {err}"))?;
    if resolved.starts_with(root) {
        Ok(resolved)
    } else {
        Err(format!("{path}: outside of the opened folder"))
    }
}

/// Lists the sub-directories and Markdown files of `dir`, directories first.
/// Hidden entries are skipped.
pub fn list_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = std::fs::read_dir(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            // Follows symlinks, so a link to a directory shows up as one.
            let is_dir = path.is_dir();
            if !is_dir && !is_markdown(&path) {
                return None;
            }
            Some(Entry { name, path, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder in the temporary directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("salak-core-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(crate::canonicalize(&dir).unwrap())
        }

        fn touch(&self, relative: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lists_folders_first_in_case_insensitive_order() {
        let dir = TempDir::new("list");
        for file in [
            "b.md",
            "A.md",
            "c.MD",
            "notes.txt",
            ".hidden.md",
            "zed/x.md",
            ".git/y.md",
            "Alpha/x.md",
        ] {
            dir.touch(file);
        }
        let names: Vec<_> = list_dir(&dir.0)
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.is_dir))
            .collect();
        let expected = [
            ("Alpha", true),
            ("zed", true),
            ("A.md", false),
            ("b.md", false),
            ("c.MD", false),
        ];
        assert_eq!(
            names,
            expected.map(|(name, is_dir)| (name.to_string(), is_dir))
        );
    }

    #[test]
    fn lists_absolute_paths() {
        let dir = TempDir::new("paths");
        dir.touch("a.md");
        assert_eq!(list_dir(&dir.0).unwrap()[0].path, dir.0.join("a.md"));
        assert!(list_dir(&dir.0.join("missing")).is_err());
    }

    #[test]
    fn accepts_paths_inside_the_root() {
        let dir = TempDir::new("inside");
        dir.touch("sub/a.md");
        let file = dir.0.join("sub/a.md");
        assert_eq!(resolve_in_root(&dir.0, file.to_str().unwrap()), Ok(file));
    }

    #[test]
    fn refuses_paths_outside_the_root() {
        let dir = TempDir::new("outside");
        dir.touch("root/a.md");
        dir.touch("other/b.md");
        let root = dir.0.join("root");
        let other = dir.0.join("other/b.md");
        assert!(resolve_in_root(&root, other.to_str().unwrap()).is_err());
        // `..` is resolved before the check.
        let escaping = root.join("../other/b.md");
        assert!(resolve_in_root(&root, escaping.to_str().unwrap()).is_err());
        assert!(resolve_in_root(&root, root.join("missing.md").to_str().unwrap()).is_err());
    }
}
