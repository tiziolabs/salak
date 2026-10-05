//! Everything the Salak frontends share, with no GUI toolkit involved.
//!
//! File watching is not here: each frontend uses its native file monitor.

pub mod about;
pub mod cli;
pub mod files;
pub mod help;
pub mod markdown;
pub mod session;
pub mod theme;

pub use pulldown_cmark;

use std::io;
use std::path::{Path, PathBuf};

/// Canonical path. On Windows `std::fs::canonicalize` returns `\\?\` paths,
/// which users would see and the rest of the system does not expect.
pub(crate) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    #[cfg(windows)]
    {
        dunce::canonicalize(path)
    }
    #[cfg(not(windows))]
    {
        std::fs::canonicalize(path)
    }
}
