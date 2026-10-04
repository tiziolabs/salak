// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Tiling compositors (sway, i3, Hyprland) manage window chrome themselves:
/// client-side decorations would only add a useless title bar inside the tile.
fn under_tiling_wm() -> bool {
    ["SWAYSOCK", "I3SOCK", "HYPRLAND_INSTANCE_SIGNATURE"]
        .iter()
        .any(|var| std::env::var_os(var).is_some())
}

fn main() {
    tauri::Builder::default()
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
