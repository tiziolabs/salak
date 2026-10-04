// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod files;
#[cfg(feature = "highlight")]
mod highlight;
mod render;
mod style;
mod watch;

use std::ffi::OsStr;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    /// Folder shown in the tree. Every file access is confined to it.
    /// `None` until a file or a folder is opened.
    root: Mutex<Option<PathBuf>>,
    /// File given on the command line, opened at startup.
    initial: Option<PathBuf>,
    /// User style sheet. It may not exist (yet).
    style: Option<PathBuf>,
    /// Watches the opened document. Replacing it stops the previous watch.
    doc_watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// Watches the user style sheet, for live editing of themes.
    style_watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

/// What the frontend shows: a folder and, possibly, a file to open in it.
/// Without a folder, it shows the welcome page.
#[derive(Serialize)]
struct Session {
    root: Option<String>,
    root_name: Option<String>,
    initial: Option<String>,
    separator: char,
}

impl Session {
    fn new(root: Option<&Path>, initial: Option<&Path>) -> Self {
        let lossy = |path: &Path| path.to_string_lossy().into_owned();
        Session {
            root: root.map(lossy),
            root_name: root.map(|root| {
                root.file_name()
                    .map_or_else(|| lossy(root), |name| name.to_string_lossy().into_owned())
            }),
            initial: initial.map(lossy),
            separator: MAIN_SEPARATOR,
        }
    }
}

#[tauri::command]
fn session(state: State<AppState>) -> Session {
    Session::new(state.root.lock().unwrap().as_deref(), state.initial.as_deref())
}

fn current_root(state: &AppState) -> Result<PathBuf, String> {
    state.root.lock().unwrap().clone().ok_or_else(|| "no folder is opened".into())
}

/// Splits a file or a folder given by the user into the folder to browse
/// and the file to open, as on the command line.
fn split_target(path: PathBuf) -> (PathBuf, Option<PathBuf>) {
    if path.is_dir() {
        (path, None)
    } else {
        let root = path.parent().map(PathBuf::from).unwrap_or_else(|| path.clone());
        (root, Some(path))
    }
}

