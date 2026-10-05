use std::path::PathBuf;

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

/// Emits `event` (payload: the path) when `file` changes on disk.
///
/// The parent folder is watched rather than the file itself: many editors
/// save by writing a temporary file and renaming it over the original,
/// which would silently end a watch on the original inode.
pub fn watch(
    app: AppHandle,
    file: PathBuf,
    event: &'static str,
) -> notify::Result<RecommendedWatcher> {
    let dir = file
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| file.clone());
    let payload = file.to_string_lossy().into_owned();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(change) = result else { return };
        let relevant = match change.kind {
            // Reading the file ourselves can update its access time.
            EventKind::Modify(ModifyKind::Metadata(_)) => false,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => true,
            _ => false,
        };
        if relevant && change.paths.contains(&file) {
            let _ = app.emit(event, &payload);
        }
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}
