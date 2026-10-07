# Changelog

## Unreleased

- **Linux:** Salak is now a native GTK 4 and libadwaita application, without
  WebKitGTK. It needs GTK 4.16 and libadwaita 1.6 (Debian 13 or later,
  Ubuntu 25.04 or later). Raw HTML in Markdown is no longer rendered, except
  `<br>`, `<kbd>`, `<sup>` and `<sub>`. The application identifier is
  `com.tiziolabs.salak`: the desktop entry, the icon and the sway `app_id`
  use it. Windows keeps the Tauri application.
- The Debian package ships AppStream metadata and a man page.
- **Breaking:** themes are now small INI files (`theme.ini`) instead of CSS
  (`style.css`), which is no longer read. The option `--css` is replaced by
  `--theme`. A theme sets colors for the light and dark modes, fonts, the
  font size and the width of the text; mistakes are reported on the standard
  error instead of breaking the document. See Help › Theming Guide, which
  explains how to migrate.

## 0.1.0 - 2026-10-04

First release.

- File tree of a folder, loaded lazily and driven by the keyboard.
- GitHub-like rendering: tables, footnotes, task lists, strikethrough, and
  syntax highlighting of code blocks.
- Tabs, one per opened document, with a menu to close several at once.
- Light and dark themes following the system, and a user style sheet applied
  live while it is edited.
- A banner offers to reload a file changed on disk.
- Welcome page, File menu and shortcuts to open a file or a folder.
- Help menu with a user guide, a theming guide and an About dialog.
- Button and key to hide the sidebar.
- Debian package, Windows installer and portable Windows build.
