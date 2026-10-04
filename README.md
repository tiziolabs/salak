# Salak

A lightweight, read-only Markdown reader for the desktop.

- File tree of a folder, IDE style, loaded lazily.
- GitHub-like rendering: tables, footnotes, task lists, strikethrough.
- Light and dark themes, following the system preference.
- Custom style sheet, applied live while you edit it.
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
salak --css dark.css  # use another style sheet
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

## Custom style

Salak looks for a style sheet in:

- `$XDG_CONFIG_HOME/salak/style.css`, so `~/.config/salak/style.css` by
  default, on Linux;
- `%APPDATA%\salak\style.css` on Windows;
- or the file given with `--css FILE`.

It is applied on top of the default style, which it can override with plain
CSS, and reloaded as soon as it is saved. It only affects the document, never
the file tree. Symlinks are followed, so the file can live in a dotfiles
repository.

The quickest way to make a theme is to override the variables of the default
style:

```css
:host {
  --fg: #cdd6f4;        /* text */
  --muted: #a6adc8;     /* quotes, footnotes */
  --bg: #1e1e2e;        /* background */
  --border: #45475a;    /* rules, tables, headings */
  --subtle-bg: #181825; /* code blocks, table stripes */
  --code-bg: #313244;   /* inline code */
  --link: #89b4fa;
  --mark: #f9e2af40;
}
```

Any element of the document can also be targeted directly:

```css
.markdown-body {
  max-width: 72ch;
  font-family: "Iosevka Aile", sans-serif;
}

h1, h2 { border-bottom: none; }
```

Fonts must be installed on the system: for security, the style sheet cannot
load resources from the network.

Code blocks are highlighted with classes named after the scopes of the
language grammars: `keyword.control.shell` gives `hl-keyword hl-control
hl-shell`. Their colors are variables too:

```css
:host {
  --hl-comment: #6c7086;
  --hl-keyword: #cba6f7;
  --hl-string: #a6e3a1;
  --hl-constant: #fab387;   /* numbers, options like `-y` */
  --hl-function: #89b4fa;   /* functions, shell commands */
  --hl-type: #f9e2af;
  --hl-variable: #f38ba8;
  --hl-tag: #94e2d5;        /* HTML tags, Markdown headings */
  --hl-inserted: #a6e3a1;   /* diffs */
  --hl-deleted: #f38ba8;
}

.hl-comment { font-style: normal; }
```

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

Syntax highlighting of code blocks is enabled by default. It adds about
2 MB to the binary; to build without it:

```sh
cargo build --release --no-default-features --features custom-protocol
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
