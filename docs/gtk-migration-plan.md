# Migration plan: salak-core, salak-tauri and salak-gtk

This document is self-contained: every task below can be picked up with no
other context than this file and the repository. Read sections 1 to 4 once,
then take any task of section 6 whose dependencies are done.

Status: proposed, written on 2026-10-05 against the `v0.1.0` tag.

## 1. Goals

1. **No web engine on Linux.** Salak renders Markdown with native GTK 4
   widgets instead of WebKitGTK.
2. **No Tauri on Linux.** Linux gets a native GTK 4 + libadwaita application;
   Tauri is only kept for Windows (and macOS, if it is ever packaged).
3. **As small as possible.** Fewest Rust crates and system libraries. On
   Linux, the only system libraries are the ones a GNOME desktop already has:
   GTK 4, libadwaita and GtkSourceView 5.
4. **Packageable by Debian.** Salak should one day be accepted into Debian,
   so the GTK application follows the Debian Rust packaging rules (section 3).

Non-goals: find in page, editing, printing, a plugin system. They can come
later and are not needed for parity with v0.1.0.

## 2. Current state (v0.1.0)

A single crate, `salak`, built with Tauri 2. The backend is Rust, the
frontend is plain HTML, CSS and JavaScript in `ui/`.

| File | Role | Fate |
| --- | --- | --- |
| `src/main.rs` | Tauri setup, `AppState`, commands (`session`, `open_path`, `pick`, `open_file`, `list_dir`, `close_document`, `open_help`, `about`, `open_url`, `user_style`), menu, `under_tiling_wm()` | Split: logic to core, the rest to salak-tauri |
| `src/cli.rs` | Command line parsing (`[PATH]`, `--css FILE`, `-h`, `-V`) | Core (`--css` becomes `--theme`) |
| `src/files.rs` | `MARKDOWN_EXTENSIONS`, `is_markdown`, `resolve_in_root` (confinement to the opened folder), `list_dir` (dirs first, hidden entries skipped) | Core, without serde |
| `src/style.rs` | Default style sheet path (XDG / `%APPDATA%`), `read`, `watch_target` | Core, becomes `theme` |
| `src/render.rs` | Markdown → sanitized HTML: link resolution, heading ids, `percent_decode`, ammonia | Split: parsing and links to core, HTML to salak-tauri |
| `src/highlight.rs` | syntect, `<span class="hl-…">` | salak-tauri only |
| `src/watch.rs` | notify, watches the parent folder of a file, emits an event | salak-tauri only |
| `ui/` | Tree, tabs, keys, banner, tab menu, welcome page, About dialog | salak-tauri only; rewritten natively in salak-gtk |
| `docs/help/*.md` | Help pages embedded with `include_str!` | Core (moved inside the crate) |
| `tauri.conf.json`, `tauri.windows.conf.json`, `capabilities/`, `build.rs` | Tauri configuration | salak-tauri |
| `packaging/salak.desktop`, `icons/` | Linux desktop integration | salak-gtk (renamed, see T8.2) |
| `scripts/package-windows.ps1` | Windows installer and portable build | salak-tauri paths |

The Linux build pulls GTK 3, WebKitGTK 4.1, libsoup and JavaScriptCore, and
`Cargo.lock` holds 457 crates. The Windows release binary is 5.98 MB.

### Features to keep (parity checklist)

Every item must work in salak-gtk before Linux switches over (T8.1).

- Command line: `salak`, `salak DIR`, `salak FILE` (browses its folder),
  `--theme FILE`, `-h`, `-V`; exit code 2 and usage on bad arguments.
- Welcome page when no path is given: Open File, Open Folder, link to the
  user guide.
- Lazy file tree of the opened folder: folders first, then Markdown files,
  case-insensitive order, hidden entries skipped, children re-read at every
  expansion, symlinks to folders shown as folders.
- Every file access confined to the opened folder (`resolve_in_root`).
- Opening a file inside the opened folder keeps the folder; any other file
  opens its own folder.
- Tabs: one per document, a new tab opens right after the active one, an
  already open document is only brought to the front, the canonical path
  merges duplicates, closing the active tab activates the right neighbour,
  else the left one. Middle click closes. Context menu: Close This Tab, Close
  Other Tabs, Close Tabs to the Right, Close Tabs to the Left (disabled when
  they would close nothing). Each tab remembers its scroll position. Only the
  active tab is watched; the others are read again when activated.
- Rendering: headings with GitHub-like unique ids (`hello-world`,
  `hello-world-1`), paragraphs, emphasis, strong, strikethrough, inline code,
  code blocks (highlighted when the language is known), block quotes,
  ordered and unordered lists, task lists, tables with column alignment,
  footnotes, horizontal rules, images (local and https), links.
- Links: `#fragment` scrolls; relative or `/`-rooted links to Markdown files
  open a tab, scroll to the fragment and reveal the file in the tree; links to
  other local files do nothing; `help:<name>` opens a help page;
  `http`, `https` and `mailto` open in the default application; anything else
  is ignored.
- File watching: when the active file changes on disk, a banner offers
  Reload (`r`) and Dismiss (`Esc`). Watching the parent folder, because
  editors save by renaming a temporary file over the original.
- Theme file applied live at every save, without a banner.
- Window title: `<file> - Salak`, `<folder> - Salak` or `Salak`.
- Help menu: User Guide (`F1`), Theming Guide, About Salak (version,
  license, developer name without e-mail, repository link).
- Under a tiling window manager (`SWAYSOCK`, `I3SOCK` or
  `HYPRLAND_INSTANCE_SIGNATURE` set): no window decorations.
- Keys:

| Key | Action |
| --- | --- |
| `Ctrl+O` / `Ctrl+Shift+O` | Open a file / a folder |
| `Ctrl+W` | Close the tab |
| `Ctrl+PageDown` / `Ctrl+PageUp` | Next / previous tab |
| `Ctrl+Q` | Quit |
| `F1` | User guide |
| `Tab` | Switch focus between the tree and the document |
| `↑` `↓` / `j` `k` | Move in the tree, scroll the document (48 px) |
| `←` `→` / `h` `l` | Collapse / expand a folder; `h` on a file goes to its parent folder |
| `Enter` / `o` | Open the selected file or toggle the folder |
| `g` / `G`, `Home` / `End` | Top / bottom |
| `d` / `u` | Half a page down / up |
| `r`, `F5`, `Ctrl+R` | Reload the document |
| `Esc` | Dismiss the "file changed" banner |
| `b`, `Ctrl+B` | Toggle the sidebar |

## 3. Constraints and decisions

### D1. Debian baseline: Debian 13 (trixie) and later; Debian 12 is dropped

New packages enter Debian through **unstable** and only reach the **next**
stable release; a released stable never receives new packages (backports
come from testing). Debian does not ask upstreams to support oldstable.

Versions in Debian, checked on packages.debian.org on 2026-10-05:

| Package | 12 bookworm (oldstable) | 13 trixie (stable) | forky / sid |
| --- | --- | --- | --- |
| `rustc` | 1.63 | 1.85 (1.95 in backports) | 1.95 / 1.96 |
| `libadwaita-1-0` | 1.2 | 1.7 | 1.9 / 1.10 |
| `librust-gtk4-dev` | 0.3 | 0.9 | 0.11 |
| `librust-libadwaita-dev` | absent | 0.7 | 0.9 |
| `librust-sourceview5-dev` | absent | 0.9 | 0.11 |
| `librust-pulldown-cmark-dev` | 0.9 | 0.10 | 0.13 |

