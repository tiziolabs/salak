//! Watching a file for changes made by other programs.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gtk::gio::{self, prelude::*};
use gtk::glib;

/// Editors fire several events for one save; they are merged into one call.
const SETTLE: Duration = Duration::from_millis(100);

/// Calls `on_change` when `file` is written, replaced, moved or deleted.
/// Dropping the monitor (after `cancel`) stops the watch.
///
/// The parent folder is watched, not the file itself: editors save by writing
/// a temporary file and renaming it over the original, which a watch on the
/// file would lose track of.
pub fn watch(file: &Path, on_change: impl Fn() + 'static) -> Option<gio::FileMonitor> {
    let parent = file.parent()?;
    let monitor = gio::File::for_path(parent)
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        .ok()?;
    let file = file.to_path_buf();
    let on_change = Rc::new(on_change);
    let pending = Rc::new(Cell::new(false));
    monitor.connect_changed(move |_, changed, other, event| {
        use gio::FileMonitorEvent as Event;
        // Reading the file updates its access time: attribute changes are noise.
        if !matches!(
            event,
            Event::Changed
                | Event::Created
                | Event::Deleted
                | Event::MovedIn
                | Event::MovedOut
                | Event::Renamed
        ) {
            return;
        }
        let concerns = |candidate: &gio::File| candidate.path().as_deref() == Some(file.as_path());
        if !(concerns(changed) || other.is_some_and(concerns)) || pending.replace(true) {
            return;
        }
        let on_change = on_change.clone();
        let pending = pending.clone();
        glib::timeout_add_local_once(SETTLE, move || {
            pending.set(false);
            on_change();
        });
    });
    Some(monitor)
}
