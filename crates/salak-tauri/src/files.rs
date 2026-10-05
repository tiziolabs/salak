use std::path::{Path, PathBuf};

use serde::Serialize;

pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn"];

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: String,
    is_dir: bool,
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| MARKDOWN_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Canonicalizes `path` and makes sure it does not escape `root`.
pub fn resolve_in_root(root: &Path, path: &str) -> Result<PathBuf, String> {
    let resolved = dunce::canonicalize(path)
        .map_err(|err| format!("{path}: {err}"))?;
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
            Some(Entry {
                name,
                path: path.to_string_lossy().into_owned(),
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}