Debian 12 cannot build salak-gtk with Debian's own tools: no libadwaita
bindings, a too old Rust compiler, gtk4-rs 0.3. Even a `.deb` built with
rustup for it would need a second code path, because GTK 4.8 and
libadwaita 1.2 lack `FileDialog`, `UriLauncher`, `AdwToolbarView`,
`AdwOverlaySplitView`, `AdwBanner` and `AdwAboutDialog`. That cost is not
worth paying.

Consequences:

- The Rust dependencies follow Debian **unstable**, where new packages enter:
  `gtk4 0.11`, `libadwaita 0.9`, `sourceview5 0.11`, `pulldown-cmark 0.13`
  (P2 of [debian-packaging.md](debian-packaging.md), 2026-10-07; this
  replaces a pin to the older versions of trixie). Check the versions again
  on packages.debian.org before each dependency bump; dependabot must not
  move a crate past the Debian version (T8.5).
- `salak-gtk` sets `rust-version = "1.92"`, required by gtk4 0.11 and
  glib 0.22. On trixie, build with the rustc of trixie-backports or rustup.
- Version features no higher than trixie's system libraries: GTK 4.18,
  libadwaita 1.7. T4.1 checks the exact GtkSourceView version.
- T4.1 findings (2026-10-05). System libraries were checked on a development
  machine (GTK 4.22, libadwaita 1.9, GtkSourceView 5.18), not yet on a
  Debian 13 container: do that before T8.3. `gtk4 0.11` and `glib 0.22`
  require Rust 1.92, above trixie's 1.85, so salak-gtk moved to `gtk4 0.9`,
  `libadwaita 0.7` and `sourceview5 0.9` (MSRV 1.70). Their highest version
  features are `v4_16` and `v1_6`, a little under trixie's GTK 4.18 and
  libadwaita 1.7; the code needs nothing newer.

### D2. Debian Rust packaging rules that shape the code

- Debian builds Rust programs offline, and every dependency must already be
  packaged in Debian. Salak is packaged as a standalone source package built
  with `dh-cargo` from the upstream tarball, not published on crates.io (P3
  of [debian-packaging.md](debian-packaging.md), which replaces the debcargo
  plan written here first). salak-tauri is a workspace of its own and is
  never needed to build salak-gtk.
- A published crate only contains its own directory: help pages, icons, the
  desktop file, the man page and the metainfo must live **inside** the crate
  that uses them. No `include_str!("../../…")` reaching out of the crate.
- `cargo test` runs offline and without a display (autopkgtest). Tests that
  need GTK are kept out of the default test run (see T5.1 for how the
  renderer stays testable without GTK).
- Lintian expects a man page, a desktop file named after the application id,
  AppStream metainfo, and icons with sources (`icons/salak.svg`).
- Licenses: MIT OR Apache-2.0, already compatible.

### D3. libadwaita is used

It is installed on every GNOME desktop and adds no Rust crate beyond the
gtk4-rs family. It provides the tab view, split view, banner, status page,
About dialog, and the light/dark preference (`adw::StyleManager`).

### D4. Native rendering with one `GtkTextView` per document

The Markdown is rendered into a `gtk::TextBuffer`:

- inline and paragraph styles are `TextTag`s;
- links are tags carrying their target;
- heading ids are `TextMark`s named after the id;
- tables, images, horizontal rules and code blocks are widgets inserted at
  `TextChildAnchor`s.

This keeps selection across paragraphs and makes a later find in page
possible. Raw HTML in Markdown is not rendered (see T5.8).

### D5. Code highlighting with GtkSourceView 5 on Linux

Code blocks are `sourceview5::View`s, read-only, with the language guessed
from the info string and a style scheme following the light/dark mode. It is
an optional Cargo feature `highlight`, on by default. syntect stays in
salak-tauri only.

### D6. The theme format is no longer CSS

A small INI-like file, parsed by hand in salak-core (no dependency), shared by
both frontends:

```ini
# ~/.config/salak/theme.ini, %APPDATA%\salak\theme.ini, or salak --theme FILE
# Keys outside a section apply to both modes.
font = Iosevka Aile
mono-font = Iosevka
font-size = 17
max-width = 72          # in characters

[light]
fg = #4c4f69
bg = #eff1f5
link = #1e66f5

[dark]
fg = #cdd6f4
bg = #1e1e2e
link = #89b4fa
code-scheme = Adwaita-dark   # GtkSourceView scheme, salak-gtk only
```

- Colour keys: `fg`, `muted`, `bg`, `border`, `subtle-bg`, `code-bg`, `link`,
  `mark`; `hl-comment`, `hl-keyword`, `hl-string`, `hl-constant`,
  `hl-function`, `hl-type`, `hl-variable`, `hl-tag`, `hl-inserted`,
  `hl-deleted` (salak-tauri only, syntect).
- Layout keys: `font`, `mono-font`, `font-size`, `max-width`.
- GTK only: `code-scheme`.
- Colours are `#rgb`, `#rrggbb` or `#rrggbbaa`.
- Lines are `key = value`; `#` and `;` start comments; sections are `[light]`
  and `[dark]`.
- Unknown keys and invalid values produce a warning on stderr and are
  ignored; the file never prevents Salak from starting.
- Keys left out fall back to the defaults: libadwaita's colours in
  salak-gtk, `markdown.css` in salak-tauri.
- The option `--css` is renamed `--theme`; `style.css` is no longer read.

### D7. Workspace layout and build commands

```
salak/
├── Cargo.toml              # [workspace] + [profile.release]
├── crates/
│   ├── salak-core/         # pure Rust, published
│   │   └── help/           # user-guide.md, theming.md
│   ├── salak-tauri/        # Windows; ui/, tauri.conf.json, build.rs, capabilities/
│   └── salak-gtk/          # Linux, published
│       └── data/           # desktop file, metainfo, man page, icons
├── docs/assets/            # README images
├── scripts/
└── …
```

| Crate | Dependencies |
| --- | --- |
| salak-core | `pulldown-cmark` (`default-features = false`); `dunce` only for `cfg(windows)` |
| salak-tauri | salak-core, `tauri`, `tauri-plugin-dialog`, `serde`, `ammonia`, `notify`, `dunce`, `pulldown-cmark` with `html`, optional `syntect` |
| salak-gtk | salak-core, `gtk4`, `libadwaita`, optional `sourceview5` |

- Both applications produce a binary named `salak`. Always build one of them
  with `-p`: `cargo build --release -p salak-gtk` on Linux,
  `cargo build --release -p salak-tauri` on Windows. Do not build both in
  one command (output file collision).
- Tests on Linux: `cargo test -p salak-core -p salak-gtk`.
- salak-tauri keeps compiling on Linux (its code stays cross-platform), but
  it is no longer packaged there.
- No GObject subclassing in salak-gtk unless a task requires it: plain Rust
  structs, `Rc<RefCell<…>>`, `glib::clone!`.
- File system access never blocks the UI thread: `gio` async functions
  (`load_contents_future`, `enumerate_children_future`) or
  `gio::spawn_blocking`, driven by `glib::MainContext::spawn_local`.

## 4. Conventions for every task

- Conventional commit messages (`feat:`, `fix:`, `refactor:`, `docs:`,
  `chore:`), one commit per task when possible.
- Comments follow the existing style: short, explaining why, in English.
- `cargo fmt`, `cargo clippy -p <crate> -- -D warnings` and the tests of the
  affected crates pass before a task is done.
- Behaviour that is not explicitly changed by the task stays identical.
- Update `CHANGELOG.md` under "Unreleased" for user-visible changes.

## 5. Phases

