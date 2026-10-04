// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;

use std::path::{PathBuf, MAIN_SEPARATOR};

use serde::Serialize;
use tauri::{State, WebviewUrl, WebviewWindowBuilder};

struct AppState {
    /// Folder shown in the tree. Every file access is confined to it.
    root: PathBuf,
    /// File given on the command line, opened at startup.
    initial: Option<PathBuf>,
}

#[derive(Serialize)]
struct Session {
    root: String,
    root_name: String,
    initial: Option<String>,
    separator: char,
}

#[tauri::command]
fn session(state: State<AppState>) -> Session {
    Session {
        root: state.root.to_string_lossy().into_owned(),
        root_name: state
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| state.root.to_string_lossy().into_owned()),
        initial: state
            .initial
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        separator: MAIN_SEPARATOR,
    }
}

#[tauri::command]
fn list_dir(state: State<AppState>, path: String) -> Result<Vec<files::Entry>, String> {
    let dir = files::resolve_in_root(&state.root, &path)?;
    files::list_dir(&dir)
}

/// `salak [PATH]`: PATH is a folder to browse or a file to open (its folder
/// is then browsed). Defaults to the current directory.
fn parse_args() -> Result<AppState, String> {
    let arg = std::env::args_os().nth(1).unwrap_or_else(|| ".".into());
    let path = PathBuf::from(&arg)
        .canonicalize()
        .map_err(|err| format!("{}: {err}", PathBuf::from(&arg).display()))?;
    if path.is_dir() {
        Ok(AppState { root: path, initial: None })
    } else {
        let root = path.parent().map(PathBuf::from).unwrap_or_else(|| path.clone());
        Ok(AppState { root, initial: Some(path) })
    }
}

/// Tiling compositors (sway, i3, Hyprland) manage window chrome themselves:
/// client-side decorations would only add a useless title bar inside the tile.
fn under_tiling_wm() -> bool {
    ["SWAYSOCK", "I3SOCK", "HYPRLAND_INSTANCE_SIGNATURE"]
        .iter()
        .any(|var| std::env::var_os(var).is_some())
}

fn main() {
    let state = match parse_args() {
        Ok(state) => state,
        Err(err) => {
            eprintln!("salak: {err}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![session, list_dir])
        .setup(|app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Salak")
                .inner_size(1000.0, 700.0)
                .decorations(!under_tiling_wm())
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Salak");
}
