# Salak

A lightweight, read-only Markdown reader for the desktop.

- File tree of a folder, IDE style, loaded lazily.
- GitHub-like rendering: tables, footnotes, task lists, strikethrough.
- Light and dark themes, following the system preference.
- Watches the opened file and offers to reload it when it changes.
- Keyboard driven, designed to fit tiling window managers such as sway.

Salak is built with [Tauri 2](https://tauri.app): a Rust backend and the
system webview (WebKitGTK on Linux, WebView2 on Windows). The frontend is
plain HTML, CSS and JavaScript, with no framework, no bundler and no npm.

## Usage

```sh
salak                 # browse the current directory
salak ~/notes         # browse a folder
salak README.md       # open a file, browsing its folder
```

### Keys

| Key | Action |
| --- | --- |
| `Tab` | Switch focus between the tree and the document |
| `↑` `↓` / `j` `k` | Move in the tree, scroll the document |
| `←` `→` / `h` `l` | Collapse / expand a folder |
| `Enter` / `o` | Open the selected file or toggle the folder |
| `g` / `G` | Go to top / bottom |
| `d` / `u` | Scroll half a page down / up |
| `r`, `F5`, `Ctrl+R` | Reload the document |
| `Esc` | Dismiss the "file changed" banner |
| `b`, `Ctrl+B` | Toggle the sidebar |

## Building

Requirements on Linux: a Rust toolchain and the WebKitGTK development
files. On Debian / Ubuntu:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libssl-dev
```

On Arch: `sudo pacman -S --needed base-devel webkit2gtk-4.1`.

Then a plain Cargo build is enough, the Tauri CLI is not needed:

```sh
cargo build --release
./target/release/salak
```

Run the tests with `cargo test`.

## sway

Salak runs natively on Wayland. When it detects sway, i3 or Hyprland, it
disables its own window decorations and lets the compositor draw them.
The window title is the name of the opened file.

Example rules in `~/.config/sway/config` (check the `app_id` with
`swaymsg -t get_tree` if needed):

```
bindsym $mod+m exec salak ~/notes
for_window [app_id="salak"] floating enable, resize set 1100 800
```

If the window stays blank, a known issue of WebKitGTK with some GPU
drivers, start it with `WEBKIT_DISABLE_DMABUF_RENDERER=1 salak`.

## Security

Markdown files may contain raw HTML. Salak sanitizes the rendered HTML with
[ammonia](https://crates.io/crates/ammonia), forbids scripts through a
Content Security Policy, and only gives access to files inside the opened
folder.

## Roadmap

- Custom style sheet, overriding or replacing the default one, read from
  `$XDG_CONFIG_HOME/salak/style.css` (`~/.config/salak/style.css` by default)
  on Linux and `%APPDATA%\salak\style.css` on Windows.
- Syntax highlighting of code blocks.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