| Phase | Content | Releasable alone |
| --- | --- | --- |
| 0 | Measures | yes |
| 1 | Cargo workspace, nothing else changes | yes |
| 2 | salak-core extracted | yes |
| 3 | New theme format, adopted by salak-tauri | yes (breaking change for themes) |
| 4 | salak-gtk skeleton | yes (Linux still ships Tauri) |
| 5 | Native Markdown renderer | yes |
| 6 | Feature parity | yes |
| 7 | Themes and highlighting in salak-gtk | yes |
| 8 | Linux switches to salak-gtk | release |
| 9 | Debian submission preparation | — |

Phases 3 and 4 only depend on phase 2 and can run in parallel.

## 6. Tasks

### Phase 0: measures

#### T0.1 Record the baseline

- **Depends on:** nothing.
- **Steps:**
  1. On Linux (Debian 13 or Ubuntu 24.04), at tag `v0.1.0`:
     `cargo build --release`, `cargo deb`.
  2. Record:
     - the size of `target/release/salak` and of the `.deb`;
     - `ldd target/release/salak | wc -l`;
     - the `Depends:` line of the `.deb` (`dpkg-deb -I`);
     - `grep -c '^name = ' Cargo.lock`;
     - `cargo bloat --release --crates -n 15` (`cargo install cargo-bloat`);
     - resident memory of all Salak processes (`salak` and `WebKitWebProcess`)
       with `README.md` open, from `ps -o rss`.
  3. Fill the table in section 7 of this file.
- **Done when:** section 7 has the "v0.1.0" column filled.

### Phase 1: Cargo workspace

#### T1.1 Turn the repository into a workspace with a single member

- **Depends on:** nothing.
- **Steps:**
  1. `git mv src build.rs ui tauri.conf.json tauri.windows.conf.json capabilities crates/salak-tauri/`.
  2. Create `crates/salak-tauri/Cargo.toml` from the current `[package]`,
     `[dependencies]`, `[build-dependencies]`, `[features]` and
     `[package.metadata.deb]`, with `name = "salak-tauri"`, `publish = false`
     and `[[bin]] name = "salak", path = "src/main.rs"`.
  3. Shared metadata goes in the root `[workspace.package]` (`version`,
     `license`, `edition`, `rust-version`, `authors`, `repository`); members
     use `version.workspace = true` and so on.
  4. The root `Cargo.toml` keeps only `[workspace]`
     (`members = ["crates/*"]`, `resolver = "2"`), `[workspace.package]` and
     `[profile.release]` (profiles are ignored in members).
  5. Fix the paths that leave the crate directory:
     - `include_str!("../docs/help/…")` in `main.rs` becomes
       `../../../docs/help/…` for now; T2.6 moves the help pages;
     - icon paths in `tauri.conf.json` (`../../icons/…`);
     - asset paths in `[package.metadata.deb]` (`../../`; cargo-deb resolves
       them relative to the crate);
     - `scripts/package-windows.ps1` (binary and config locations).
  6. Keep `Cargo.lock` at the root. Run `cargo build --release -p salak-tauri`.
  7. Update `README.md` and `RELEASING.md` build commands with `-p salak-tauri`.
- **Done when:** the binary behaves exactly as v0.1.0 (open a folder, a file,
  a help page, the theme reload, the file-changed banner), `cargo test -p
  salak-tauri` passes, `cargo deb -p salak-tauri` and the Windows script still
  produce their packages.

### Phase 2: salak-core

Each task moves code from `crates/salak-tauri/src/` to
`crates/salak-core/src/` and makes salak-tauri use it. Tests move with the
code they test.

#### T2.1 Create the salak-core crate

- **Depends on:** T1.1.
- **Steps:**
  1. `crates/salak-core/Cargo.toml`: `name = "salak-core"`, library only,
     workspace metadata, `description = "Core of the Salak Markdown reader"`,
     dependency `pulldown-cmark = { version = "0.10", default-features = false }`,
     and `[target.'cfg(windows)'.dependencies] dunce = "1"`.
  2. `src/lib.rs` declaring the modules added by the next tasks.
  3. A private helper `canonicalize(path) -> io::Result<PathBuf>` in
     `src/lib.rs` (or `src/path.rs`): `dunce::canonicalize` on Windows,
     `std::fs::canonicalize` elsewhere, with a comment on the `\\?\` prefix.
     All core code uses it instead of `dunce`.
  4. salak-tauri depends on `salak-core = { path = "../salak-core", version = "0.1.0" }`.
- **Done when:** the workspace builds; salak-core has no dependency other
  than pulldown-cmark (and dunce on Windows): check with
  `cargo tree -p salak-core --target x86_64-unknown-linux-gnu`.

#### T2.2 Move `cli`

- **Depends on:** T2.1.
- **Steps:** move `cli.rs` unchanged (with its tests) to `salak_core::cli`;
  salak-tauri imports it. The `--css` rename happens in T3.3.
- **Done when:** tests pass in salak-core; salak-tauri has no `cli.rs`.

#### T2.3 Move `files` without serde

- **Depends on:** T2.1.
- **Steps:**
  1. Move `files.rs` to `salak_core::files`. `Entry` gets public fields
     (`name: String`, `path: PathBuf`, `is_dir: bool`) and no `Serialize`.
     `path` becomes a `PathBuf`: string conversion is the frontend's job.
  2. In salak-tauri, add a `#[derive(Serialize)] struct EntryDto { name,
     path: String, is_dir }` built from `Entry`, so the JSON sent to `ui/` is
     unchanged.
  3. Add tests in salak-core for `list_dir` on a temporary folder (folders
     first, case-insensitive order, hidden entries and non-Markdown files
     skipped) and for `resolve_in_root` (accepted inside, refused outside,
     `..` escaping refused). Use `std::env::temp_dir()` and a unique
     sub-folder; no `tempfile` dependency.
- **Done when:** the tree in the Tauri app is unchanged; the new tests pass.

#### T2.4 Move the session logic

- **Depends on:** T2.3.
- **Steps:**
  1. Create `salak_core::session` with:
     - `split_target(path) -> (PathBuf, Option<PathBuf>)` (from `main.rs`);
     - `pub struct Session { pub root: Option<PathBuf>, pub initial: Option<PathBuf> }`;
     - `Session::from_args(path: Option<PathBuf>) -> Result<Session, String>`,
       the path part of `state_from_args`;
     - `Session::open(&mut self, path: &str) -> Result<Opened, String>`, the
       rule of `open_path`: canonicalize; a file inside the current root keeps
       the root; anything else goes through `split_target`. `Opened` tells
       whether the root changed and which file, if any, to open;
     - `root_name()` (the folder name, or the full path for `/`);
     - `window_title(name: Option<&OsStr>) -> String` (`"<name> - Salak"` or
       `"Salak"`), from `set_title`.
  2. salak-tauri keeps its `Session` DTO for the frontend, built from the core
     one, and keeps the `asset_protocol_scope` calls.
  3. Tests: file in root, file outside root, folder, missing path.
- **Done when:** opening files and folders in the Tauri app behaves as before.

#### T2.5 Split `render.rs`: Markdown and links in core

- **Depends on:** T2.1.
- **Steps:**
  1. Create `salak_core::markdown` with:
     - `pub const OPTIONS: Options` (tables, footnotes, strikethrough, task
       lists);
     - `pub fn events(markdown: &str) -> Vec<Event<'_>>`: parses with
       `OPTIONS` and calls `add_heading_ids`;
     - `pub fn slugify`;
     - `pub enum Link { Anchor(String), Help(String), Local { path: PathBuf, fragment: String }, External(String), Unsupported }`;
     - `pub fn resolve_link(url: &str, base: &Path, root: &Path) -> Link`,
       built from `local_target`, `has_scheme` and `percent_decode`.
       `External` only for `http`, `https` and `mailto` (the rule of
       `open_url`), `Unsupported` for other schemes, `Anchor` for `#…` (the
       fragment percent-decoded), `Help` for `help:<name>`;
     - `pub fn is_openable(path) -> bool`: true for Markdown files (the rule
       of `MARKDOWN_FILE` in `ui/app.js`), using `files::is_markdown`.
  2. Re-export `pulldown_cmark` from salak-core (`pub use pulldown_cmark;`)
     so frontends use the same version.
  3. salak-tauri's `render.rs` keeps the HTML-specific parts: it maps `Link`
     to `salak:` / `ASSET_PREFIX` / unchanged URLs, enables the `html` feature
     of its own `pulldown-cmark` dependency (same version as core), runs
     highlighting and ammonia. Keep `encode` there (HTML-specific).
  4. Move the tests: `heading_ids_are_unique`, `detects_schemes` and new
     `resolve_link` tests (relative, `/`-rooted, fragment, percent-encoded,
     Windows drive `C:/`, `help:`, `mailto:`, `javascript:` → `Unsupported`)
     to core. HTML tests (`strips_scripts`, `rewrites_local_links_and_images`,
     `keeps_task_lists_and_alignment`, `highlights_known_languages_only`,
     `keeps_help_links`) stay in salak-tauri.
