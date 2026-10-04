// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;
mod render;
mod watch;

use std::path::{PathBuf, MAIN_SEPARATOR};
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

struct AppState {
    /// Folder shown in the tree. Every file access is confined to it.
    root: PathBuf,
    /// File given on the command line, opened at startup.
    initial: Option<PathBuf>,
    /// Watches the opened document. Replacing it stops the previous watch.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
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

#[derive(Serialize)]
struct Document {
    /// Canonical path, which may differ from the requested one.
    path: String,
    html: String,
}

// Commands are async so that file system access never blocks the UI thread.

#[tauri::command]
async fn list_dir(state: State<'_, AppState>, path: String) -> Result<Vec<files::Entry>, String> {
    let dir = files::resolve_in_root(&state.root, &path)?;
    files::list_dir(&dir)
}

#[tauri::command]
async fn open_file(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<Document, String> {
    let file = files::resolve_in_root(&state.root, &path)?;
    let bytes = std::fs::read(&file).map_err(|err| format!("{path}: {err}"))?;
    let html = render::render(&String::from_utf8_lossy(&bytes), &file, &state.root);

    let watcher = watch::watch(window.app_handle().clone(), file.clone(), "file-changed")
        .map_err(|err| eprintln!("salak: cannot watch {}: {err}", file.display()))
        .ok();
    *state.watcher.lock().unwrap() = watcher;

    // Shown by sway in the title bar / tab of the container.
    if let Some(name) = file.file_name() {
        let _ = window.set_title(&format!("{} - Salak", name.to_string_lossy()));
    }
    Ok(Document {
        path: file.to_string_lossy().into_owned(),
        html,
    })
}

fn browser_command() -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Command::new("xdg-open")
    }
}

/// Opens a web link in the default browser.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !["http://", "https://", "mailto:"].iter().any(|scheme| url.starts_with(scheme)) {
        return Err(format!("{url}: unsupported link"));
    }
    let mut child = browser_command().arg(&url).spawn().map_err(|err| err.to_string())?;
    // Reap the process so it does not linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// `salak [PATH]`: PATH is a folder to browse or a file to open (its folder
/// is then browsed). Defaults to the current directory.
fn parse_args() -> Result<AppState, String> {
    let arg = std::env::args_os().nth(1).unwrap_or_else(|| ".".into());
    let path = PathBuf::from(&arg)
        .canonicalize()
        .map_err(|err| format!("{}: {err}", PathBuf::from(&arg).display()))?;
    if path.is_dir() {
        Ok(AppState { root: path, initial: None, watcher: Mutex::default() })
    } else {
        let root = path.parent().map(PathBuf::from).unwrap_or_else(|| path.clone());
        Ok(AppState { root, initial: Some(path), watcher: Mutex::default() })
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
        .invoke_handler(tauri::generate_handler![session, list_dir, open_file, open_url])
        .setup(|app| {
            // Lets the webview load images located in the opened folder.
            let root = app.state::<AppState>().root.clone();
            app.asset_protocol_scope().allow_directory(&root, true)?;

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
