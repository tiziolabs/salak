# Theming guide

The look of documents can be changed with a CSS style sheet. It is applied on
top of the default style and only affects the document, never the file tree.

## Where the style sheet lives

- `$XDG_CONFIG_HOME/salak/style.css`, so `~/.config/salak/style.css` by
  default, on Linux;
- `%APPDATA%\salak\style.css` on Windows;
- or any file given with `salak --css FILE`.

The file does not need to exist: create it, and Salak picks it up as soon as
it is saved. It is reloaded live at every save, so a theme can be tuned with
the document in sight. Symlinks are followed, so the file can live in a
dotfiles repository.

If the `salak` folder itself does not exist when Salak starts, live reload
only begins with the next launch.

## Colors

The simplest theme overrides the variables of the default style. They are
declared on `:host`, the root of the document:

```css
:host {
  --fg: #cdd6f4;        /* text */
  --muted: #a6adc8;     /* quotes, footnotes, small headings */
  --bg: #1e1e2e;        /* background */
  --border: #45475a;    /* rules, tables, headings */
  --subtle-bg: #181825; /* code blocks, table stripes */
  --code-bg: #313244;   /* inline code */
  --link: #89b4fa;      /* links */
  --mark: #f9e2af40;    /* highlighted text */
}
```

Any variable left out keeps its default value.

## Light and dark

The default style has a light and a dark palette, chosen by the system
preference. A theme can do the same with a media query:

```css
:host {
  --bg: #eff1f5;
  --fg: #4c4f69;
}

@media (prefers-color-scheme: dark) {
  :host {
    --bg: #1e1e2e;
    --fg: #cdd6f4;
  }
}
```

A theme that sets its colors outside of a media query applies them in both
modes.

## Code blocks

Code blocks are highlighted with classes named after the scopes of the
language grammars, prefixed with `hl-`: the scope `keyword.control.shell`
gives the classes `hl-keyword hl-control hl-shell`. Their colors are
variables too:

```css
:host {
  --hl-comment: #6c7086;
  --hl-keyword: #cba6f7;    /* keywords, `fn`, `let` */
  --hl-string: #a6e3a1;
  --hl-constant: #fab387;   /* numbers, options like `-y` */
  --hl-function: #89b4fa;   /* functions, shell commands */
  --hl-type: #f9e2af;
  --hl-variable: #f38ba8;
  --hl-tag: #94e2d5;        /* HTML tags, Markdown headings */
  --hl-inserted: #a6e3a1;   /* diffs */
  --hl-deleted: #f38ba8;
}
```

Classes can also be targeted directly, for finer control:

```css
.hl-comment { font-style: normal; }
.hl-keyword.hl-control { font-weight: 600; }
```

Highlighting can be left out when Salak is built: code blocks then only use
`--subtle-bg` and `--fg`.

## Layout and fonts

Any element of the document can be styled with plain CSS. The document is
wrapped in an `article` with the class `markdown-body`:

```css
.markdown-body {
  max-width: 72ch;
  font-family: "Iosevka Aile", sans-serif;
  font-size: 17px;
}

h1, h2 { border-bottom: none; }

pre, code { font-family: "Iosevka", monospace; }
```

The style sheet wins over the default style at equal specificity, since it
comes after it.

## Limits

For security, the style sheet cannot load fonts or other style sheets: fonts
must be installed on the system (`@font-face` rules can only refer to them
with `local()`), and `@import` rules are ignored.