- **Done when:** `cargo tree -p salak-core -e features` does not show the
  `html` feature of pulldown-cmark; all tests pass; rendering in the Tauri app
  is unchanged.

#### T2.6 Move help pages and About into core

- **Depends on:** T2.1.
- **Steps:**
  1. `git mv docs/help crates/salak-core/help`.
  2. `salak_core::help`:
     - `pub struct Page { pub name: &'static str, pub title: &'static str, pub markdown: &'static str }`;
     - `pub const PAGES: &[Page]` with `include_str!("../help/…")`;
     - `pub fn page(name) -> Option<&'static Page>`.
  3. `salak_core::about`: `pub struct About { version, license, author, repository }`
     and `pub fn about() -> About`, from `main.rs`. Because `CARGO_PKG_*` is
     expanded in salak-core, the version shown is salak-core's; both crates
     share `version.workspace = true`, so they never differ.
  4. Add `include = ["src/**", "help/**", "Cargo.toml", "README.md", "LICENSE-*"]`
     to salak-core's `[package]`, plus symlinks or copies of the licence files,
     so `cargo package -p salak-core` contains the help pages.
  5. Fix links to `docs/help/…` in `README.md`.
- **Done when:** `cargo package -p salak-core --list` lists `help/*.md`;
  F1 still works in the Tauri app.

#### T2.7 Keep watching and theming I/O out of core

- **Depends on:** T2.1.
- **Steps:**
  1. Move `style.rs` to `salak_core::style` unchanged for now (T3.1 replaces
     it). It uses the core `canonicalize`.
  2. `watch.rs` (notify) stays in salak-tauri: each frontend uses its native
     file monitor. Add a sentence saying so at the top of `salak-core/src/lib.rs`.
  3. Remove `dunce` from salak-tauri if nothing uses it any more; otherwise
     keep it.
- **Done when:** `cargo tree -p salak-core` shows no `notify`, `serde`,
  `tauri` or `ammonia`.

### Phase 3: theme format

#### T3.1 Theme parser in salak-core

- **Depends on:** T2.7.
- **Steps:**
  1. Replace `salak_core::style` with `salak_core::theme`:
     - `default_path()`: same folders as today, file name `theme.ini`;
     - `pub struct Rgba { r: u8, g: u8, b: u8, a: u8 }` with
       `fn parse(&str) -> Option<Rgba>` (`#rgb`, `#rrggbb`, `#rrggbbaa`) and
       `fn to_css(&self) -> String`;
     - `pub struct Palette` with one `Option<Rgba>` per colour key of D6;
     - `pub struct Theme { light: Palette, dark: Palette, font: Option<String>, mono_font: Option<String>, font_size: Option<f32>, max_width: Option<u32>, code_scheme_light: Option<String>, code_scheme_dark: Option<String> }`;
     - `pub fn parse(text: &str) -> (Theme, Vec<String>)`: the theme and its
       warnings (`line N: unknown key "x"`). Keys outside a section fill both
       palettes; section keys override them;
     - `pub fn load(path) -> Result<Option<(Theme, Vec<String>)>, String>`:
       `None` when the file does not exist, as `style::read` does today;
     - keep `watch_target` as is.
  2. Tests: empty file, comments, both sections, override order, invalid
     colour, unknown key, unknown section, CRLF line endings, `=` inside a
     font name, trailing spaces.
- **Done when:** tests pass; no new dependency.

#### T3.2 salak-tauri applies the new theme

