# User guide

Salak is a read-only Markdown reader: it shows the Markdown files of a folder
in a tree, and renders the selected one.

## Opening files

- **File › Open File…** (`Ctrl+O`) opens a Markdown file. A file inside the
  folder already opened is shown in its tree; any other file opens its own
  folder.
- **File › Open Folder…** (`Ctrl+Shift+O`) browses another folder.

From a terminal:

```sh
salak                 # welcome page, to open a file or a folder
salak .               # browse the current directory
salak ~/notes         # browse a folder
salak README.md       # open a file, browsing its folder
salak --css dark.css  # use another style sheet
```

The tree only lists folders and Markdown files (`.md`, `.markdown`, `.mdown`,
`.mkd`, `.mkdn`). Hidden files are skipped.

## Reading

Links to other Markdown files of the folder open in Salak, links to web pages
open in the default browser, and `#section` links scroll to their heading.

Every document opens in its own tab, right after the current one; a document
already open is brought to the front. A tab is closed with its `×` button, a
middle click or `Ctrl+W`. A right click on a tab offers to close it, the other
tabs, or the tabs on its right or its left.

When the opened file changes on disk, a banner offers to reload it. A tab in
the background is read again when it is brought back to the front.

## Keys

| Key | Action |
| --- | --- |
| `Ctrl+O` | Open a file |
| `Ctrl+Shift+O` | Open a folder |
| `Ctrl+W` | Close the tab |
| `Ctrl+PageDown` / `Ctrl+PageUp` | Next / previous tab |
| `Ctrl+Q` | Quit |
| `F1` | Show this guide |
| `Tab` | Switch focus between the tree and the document |
| `↑` `↓` / `j` `k` | Move in the tree, scroll the document |
| `←` `→` / `h` `l` | Collapse / expand a folder |
| `Enter` / `o` | Open the selected file or toggle the folder |
| `g` / `G` | Go to top / bottom |
| `d` / `u` | Scroll half a page down / up |
| `r`, `F5`, `Ctrl+R` | Reload the document |
| `Esc` | Dismiss the "file changed" banner |
| `b`, `Ctrl+B` | Toggle the sidebar |

## Appearance

Salak follows the light or dark preference of the system. Its look can be
changed with a style sheet: see the [theming guide](help:theming).

## Security

Markdown files may contain raw HTML. Salak removes anything that could run
code, never runs scripts, and only reads files inside the opened folder.