/// Opens a file or a folder chosen by the user. A file inside the folder
/// already opened keeps it; any other file opens its own folder.
#[tauri::command]
async fn open_path(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<Session, String> {
    let path = PathBuf::from(&path)
        .canonicalize()
        .map_err(|err| format!("{path}: {err}"))?;
    let mut root = state.root.lock().unwrap();
    let (new_root, file) = match root.as_deref() {
        Some(current) if path.is_file() && path.starts_with(current) => {
            (current.to_path_buf(), Some(path))
        }
        _ => split_target(path),
    };
    // Lets the webview load images located in the new folder.
    window
        .asset_protocol_scope()
        .allow_directory(&new_root, true)
        .map_err(|err| err.to_string())?;
    if file.is_none() {
        *state.doc_watcher.lock().unwrap() = None;
        set_title(&window, new_root.file_name());
    }
    *root = Some(new_root);
    Ok(Session::new(root.as_deref(), file.as_deref()))
}

/// Asks the user for a Markdown file, or a folder, to open.
#[tauri::command]
async fn pick(
    window: WebviewWindow,
    state: State<'_, AppState>,
    folder: bool,
) -> Result<Option<String>, String> {
    let mut dialog = window.dialog().file().set_parent(&window);
    if let Some(root) = state.root.lock().unwrap().as_deref() {
        dialog = dialog.set_directory(root);
    }
    let picked = if folder {
        dialog.set_title("Open Folder").blocking_pick_folder()
    } else {
        dialog
            .set_title("Open File")
            .add_filter("Markdown", files::MARKDOWN_EXTENSIONS)
            .blocking_pick_file()
    };
    picked
        .map(|path| path.into_path().map(|path| path.to_string_lossy().into_owned()))
        .transpose()
        .map_err(|err| err.to_string())
}

/// Shown by sway in the title bar / tab of the container.
fn set_title(window: &WebviewWindow, name: Option<&OsStr>) {
    let title = match name {
        Some(name) => format!("{} - Salak", name.to_string_lossy()),
        None => "Salak".into(),
    };
    let _ = window.set_title(&title);
}

#[derive(Serialize)]
struct Document {
    /// Canonical path, which may differ from the requested one.
    path: String,
    /// Name shown in its tab.
    title: String,
    html: String,
}

/// Help pages, embedded in the binary: name, title, Markdown.
const HELP: &[(&str, &str, &str)] = &[
    ("user-guide", "User Guide", include_str!("../docs/help/user-guide.md")),
    ("theming", "Theming Guide", include_str!("../docs/help/theming.md")),
];

#[tauri::command]
fn open_help(window: WebviewWindow, name: String) -> Result<Document, String> {
    let (_, title, markdown) = HELP
        .iter()
        .find(|(id, ..)| *id == name)
        .ok_or_else(|| format!("{name}: no such help page"))?;
    let _ = window.set_title(&format!("{title} - Salak"));
    Ok(Document {
        // Help pages link to each other with `help:` URLs, not paths.
        path: format!("help:{name}"),
        title: title.to_string(),
        html: render::render(markdown, Path::new(""), Path::new("")),
    })
}

// Commands are async so that file system access never blocks the UI thread.

#[tauri::command]
async fn list_dir(state: State<'_, AppState>, path: String) -> Result<Vec<files::Entry>, String> {
    let dir = files::resolve_in_root(&current_root(&state)?, &path)?;
    files::list_dir(&dir)
}

#[tauri::command]
async fn open_file(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<Document, String> {
    let root = current_root(&state)?;
    let file = files::resolve_in_root(&root, &path)?;
    let bytes = std::fs::read(&file).map_err(|err| format!("{path}: {err}"))?;
    let html = render::render(&String::from_utf8_lossy(&bytes), &file, &root);

    let watcher = watch::watch(window.app_handle().clone(), file.clone(), "file-changed")
        .map_err(|err| eprintln!("salak: cannot watch {}: {err}", file.display()))
        .ok();
    *state.doc_watcher.lock().unwrap() = watcher;

    set_title(&window, file.file_name());
    Ok(Document {
        path: file.to_string_lossy().into_owned(),
        title: file.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        html,
    })
}

/// Once the last tab is closed: stops watching its file.
#[tauri::command]
fn close_document(window: WebviewWindow, state: State<AppState>) {
    *state.doc_watcher.lock().unwrap() = None;
    let root = state.root.lock().unwrap();
    set_title(&window, root.as_deref().and_then(Path::file_name));
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
    // Without a path, the welcome page invites to open one.
    let (root, initial) = match path {
        Some(path) => {
            let (root, initial) = split_target(canonicalize(path)?);
            (Some(root), initial)
        }
        None => (None, None),
    };
    // An explicit style sheet must exist, the default one is optional.
    let style = match css {
        Some(css) => Some(canonicalize(css)?),
        None => style::default_path(),
    };
    Ok(AppState {
        root: Mutex::new(root),
        initial,
        style,
        doc_watcher: Mutex::default(),
        style_watcher: Mutex::default(),
    })
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let file = Submenu::with_items(
        app,
        "&File",
        true,
        &[
            &MenuItem::with_id(app, "open-file", "&Open File…", true, Some("CmdOrCtrl+O"))?,
            &MenuItem::with_id(
                app,
                "open-folder",
                "Open &Folder…",
                true,
                Some("CmdOrCtrl+Shift+O"),
            )?,
            &MenuItem::with_id(app, "close-tab", "&Close Tab", true, Some("CmdOrCtrl+W"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "&Quit", true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        "&Help",
        true,
        &[
            &MenuItem::with_id(app, "help:user-guide", "&User Guide", true, Some("F1"))?,
            &MenuItem::with_id(app, "help:theming", "&Theming Guide", true, None::<&str>)?,
        ],
    )?;
    Menu::with_items(app, &[&file, &help])
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
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            session,
            list_dir,
            open_file,
            close_document,
            open_path,
            pick,
            open_help,
            open_url,
            user_style
        ])
        .setup(|app| {
            // Lets the webview load images located in the opened folder.
            if let Some(root) = app.state::<AppState>().root.lock().unwrap().as_deref() {
                app.asset_protocol_scope().allow_directory(root, true)?;
            }

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
                .menu(menu(app.handle())?)
                .on_menu_event(|window, event| match event.id().as_ref() {
                    "quit" => window.app_handle().exit(0),
                    // The frontend handles the other items, like its keys.
                    id => {
                        let _ = window.emit("menu", id);
                    }
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Salak");
}
