// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "highlight")]
mod highlight;
mod render;
mod watch;

use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::process::Command;
use std::sync::Mutex;

use salak_core::about::About;
use salak_core::session::{window_title, Session};
use salak_core::{cli, files, help, style};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    /// Folder shown in the tree (every file access is confined to it) and
    /// file given on the command line.
    session: Mutex<Session>,
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
struct SessionDto {
    root: Option<String>,
    root_name: Option<String>,
    initial: Option<String>,
    separator: char,
}

impl SessionDto {
    fn new(session: &Session, initial: Option<&Path>) -> Self {
        SessionDto {
            root: session.root.as_deref().map(lossy),
            root_name: session.root_name(),
            initial: initial.map(lossy),
            separator: MAIN_SEPARATOR,
        }
    }
}

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[tauri::command]
fn session(state: State<AppState>) -> SessionDto {
    let session = state.session.lock().unwrap();
    SessionDto::new(&session, session.initial.as_deref())
}

fn current_root(state: &AppState) -> Result<PathBuf, String> {
    state
        .session
        .lock()
        .unwrap()
        .root
        .clone()
        .ok_or_else(|| "no folder is opened".into())
}

/// Opens a file or a folder chosen by the user. A file inside the folder
/// already opened keeps it; any other file opens its own folder.
#[tauri::command]
async fn open_path(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<SessionDto, String> {
    let mut session = state.session.lock().unwrap();
    let opened = session.open(&path)?;
    // Lets the webview load images located in the new folder.
    window
        .asset_protocol_scope()
        .allow_directory(&opened.root, true)
        .map_err(|err| err.to_string())?;
    if opened.file.is_none() {
        *state.doc_watcher.lock().unwrap() = None;
        set_title(&window, opened.root.file_name());
    }
    Ok(SessionDto::new(&session, opened.file.as_deref()))
}

/// Asks the user for a Markdown file, or a folder, to open.
#[tauri::command]
async fn pick(
    window: WebviewWindow,
    state: State<'_, AppState>,
    folder: bool,
) -> Result<Option<String>, String> {
    let mut dialog = window.dialog().file().set_parent(&window);
    if let Some(root) = state.session.lock().unwrap().root.as_deref() {
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
        .map(|path| {
            path.into_path()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .transpose()
        .map_err(|err| err.to_string())
}

/// Shown by sway in the title bar / tab of the container.
fn set_title(window: &WebviewWindow, name: Option<&std::ffi::OsStr>) {
    let _ = window.set_title(&window_title(name));
}

#[derive(Serialize)]
struct Document {
    /// Canonical path, which may differ from the requested one.
    path: String,
    /// Name shown in its tab.
    title: String,
    html: String,
}

/// Shown by Help › About Salak, from the package metadata.
#[derive(Serialize)]
struct AboutDto {
    version: &'static str,
    license: &'static str,
    author: String,
    repository: &'static str,
}

#[tauri::command]
fn about() -> AboutDto {
    let About {
        version,
        license,
        author,
        repository,
    } = salak_core::about::about();
    AboutDto {
        version,
        license,
        author,
        repository,
    }
}

#[tauri::command]
fn open_help(window: WebviewWindow, name: String) -> Result<Document, String> {
    let page = help::page(&name).ok_or_else(|| format!("{name}: no such help page"))?;
    let _ = window.set_title(&format!("{} - Salak", page.title));
    Ok(Document {
        // Help pages link to each other with `help:` URLs, not paths.
        path: format!("help:{name}"),
        title: page.title.to_string(),
        html: render::render(page.markdown, Path::new(""), Path::new("")),
    })
}

// Commands are async so that file system access never blocks the UI thread.

#[derive(Serialize)]
struct EntryDto {
    name: String,
    path: String,
    is_dir: bool,
}

impl From<files::Entry> for EntryDto {
    fn from(entry: files::Entry) -> Self {
        EntryDto {
            name: entry.name,
            path: lossy(&entry.path),
            is_dir: entry.is_dir,
        }
    }
}

#[tauri::command]
async fn list_dir(state: State<'_, AppState>, path: String) -> Result<Vec<EntryDto>, String> {
    let dir = files::resolve_in_root(&current_root(&state)?, &path)?;
    let entries = files::list_dir(&dir)?;
    Ok(entries.into_iter().map(EntryDto::from).collect())
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
        title: file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        html,
    })
}

/// Once the last tab is closed: stops watching its file.
#[tauri::command]
fn close_document(window: WebviewWindow, state: State<AppState>) {
    *state.doc_watcher.lock().unwrap() = None;
    let session = state.session.lock().unwrap();
    set_title(&window, session.root.as_deref().and_then(Path::file_name));
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
    if !["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        return Err(format!("{url}: unsupported link"));
    }
    let mut child = browser_command()
        .arg(&url)
        .spawn()
        .map_err(|err| err.to_string())?;
    // Reap the process so it does not linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn state_from_args(path: Option<PathBuf>, css: Option<PathBuf>) -> Result<AppState, String> {
    let session = Session::from_args(path)?;
    // An explicit style sheet must exist, the default one is optional.
    let style = match css {
        Some(css) => Some(style::explicit_path(&css)?),
        None => style::default_path(),
    };
    Ok(AppState {
        session: Mutex::new(session),
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
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "about", "&About Salak", true, None::<&str>)?,
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
            about,
            open_url,
            user_style
        ])
        .setup(|app| {
            // Lets the webview load images located in the opened folder.
            if let Some(root) = app
                .state::<AppState>()
                .session
                .lock()
                .unwrap()
                .root
                .as_deref()
            {
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
