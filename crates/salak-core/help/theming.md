# Theming guide

The look of documents is set by a theme: a small text file with the colors,
fonts and width of the text. It only affects the document, never the file
tree.

## Where the theme lives

- `$XDG_CONFIG_HOME/salak/theme.ini`, so `~/.config/salak/theme.ini` by
  default, on Linux;
- `%APPDATA%\salak\theme.ini` on Windows;
- or any file given with `salak --theme FILE`.

The file does not need to exist: create it, and Salak picks it up as soon as
it is saved. It is applied live at every save, with no banner, so a theme can
be tuned with the document in sight. Symlinks are followed, so the file can
live in a dotfiles repository.

If the `salak` folder itself does not exist when Salak starts, live reload
only begins with the next launch.

## Format

```ini
# A comment. Lines starting with ";" are comments too.
font = Iosevka Aile
font-size = 17          # a comment after a value needs a space before it

[dark]
bg = #1e1e2e
```

- Lines are `key = value`. Spaces around them are ignored, and the value may
  contain `=`.
- `#` or `;` at the start of a line, or after a space at the end of a value,
  starts a comment. A value that starts with `#` is a color.
- Colors are `#rgb`, `#rrggbb` or `#rrggbbaa` (the last two digits are the
  opacity).
- `[light]` and `[dark]` start a section. Keys before any section apply to
  both modes; keys in a section apply to that mode only and win over the ones
  before it.
- An unknown key, section or invalid value prints a warning on the standard
  error and is ignored. A broken theme never prevents Salak from starting.

## Colors

Colors can be set outside of a section, or in `[light]` and `[dark]`. A key
left out keeps its default value.

| Key | Used for |
| --- | --- |
| `fg` | text |
| `muted` | quotes, footnotes, small headings |
| `bg` | background |
| `border` | rules, tables, headings |
| `subtle-bg` | code blocks, table stripes |
| `code-bg` | inline code |
| `link` | links |
| `mark` | highlighted text |

## Light and dark

Salak follows the light or dark preference of the system, and each mode has
its own palette:

```ini
# Both modes
font = Iosevka Aile

[light]
fg = #4c4f69
bg = #eff1f5

[dark]
fg = #cdd6f4
bg = #1e1e2e
```

## Code blocks

How code is highlighted depends on the version of Salak:

- `hl-*` keys set the colors of the syntax highlighting of the Tauri
  version (Windows). They can be set outside of a section, or in `[light]`
  and `[dark]`.

  | Key | Used for |
  | --- | --- |
  | `hl-comment` | comments |
  | `hl-keyword` | keywords, `fn`, `let` |
  | `hl-string` | strings |
  | `hl-constant` | numbers, options like `-y` |
  | `hl-function` | functions, shell commands |
  | `hl-type` | types |
  | `hl-variable` | variables |
  | `hl-tag` | HTML tags, Markdown headings |
  | `hl-inserted` | added lines of a diff |
  | `hl-deleted` | removed lines of a diff |

- `code-scheme` is the name of a GtkSourceView style scheme, for the GTK
  version (Linux): `code-scheme = Adwaita-dark`. It can be set outside of a
  section, or in `[light]` and `[dark]`.

A key that does not apply to the version in use is accepted and ignored.
Highlighting can be left out when Salak is built: code blocks then only use
`subtle-bg` and `fg`.

## Layout and fonts

These keys are not per mode, so they go outside of a section.

| Key | Meaning | Default |
| --- | --- | --- |
| `font` | font of the text | the system font |
| `mono-font` | font of code | a monospace font |
| `font-size` | size of the text, in pixels | `16` |
| `max-width` | width of the text, in characters | about 100 |

Fonts must be installed on the system. A font name is written as is, without
quotes.

## A full example

```ini
# ~/.config/salak/theme.ini: Catppuccin
font = Iosevka Aile
mono-font = Iosevka
font-size = 17
max-width = 72

[light]
fg = #4c4f69
muted = #6c6f85
bg = #eff1f5
border = #ccd0da
subtle-bg = #e6e9ef
code-bg = #dce0e8
link = #1e66f5
mark = #df8e1d40
hl-comment = #9ca0b0
hl-keyword = #8839ef
hl-string = #40a02b
hl-constant = #fe640b
hl-function = #1e66f5
hl-type = #df8e1d
hl-variable = #d20f39
hl-tag = #179299
hl-inserted = #40a02b
hl-deleted = #d20f39

[dark]
fg = #cdd6f4
muted = #a6adc8
bg = #1e1e2e
border = #45475a
subtle-bg = #181825
code-bg = #313244
link = #89b4fa
mark = #f9e2af40
hl-comment = #6c7086
hl-keyword = #cba6f7
hl-string = #a6e3a1
hl-constant = #fab387
hl-function = #89b4fa
hl-type = #f9e2af
hl-variable = #f38ba8
hl-tag = #94e2d5
hl-inserted = #a6e3a1
hl-deleted = #f38ba8
code-scheme = Adwaita-dark
```

## Migrating from style.css

Up to version 0.1.0, the theme was a CSS file. It is no longer read, and
`--css` was replaced by `--theme`. Arbitrary CSS is not supported anymore:
a theme can only set the keys above.

| In `style.css` | In `theme.ini` |
| --- | --- |
| `--fg: #cdd6f4;` | `fg = #cdd6f4` |
| `--subtle-bg`, `--code-bg`, `--hl-keyword`, … | the same name, without `--` |
| `@media (prefers-color-scheme: dark) { … }` | a `[dark]` section |
| `.markdown-body { font-family: "X"; }` | `font = X` |
| `pre, code { font-family: "Y"; }` | `mono-font = Y` |
| `.markdown-body { font-size: 17px; }` | `font-size = 17` |
| `.markdown-body { max-width: 72ch; }` | `max-width = 72` |
