// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod files;
mod render;
mod style;
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
    /// User style sheet. It may not exist (yet).
    style: Option<PathBuf>,
    /// Watches the opened document. Replacing it stops the previous watch.
    doc_watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// Watches the user style sheet, for live editing of themes.
    style_watcher: Mutex<Option<notify::RecommendedWatcher>>,
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
    *state.doc_watcher.lock().unwrap() = watcher;

    // Shown by sway in the title bar / tab of the container.
    if let Some(name) = file.file_name() {
        let _ = window.set_title(&format!("{} - Salak", name.to_string_lossy()));
    }
    Ok(Document {
        path: file.to_string_lossy().into_owned(),
        html,
    })
}

#[tauri::command]
async fn user_style(state: State<'_, AppState>) -> Result<Option<String>, String> {
    match &state.style {
        Some(path) => style::read(path),
        None => Ok(None),
    }
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

fn state_from_args(path: Option<PathBuf>, css: Option<PathBuf>) -> Result<AppState, String> {
    let canonicalize = |path: PathBuf| {
        path.canonicalize()
            .map_err(|err| format!("{}: {err}", path.display()))
    };
    let path = canonicalize(path.unwrap_or_else(|| ".".into()))?;
    let (root, initial) = if path.is_dir() {
        (path, None)
    } else {
        let root = path.parent().map(PathBuf::from).unwrap_or_else(|| path.clone());
        (root, Some(path))
    };
    // An explicit style sheet must exist, the default one is optional.
    let style = match css {
        Some(css) => Some(canonicalize(css)?),
        None => style::default_path(),
    };
    Ok(AppState {
        root,
        initial,
        style,
        doc_watcher: Mutex::default(),
        style_watcher: Mutex::default(),
    })
}

/// Tiling compositors (sway, i3, Hyprland) manage window chrome themselves:
/// client-side decorations would only add a useless title bar inside the tile.
fn under_tiling_wm() -> bool {
    ["SWAYSOCK", "I3SOCK", "HYPRLAND_INSTANCE_SIGNATURE"]
        .iter()
        .any(|var| std::env::var_os(var).is_some())
}

fn main() {
    let state = match cli::parse(std::env::args_os().skip(1)) {
        Ok(cli::Command::Help) => {
            println!("{}", cli::USAGE);
            return;
        }
        Ok(cli::Command::Version) => {
            println!("salak {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Ok(cli::Command::Run { path, css }) => state_from_args(path, css),
        Err(err) => Err(format!("{err}\n\n{}", cli::USAGE)),
    };
    let state = match state {
        Ok(state) => state,
        Err(err) => {
            eprintln!("salak: {err}");
            std::process::exit(2);
        }
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            session, list_dir, open_file, open_url, user_style
        ])
        .setup(|app| {
            // Lets the webview load images located in the opened folder.
            let root = app.state::<AppState>().root.clone();
            app.asset_protocol_scope().allow_directory(&root, true)?;

            // Without its folder, live reload starts with the next launch.
            let state = app.state::<AppState>();
            if let Some(target) = state.style.as_deref().and_then(style::watch_target) {
                let watcher = watch::watch(app.handle().clone(), target, "style-changed")
                    .map_err(|err| eprintln!("salak: cannot watch the style sheet: {err}"))
                    .ok();
                *state.style_watcher.lock().unwrap() = watcher;
            }

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