- **Depends on:** T3.1.
- **Steps:**
  1. In `ui/markdown.css`, introduce variables for what the theme can set:
     `--font`, `--mono-font`, `--font-size`, `--max-width` (in `ch`), used by
     `.markdown-body`, `pre` and `code`, with the current values as defaults.
  2. Replace the `user_style` command with `theme_css`, which loads the theme
     and turns it into CSS: `:host { --fg: …; … }` for keys shared by both
     modes and the light palette, and
     `@media (prefers-color-scheme: dark) { :host { … } }` for the dark
     palette. Font names are quoted and stripped of `"`, `\`, `;`, `{`, `}`
     and newlines, so the generated CSS cannot be broken out of.
  3. Theme warnings go to stderr.
  4. Watch `theme.ini` instead of `style.css` (same `style-changed` event).
- **Done when:** a `theme.ini` with colours, fonts and both sections changes
  the document live; a broken file only prints warnings.

#### T3.3 Rename `--css` to `--theme` and rewrite the theming guide

- **Depends on:** T3.1.
- **Steps:**
  1. In `salak_core::cli`: `--theme FILE` and `--theme=FILE`; `--css` gives
     the error `--css was replaced by --theme (the theme format changed, see
     Help › Theming Guide)`. Update `USAGE` and the tests.
  2. Rewrite `crates/salak-core/help/theming.md` for the format of D6: where
     the file lives, every key with its meaning and default, light and dark
     sections, a full example (the Catppuccin values of the current guide),
     live reload, symlinks, and what is frontend-specific (`code-scheme` on
     Linux, `hl-*` on Windows). Section "Migrating from style.css".
  3. Update the "Custom style" section of `README.md` and
     `crates/salak-core/help/user-guide.md`.
  4. `CHANGELOG.md`: breaking change entry.
- **Done when:** `salak --help` shows `--theme`; the guide matches the parser.

### Phase 4: salak-gtk skeleton

#### T4.1 Create the crate and check system versions

- **Depends on:** T2.4, T2.6.
- **Steps:**
  1. On a Debian 13 system or container, record the versions of
     `libgtk-4-dev`, `libadwaita-1-dev` and `libgtksourceview-5-dev`
     (`apt-cache policy`). Write them in D1 of this file.
  2. `crates/salak-gtk/Cargo.toml`: `name = "salak-gtk"`,
     `[[bin]] name = "salak"`, workspace metadata, and:
     ```toml
     gtk = { package = "gtk4", version = "0.9", features = ["v4_16"] }
     adw = { package = "libadwaita", version = "0.7", features = ["v1_6"] }
     sourceview5 = { version = "0.9", optional = true }  # version feature per step 1
     salak-core = { path = "../salak-core", version = "0.1.0" }

     [features]
     default = ["highlight"]
     highlight = ["dep:sourceview5"]
     ```
     Lower the version features if step 1 shows older libraries.
  3. Check that the MSRV of these crates is at most 1.85 (their
     `rust-version`); if not, record the gap here and pick the newest crate
     versions that fit, as long as Debian unstable has them.
  4. `src/main.rs`: `adw::Application` with id `com.tiziolabs.salak` and an
     empty window titled "Salak", 1000×700.
- **Done when:** `cargo run -p salak-gtk` opens a window on Debian 13;
  `cargo tree -p salak-gtk --depth 1` shows only the four dependencies above.

#### T4.2 Command line and startup

- **Depends on:** T4.1.
- **Steps:**
  1. Parse arguments with `salak_core::cli::parse` **before** creating the
     application; handle `Help`, `Version`, errors (exit code 2) as in the
     Tauri `main`.
  2. Build the session with `Session::from_args`.
  3. Run the application with `app.run_with_args(&[] as &[&str])` so that
     GApplication does not parse the arguments itself.
  4. Use `ApplicationFlags::NON_UNIQUE`. Single instance with "open in the
     existing window" is out of scope; a comment says why (each invocation
     opens its own folder, as today).
- **Done when:** `salak -V`, `salak -h`, `salak --nope` (exit 2) and
  `salak missing` (exit 2 with the error) behave as the Tauri build.

#### T4.3 Window structure

- **Depends on:** T4.1.
- **Steps:**
  1. `src/window.rs`: `adw::ApplicationWindow` containing an
     `adw::ToolbarView` with an `adw::HeaderBar` on top, and as content an
     `adw::ToastOverlay` around an `adw::OverlaySplitView`:
     - sidebar: the tree (T4.5), with the folder name as title;
     - content: a `gtk::Box` with an `adw::TabBar`, an `adw::Banner` and an
       `adw::TabView`.
  2. A `gtk::Stack` switches between this layout and the welcome page (T6.7).
  3. Header bar: a sidebar toggle button bound to `show-sidebar`, and a
     primary `gtk::MenuButton` (`open-menu-symbolic`) with a `gio::Menu`:
     Open File…, Open Folder…, Close Tab, User Guide, Theming Guide,
     About Salak, Quit.
  4. Window title via `salak_core::session::window_title`.
  5. Under a tiling window manager (same variables as `under_tiling_wm()`,
     moved to salak-gtk), call `window.set_decorated(false)` and hide the
     title buttons of the header bar.
- **Done when:** the window shows an empty sidebar, an empty tab area and
  the menu; under sway it has no title buttons.

#### T4.4 Actions and shortcuts

- **Depends on:** T4.3.
- **Steps:**
  1. `src/actions.rs`: `gio::SimpleAction`s on the application or window:
     `win.open-file`, `win.open-folder`, `win.close-tab`, `win.reload`,
     `win.toggle-sidebar`, `win.help(s)` (parameter: page name),
     `win.about`, `app.quit`. Placeholders print a message until their task
     is done.
  2. Accelerators with `set_accels_for_action`: the global keys of the
     section 2 table that use Ctrl or a function key (`Ctrl+O`,
     `Ctrl+Shift+O`, `Ctrl+W`, `Ctrl+Q`, `F1`, `F5`, `Ctrl+R`, `Ctrl+B`).
     `Ctrl+PageUp/PageDown` are built into `adw::TabView`: do not bind them a
     second time.
  3. The single-letter keys (`r`, `b`, `j`, …) are not accelerators, because
     they must not fire while typing in a text field: T6.5 handles them.
- **Done when:** each shortcut triggers its action (placeholder or real).

#### T4.5 Lazy file tree

- **Depends on:** T4.3, T2.3.
- **Steps:**
  1. `src/tree.rs`: a `gio::ListStore` of a small GObject for entries (this
     is the one place where a `glib::Object` subclass is needed: name, path,
     is_dir), wrapped in a `gtk::TreeListModel` with `autoexpand = false`
     whose create function lists the children of folders.
  2. Listing runs off the UI thread: `gio::spawn_blocking` calling
     `salak_core::files::list_dir(resolve_in_root(...))`, then fill the store.
     Children are listed again at every expansion (connect to the row's
     `expanded` notification, clear and refill the child store).
  3. `gtk::ListView` with a `gtk::SignalListItemFactory`: a
     `gtk::TreeExpander` holding an icon (`folder-symbolic` or
     `text-x-generic-symbolic`) and a label.
  4. `gtk::SingleSelection`; activating a file emits a callback
     `on_open(PathBuf)`; activating a folder toggles it.
  5. `reveal(path)`: expands each ancestor folder of `path`, then selects and
     scrolls to its row (`ListView::scroll_to`, GTK 4.12). Used after opening
     a file from a link or the command line.
  6. `mark_active(path)`: CSS class `active` on the row of the open document
     (bold label).
- **Done when:** a folder of a few thousand files opens instantly, folders
  expand lazily, new files appear when a folder is re-expanded.

#### T4.6 Opening files and folders

- **Depends on:** T4.5.
- **Steps:**
  1. `win.open-file`: `gtk::FileDialog` with a `gtk::FileFilter` named
     "Markdown" holding `add_suffix` for each of `MARKDOWN_EXTENSIONS`, initial
     folder = current root; `open_future`. `win.open-folder`:
     `select_folder_future`. Only one dialog at a time (a `Cell<bool>`, as
     `picking` in `ui/app.js`).
  2. The chosen path goes through `Session::open`. If the root changed,
     rebuild the tree and close all tabs; then open the file, if any, in a tab
     and reveal it.
  3. Errors are shown as an `adw::Toast`.
  4. For now a tab shows the raw text of the file in a read-only
     `gtk::TextView` inside a `gtk::ScrolledWindow`. The file is read with
     `gio::File::load_contents_future`, after `resolve_in_root`.
- **Done when:** files and folders open from the dialogs, the command line and
  the tree; a file outside the root opens its own folder.

### Phase 5: native Markdown renderer

#### T5.1 Two-layer renderer design

- **Depends on:** T2.5, T4.6.
- **Steps:**
  1. `src/layout.rs`, **pure Rust, no GTK type**: turns
     `salak_core::markdown::events()` into a list of blocks:
     ```rust
     enum Block {
         Text(Vec<Run>),                    // a paragraph, heading, list item…
         Code { lang: Option<String>, text: String },
         Table { align: Vec<Align>, rows: Vec<Vec<Vec<Run>>> },  // first row = header
         Image { link: Link, alt: String },
         Rule,
     }
     struct Run { text: String, style: Style, link: Option<Link>, anchor: Option<String> }
     ```
     `Style` is a bit set (bold, italic, strike, code, heading level, quote
     depth, list depth, footnote reference…) plus paragraph-level attributes.
     Links are already resolved with `resolve_link`.
  2. `src/buffer.rs`, GTK: applies blocks to a `gtk::TextBuffer` and creates
     child anchors.
  3. Unit tests target `layout.rs` only, so `cargo test` needs no display
     (Debian autopkgtest, see D2). `buffer.rs` is kept thin.
- **Done when:** `layout.rs` and `buffer.rs` exist with paragraphs only, and
  a test checks `events("a *b*")` → one `Text` block with two runs.

#### T5.2 Inline styles and headings

- **Depends on:** T5.1.
- **Steps:**
  1. One `TextTagTable` per document built from a `Theme` (theme values come
     in T7.1; use libadwaita defaults now). Tags:
     - `h1`…`h6`: `scale` 2.0, 1.5, 1.25, 1.0, 0.875, 0.85, `weight` bold,
       `pixels-above-lines`;
     - `bold`, `italic`, `strike`, `code` (monospace `family`, `code-bg`
       background);
     - `paragraph` (`pixels-below-lines`).
  2. `TextView`: `editable(false)`, `cursor_visible(false)`,
     `wrap_mode(WordChar)`, margins `32/40`; centred with a maximum width by
     wrapping it in an `adw::Clamp` (`maximum_size` from `max-width`).
  3. Each heading creates a `TextMark` named after its id (left gravity) at
     its start.
  4. Soft breaks become spaces, hard breaks newlines.
- **Done when:** a document with every inline style and six heading levels
  renders readably in light and dark mode.

#### T5.3 Lists, task lists, block quotes, footnotes

- **Depends on:** T5.2.
- **Steps:**
  1. Lists: per depth a tag with `left-margin` and a negative `indent` (hanging
     bullets). Markers inserted as text: `•`, `◦`, `▪` by depth, `N.` for
     ordered lists (respect the start number).
  2. Task lists: `☐` / `☑` markers (no interactive check box, read-only app).
  3. Block quotes: per depth a tag with `left-margin`, `muted` foreground and
     `paragraph-background` = `subtle-bg`.
  4. Footnotes: the reference is a superscript link (`rise`, `scale` 0.8) to
     an anchor mark at the definition; definitions are rendered at the end
     under a separator, with a `↩` back link to the reference mark (as GitHub).
  5. Layout tests for nesting (list in quote, quote in list, list depth 3).
- **Done when:** the CommonMark list and quote examples of
  `crates/salak-core/help/user-guide.md` render correctly.

#### T5.4 Links

- **Depends on:** T5.2, T4.6.
- **Steps:**
  1. Each link run gets a dedicated anonymous `TextTag` (underline, `link`
     colour); a `HashMap<TextTag, Link>` (or tag data) stores the target.
  2. A `gtk::GestureClick` on the view: on release without a selection, find
     the tags at the iter under the pointer (`window_to_buffer_coords`,
     `iter_at_location`) and dispatch:
     - `Anchor(id)`: scroll to the mark (`scroll_to_mark`, align top);
     - `Help(name)`: open the help tab;
     - `Local { path, fragment }` with `is_openable(path)`: open a tab
       (through `resolve_in_root`), scroll to the fragment, reveal in the tree;
       other local files are ignored;
     - `External(url)`: `gtk::UriLauncher::new(&url).launch(...)`;
     - `Unsupported`: ignored.
  3. `gtk::EventControllerMotion`: `pointer` cursor over links, and a tooltip
     with the target (path or URL).
  4. Keyboard: links are not focusable (as in the Tauri app). Out of scope.
- **Done when:** every link kind of the parity checklist works.

#### T5.5 Code blocks

- **Depends on:** T5.2.
- **Steps:**
  1. Each `Block::Code` is a child anchor holding a `gtk::ScrolledWindow`
     (horizontal scrolling only, `propagate_natural_height`) around a
     read-only, monospace `gtk::TextView` with the `subtle-bg` background and
     rounded corners (`code-block` CSS class in a `gtk::CssProvider` added to
     the display).
  2. Without the `highlight` feature, the text view is plain. T7.2 replaces it
     with a `sourceview5::View` when the feature is on.
  3. Width: the anchored widget follows the width of the document view (see
     T5.7).
- **Done when:** long lines scroll horizontally inside the block, not the page.

#### T5.6 Tables and horizontal rules

- **Depends on:** T5.2.
- **Steps:**
  1. `Block::Table`: child anchor with a `gtk::Grid` of `gtk::Label`s
     (selectable, wrapping, Pango markup built from the runs, escaped with
     `glib::markup_escape_text`), bold header row, `xalign` from the column
     alignment, borders and stripes with the `border` and `subtle-bg` colours
     through CSS classes. Wrapped in a horizontal `ScrolledWindow` for wide
     tables.
  2. Links inside cells use Pango `<a href>` with `activate-link` dispatching
     to the same handler as T5.4 (the `Link` is stored in a side table and the
     href is its index).
  3. `Block::Rule`: child anchor with a `gtk::Separator`.
- **Done when:** a table with alignment, inline code and links renders, and a
  20-column table scrolls instead of widening the window.

#### T5.7 Child widget widths

- **Depends on:** T5.5, T5.6.
- **Steps:**
  1. Child anchor widgets do not follow the text view width by themselves.
     Track the view width (a `gtk::Widget` subclass overriding
     `size_allocate`, or a tick callback comparing `view.width()`), and set
     `width_request` of every anchored widget to the available text width
     (view width minus left and right margins) on change.
  2. Avoid feedback loops: anchored widgets must not request more than the
     available width (their content scrolls instead).
- **Done when:** resizing the window keeps code blocks, tables and rules
  exactly as wide as the text, with no flicker or growing window.

#### T5.8 Images and raw HTML

- **Depends on:** T5.2.
- **Steps:**
  1. `Block::Image` (and inline images, rendered as their own block): child
     anchor with a `gtk::Picture` (`can_shrink`, `ContentFit::ScaleDown`,
     natural size up to the text width), alt text as tooltip and as fallback
     label.
  2. Load asynchronously: `gio::File::for_path` (local, after
     `resolve_in_root`; files outside the root are not shown, as in the
     Tauri app) or `gio::File::for_uri` for `https` (works through gvfs on
     GNOME; on failure, show the alt text). `load_bytes_future` then
     `gdk::Texture::from_bytes`. SVG works through the gdk-pixbuf loader of
     librsvg when installed.
  3. Images never block the rendering: the placeholder is inserted first.
  4. Raw HTML (`Event::Html`, `Event::InlineHtml`): handle `<br>` as a line
     break, `<kbd>…</kbd>` as code style, `<sup>`/`<sub>` with `rise`; drop
     any other tag but keep the text between tags. Comment `<!-- -->`
     dropped. Layout tests for these cases.
  5. Document the HTML limitation in the user guide.
- **Done when:** the README of this repository (banner image, local images)
  renders; raw HTML never shows tags.

#### T5.9 Rendering performance

- **Depends on:** T5.3 to T5.8.
- **Steps:**
  1. Generate a 1 MB Markdown file (e.g. 50 copies of `CHANGELOG.md` and the
     user guide) and measure the time from file read to first paint.
  2. If above 200 ms: run `layout` in `gio::spawn_blocking` (it is pure and
     `Send`), and fill the buffer in chunks with `glib::idle_add_local`.
- **Done when:** the measure is written here and opening a 1 MB file does
  not freeze the window.
- **Measured (2026-10-05, release build, 1 MB made of CHANGELOG, both help
  pages and README repeated, 8442 blocks):** `layout` takes 15 ms, so it stays
  on the main thread. Drawing everything at once took 500 ms and froze the
  window, so `buffer::Filler` draws blocks in chunks (40 ms, then 30 ms per
  idle callback): the first paint comes after about 56 ms and the window stays
  usable, but the whole document is drawn after about 3.5 s, because the text
  view lays out again between chunks. A link to a heading that is not drawn
  yet scrolls as soon as it is.

### Phase 6: feature parity

#### T6.1 Tabs

- **Depends on:** T5.4.
- **Steps:**
  1. `src/tabs.rs`: the model is a `Vec<Tab { path, title, scroll: f64 }>`
     mirrored into `adw::TabView` pages. The page child is the document view.
  2. Open rules of the parity checklist: insert after the active page
     (`TabView::insert` at `selected position + 1`), activate an existing page
     for the same canonical path, merge duplicates when the canonical path is
     known.
  3. Closing: `close-page` signal; after closing the active page, select the
     right neighbour, else the left one (check `adw::TabView`'s default and
     override only if it differs). Last page closed: stop watching, title =
     folder name, show the "Select a Markdown file in the tree." status page
     (or the welcome page without a folder).
  4. Context menu: `TabView::set_menu_model` with the four close actions
     (`tab.close`, `tab.close-others`, `tab.close-right`, `tab.close-left`),
     enabled or disabled in the `setup-menu` signal for the page under the
     pointer.
  5. Middle click closes (built into `adw::TabBar`: check, otherwise add a
     `GestureClick` with button 2).
  6. Only the active page holds a rendered document; switching tabs saves
     the scroll position of the previous one and re-reads the new one (same
     behaviour as `activate()` in `ui/app.js`). A load counter discards slow
     loads finishing after a newer one.
  7. The tab tooltip is the full path (or the help page title).
- **Done when:** every tab item of the parity checklist works.

#### T6.2 File watching and banner

- **Depends on:** T6.1.
- **Steps:**
  1. `src/monitor.rs`: `watch(file, callback) -> gio::FileMonitor`: monitor
     the **parent folder** with `FileMonitorFlags::WATCH_MOVES`, and call back
     when the event concerns the file (`changed`, `created`, `deleted`,
     `moved-in`, `renamed` with the file as source or target). Ignore
     `attribute-changed` (reading the file updates its access time). Keep the
     comment of `watch.rs` explaining why the folder is watched.
  2. The active document is watched; replacing or dropping the monitor stops
     the previous watch.
  3. On change: show the `adw::Banner` "This file has changed on disk." with a
     Reload button (`win.reload`). `Esc` and a Dismiss action hide it.
     Opening another document hides it.
  4. `win.reload` re-reads the active document and restores its scroll
     position.
- **Done when:** saving the open file from vim, from GNOME Text Editor and with
  `echo >> file` shows the banner once per save.

#### T6.3 Help pages

- **Depends on:** T6.1.
- **Steps:** `win.help` opens `salak_core::help::page(name)` in a tab whose
  path is `help:<name>`, rendered with an empty base and root (relative links
  are inert, `help:` links work). Window title: `<page title> - Salak`.
- **Done when:** F1, the menu items and `help:` links open the pages.

#### T6.4 About dialog

- **Depends on:** T4.4.
- **Steps:** `adw::AboutDialog` with application name "Salak", the icon
  `com.tiziolabs.salak`, the data of `salak_core::about::about()` (version,
  developer name, website = repository), `license_type(gtk::License::Custom)`
  with the text "MIT or Apache-2.0", comments "A lightweight Markdown reader".
- **Done when:** Help › About Salak shows it; the repository link opens the
  browser.

#### T6.5 Single-letter keys and focus

- **Depends on:** T6.1, T4.5.
- **Steps:**
  1. A `gtk::EventControllerKey` in the capture phase on the window, active
     only when the focus is not in a text entry and no dialog or menu is open:
     `r` → reload, `b` → toggle sidebar, `Esc` → dismiss the banner, `Tab` →
     switch focus between tree and document (to the document when the sidebar
     is hidden). Keys with Ctrl or Alt are left alone.
  2. Document view: `j`/`k`/`↓`/`↑` scroll by 48 px, `d`/`u` by half a page,
     `g`/`G`/`Home`/`End` to the top/bottom, through the vertical
     `gtk::Adjustment` of the `ScrolledWindow`.
  3. Tree: `j`/`k` move the selection, `l`/`→` expand (or move down if
     already expanded), `h`/`←` collapse (or select the parent folder),
     `Enter`/`o` activate, `g`/`G`/`Home`/`End` first/last row. Check what
     `gtk::ListView` and `TreeExpander` already do with arrows and avoid
     handling them twice.
  4. Focus at startup: the document when a file is given, the tree when only a
     folder is, the Open File button on the welcome page.
- **Done when:** every key of the section 2 table works, and typing `r` in the
  file dialog does not reload.

#### T6.6 Sidebar

- **Depends on:** T4.3.
- **Steps:** `win.toggle-sidebar` toggles `OverlaySplitView::show-sidebar`
  and moves the focus (tree when shown, document when hidden). The header
  bar toggle button stays in sync. Below 600 px wide, the split view collapses
  (`adw::Breakpoint`), which is acceptable.
- **Done when:** `b`, `Ctrl+B` and the button toggle it.

#### T6.7 Welcome page

- **Depends on:** T4.6.
- **Steps:** `adw::StatusPage` with the icon `com.tiziolabs.salak`, title
  "Salak", description "Open a Markdown file, or a folder to browse its
  files.", two pill buttons (Open File… / Open Folder…, with their shortcuts
  in tooltips) and a link button "Read the user guide (F1)". Shown when no
  folder is open; hidden as soon as one is.
- **Done when:** `salak` without arguments shows it; both buttons work.

### Phase 7: themes and highlighting in salak-gtk

#### T7.1 Apply the theme

- **Depends on:** T3.1, T5.3.
- **Steps:**
  1. `src/theme.rs`: build the document `TextTagTable` and a
     `gtk::CssProvider` (document background, code block and table styles)
     from `salak_core::theme::Theme` and the current mode
     (`adw::StyleManager::default().is_dark()`).
  2. Defaults for unset keys: libadwaita colours. Read them with
     `style_context().lookup_color()` (deprecated but still present) or
     hardcode the libadwaita 1.7 palette of `window_bg_color`,
     `view_fg_color`, `accent_color`, `card_bg_color`, with a comment. The
     link colour defaults to the accent colour.
  3. Fonts: `font`, `mono-font`, `font-size` on the tags; `max-width` on the
     `adw::Clamp` (characters × the width of `0` in the font, via
     `pango::Layout`).
  4. Re-apply on `notify::dark` and when the file changes.
  5. Watch `theme::watch_target(path)` with `monitor::watch` (T6.2); apply
     changes live, with no banner. If the folder does not exist at startup,
     live reload starts with the next launch (same rule as today).
  6. Warnings to stderr, and the first one also as a toast.
- **Done when:** editing `theme.ini` changes the open document within a
  second; switching GNOME to dark mode switches the palette.

#### T7.2 Highlighting with GtkSourceView

- **Depends on:** T5.5, T7.1.
- **Steps:**
  1. With the `highlight` feature, code blocks use a `sourceview5::Buffer`
     and `sourceview5::View` (read-only, monospace, no line numbers, no
     current-line highlight).
  2. Language: `sourceview5::LanguageManager::default()`; take the first word
     of the info string, apply the aliases of `highlight.rs`
     (`shell`/`console`/`zsh`/`shell-session` → `sh`, `jsonc` → `json`,
     plus `rs` → `rust`, `py` → `python`, `yml` → `yaml`), try
     `language(id)`, then `guess_language(Some("x.<lang>"), None)`. Unknown
     languages stay plain.
  3. Style scheme: `code-scheme` of the theme for the current mode, else
     `Adwaita` / `Adwaita-dark` (check the names in
     `/usr/share/gtksourceview-5/styles/`); re-applied on `notify::dark`.
  4. Without the feature, T5.5's plain view is used: `cargo build -p
     salak-gtk --no-default-features` must compile and work.
- **Done when:** `sh`, `bash`, `shell`, `console`, `rust`, `rs`, `yaml`, `json`,
  `py`, `css` blocks are highlighted (same list as the test in
  `highlight.rs`); `nope` is plain.

### Phase 8: Linux switches to salak-gtk

#### T8.1 Parity review

- **Depends on:** phases 5 to 7.
- **Steps:** go through the parity checklist of section 2 item by item in
  salak-gtk on Debian 13 and on a GNOME desktop with the latest stable
  release, under GNOME and under sway. Write each failure as a new task in
  this file.
- **Done when:** every item is ticked.

#### T8.2 Desktop integration files inside the crate

- **Depends on:** T4.1.
- **Steps:**
  1. `crates/salak-gtk/data/com.tiziolabs.salak.desktop`, from
     `packaging/salak.desktop`, with `Icon=com.tiziolabs.salak`,
     `StartupWMClass=com.tiziolabs.salak`, `MimeType=text/markdown;`,
     `Exec=salak %f`. Validate with `desktop-file-validate`.
  2. `data/com.tiziolabs.salak.metainfo.xml`: AppStream with id,
     `metadata_license` CC0-1.0, `project_license` `MIT OR Apache-2.0`,
     summary, description, a screenshot URL from the repository, `<releases>`
     with 0.1.0, `<launchable>`, `<content_rating type="oars-1.1"/>`, and
     `<provides><binary>salak</binary></provides>`. Validate with
     `appstreamcli validate --strict`.
  3. `data/salak.1`: man page written by hand in roff (no build dependency):
     NAME, SYNOPSIS, DESCRIPTION, OPTIONS, FILES (`theme.ini`), ENVIRONMENT
     (`XDG_CONFIG_HOME`, the tiling WM variables), SEE ALSO. Check with
     `man -l` and `lintian`'s `groff` warnings (`LC_ALL=C.UTF-8 MANROFFSEQ=''
     MANWIDTH=80 man --warnings -E UTF-8 -l -Tutf8 -Z data/salak.1 >/dev/null`).
  4. Icons: copy `icons/salak.svg` to `data/icons/com.tiziolabs.salak.svg`
     and the 128 px PNG to `data/icons/com.tiziolabs.salak.png`.
  5. `include` in `Cargo.toml` so `cargo package -p salak-gtk --list` contains
     `data/**`.
- **Done when:** the three validators pass.

#### T8.3 Debian package of salak-gtk

- **Depends on:** T8.2.
- **Steps:**
  1. Move `[package.metadata.deb]` to salak-gtk, with `name = "salak"`,
     `depends = "$auto"` and assets: binary, desktop file to
     `usr/share/applications/`, metainfo to `usr/share/metainfo/`, man page
     (gzipped, `usr/share/man/man1/salak.1.gz`), icons to
     `usr/share/icons/hicolor/{scalable,128x128}/apps/`, README to
     `usr/share/doc/salak/`.
  2. Remove `[package.metadata.deb]` from salak-tauri and delete
     `packaging/salak.desktop`.
  3. Build on Debian 13 with `cargo deb -p salak-gtk`; check `Depends:`
     (expect `libgtk-4-1`, `libadwaita-1-0`, `libgtksourceview-5-0`, libc,
     and nothing from WebKit or GTK 3); run `lintian` on the result and fix
     what applies to upstream.
  4. Install it on a clean Debian 13 GNOME system and run the parity smoke
     test.
- **Done when:** the `.deb` installs, launches from the GNOME overview with
  its icon, and lintian reports no error.

#### T8.4 Documentation and release process

- **Depends on:** T8.3.
- **Steps:**
  1. `README.md`: Linux is built with GTK 4 + libadwaita, Windows with Tauri.
     Linux build requirements become
     `sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev`
     (Arch: `gtk4 libadwaita gtksourceview5`); WebKitGTK and libssl are gone.
     Build commands with `-p`. Minimum versions: Debian 13, Ubuntu 24.04 or
     later, or any distribution with GTK 4.18 and libadwaita 1.7 (adjust to
     T4.1).
  2. `RELEASING.md`: two build paths; bump the shared workspace version;
     publish salak-core then salak-gtk to crates.io (T9.1).
  3. `user-guide.md`: platform differences (raw HTML, theme keys).
  4. `CHANGELOG.md`.
- **Done when:** a new contributor can build both applications from the
  README alone.

#### T8.5 Dependency policy

- **Depends on:** T4.1.
- **Steps:**
  1. In `.github/dependabot.yml`, ignore major/minor updates of `gtk4`,
     `libadwaita`, `sourceview5`, `pulldown-cmark`, `glib`, `gio` (they must
     follow Debian unstable), keeping patch updates.
  2. Add a "Dependencies" section to `RELEASING.md`: before raising any
     salak-core or salak-gtk dependency, check that Debian unstable has that
     version (`https://packages.debian.org/sid/librust-<crate>-dev`); new
     dependencies need a strong reason and must already be in Debian.
- **Done when:** the policy is written and dependabot is configured.

#### T8.6 Measure the result

- **Depends on:** T8.3.
- **Steps:** repeat T0.1 for salak-gtk (with and without `highlight`) and
  fill the second column of section 7.
- **Done when:** the table is complete.

### Phase 9: Debian submission preparation

Replaced by [debian-packaging.md](debian-packaging.md), which follows the
standalone source package route (no crates.io) and tracks the work up to
Debian unstable and Ubuntu.

## 7. Measures

| Measure | v0.1.0 (Tauri, Linux) | salak-gtk | salak-gtk without `highlight` |
| --- | --- | --- | --- |
| Binary size | 8.13 MB | 1.01 MB | 1.01 MB |
| `.deb` size | 2.21 MB | 359 KB | 358 KB |
| Linked libraries (`ldd \| wc -l`) | 124 | 130 | 129 |
| `Depends:` | libc6, libcairo2, libdbus-1-3, libgdk-pixbuf-2.0-0, libglib2.0-0t64, libgtk-3-0t64, libjavascriptcoregtk-4.1-0, libsoup-3.0-0, libwebkit2gtk-4.1-0 | libadwaita-1-0, libc6, libglib2.0-0t64, libgtk-4-1, libgtksourceview-5-0, libpango-1.0-0 | libadwaita-1-0, libc6, libglib2.0-0t64, libgtk-4-1, libpango-1.0-0 |
| Crates in the dependency tree | 457 (lock file of the whole project) | 80 (`cargo tree -p salak-gtk -e normal`, Linux) | 78 |
| Memory with `README.md` open | 434 MiB (salak 179 + WebKitWebProcess 205 + WebKitNetworkProcess 51) | 156 MiB | 149 MiB |
| Time to open a 1 MB file (T5.9) | | first paint 56 ms, complete after ~3.5 s | not measured |

Measured on 2026-10-07 on Ubuntu 26.04 (GTK 4.22, libadwaita 1.9,
GtkSourceView 5.18), Wayland, release builds, resident memory
(`ps -o rss`) 8 s after start. The `Depends:` lines come from
`dpkg-shlibdeps` on this machine, so their version bounds are those of
Ubuntu 26.04; a Debian 13 build has not been done yet. `ldd` counts the
libraries pulled in transitively, which is why it barely drops although
the dependency lines are shorter. `cargo-bloat` is not installed and was not run.

## 8. What is lost, on purpose

- Arbitrary CSS in themes: replaced by the format of D6, on both platforms.
- Raw HTML in Markdown on Linux: only `<br>`, `<kbd>`, `<sup>`, `<sub>` are
  interpreted; other tags are dropped, their text kept.
- Selection across a table or a code block and the surrounding text: they
  are separate widgets.
- Debian 12 and other systems older than GTK 4.18 / libadwaita 1.7.
