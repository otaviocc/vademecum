# vademecum

[![CI](https://img.shields.io/github/actions/workflow/status/otaviocc/vademecum/ci.yml?branch=main)](https://github.com/otaviocc/vademecum/actions/workflows/ci.yml)
[![GitHub release](https://img.shields.io/github/v/release/otaviocc/vademecum)](https://github.com/otaviocc/vademecum/releases/latest)
[![crates.io](https://img.shields.io/crates/v/vademecum.svg)](https://crates.io/crates/vademecum)
[![license](https://img.shields.io/github/license/otaviocc/vademecum.svg)](https://github.com/otaviocc/vademecum/blob/main/LICENSE)

Read Markdown without leaving the terminal. Point `vademecum` at a file and it
renders headings, tables, task lists, block quotes and syntax-highlighted code
the way a documentation site would — then lets you walk a whole collection of
notes by following `[[wikilinks]]` with `Enter`.

It is one Rust binary with nothing to install alongside it. Themes and syntax
definitions are compiled in.

*vade mecum*, Latin: "go with me" — the handbook you carry.

```
 vademecum · README.md              ? help  / search  ⇥ link  ⏎ follow  h/l back/fwd  q quit
────────────────────────────────────────────────────────────────────────────────────────────
     file, because editors save by rename.
 13. Mouse — the wheel scrolls, on by default. --no-mouse turns capture off and
     gives the terminal back its own text selection.

 Usage


 vademecum README.md                   # interactive TUI
 vademecum --plain README.md           # force stdout mode even on a TTY
 vademecum README.md | less -R         # stdout mode is automatic when piped
 vademecum --color always README.md > out.ansi
 vademecum --width 80 README.md        # wrap width (default: min(terminal - 2, 100))
 vademecum --theme catppuccin-mocha README.md
 vademecum --config ~/my-theme.toml README.md
 vademecum --root ~/notes README.md    # vault root for wikilink lookup
 vademecum --watch README.md
────────────────────────────────────────────────────────────────────────────────────────────
README.md · line 87/964 · 8% · match 1/1
```

## Install

### Cargo

```sh
cargo install vademecum --locked
```

### Prebuilt binaries

Grab an archive from the [latest release](https://github.com/otaviocc/vademecum/releases/latest)
and put the binary on your `PATH`.

| Platform | Archive |
| --- | --- |
| Linux, x86-64 | `vademecum-<tag>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux, ARM64 | `vademecum-<tag>-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Intel and Apple silicon | `vademecum-<tag>-macos-universal.tar.gz` |
| Windows, x86-64 | `vademecum-<tag>-x86_64-pc-windows-msvc.zip` |

The macOS build is a universal binary, so there is nothing to choose between.

### From source

```sh
git clone https://github.com/otaviocc/vademecum.git
cd vademecum
cargo install --path . --locked
```

Requires [Rust](https://rustup.rs) 1.88 or newer to build.

## Quick start

```sh
vademecum README.md                   # open the pager
vademecum --watch NOTES.md            # re-render whenever the file changes
vademecum --root ~/notes index.md     # follow [[wikilinks]] across a vault
vademecum --theme catppuccin-mocha README.md
vademecum README.md | less -R         # piped output is styled text, no TUI
cat notes.md | vademecum -            # read from stdin
```

Piping is automatic: when stdout is not a terminal you get styled text instead
of the full-screen pager, so `vademecum notes.md | grep TODO` behaves. `--plain`
forces that even on a terminal.

## Reading a collection

`[[wikilinks]]`, `[[target|alias]]`, `[[target#Heading]]` and ordinary relative
links like `[text](other.md#section)` are all followable. `Tab` cycles the links
on the cursor line, `Enter` follows one, `h` and `l` walk your history. Clicking
a link follows it directly, whichever line it is on.

A target is looked for beside the current document first, then anywhere under
the **vault root** — the nearest ancestor containing `.obsidian/`, or whatever
you pass to `--root`. Hidden directories are skipped; symlinked ones are
followed, so a shared folder linked into your notes works. Two different files
of the same name are reported as ambiguous rather than guessed at, and a link
that resolves to nothing renders struck through.

`o` opens an external link in your browser. In piped output, external links
become OSC-8 hyperlinks, so terminals that support them (Kitty, iTerm2, WezTerm,
Ghostty, tmux 3.4+) make them clickable.

## Live reload

`--watch` re-renders the open document whenever it changes on disk, keeping you
on the line you were reading. Edit in one window, read in the other.

It watches the file's *directory* rather than the file, because editors save by
renaming a new file into place — so a document you delete and restore comes back
on its own, and a note you create elsewhere in the vault stops being a broken
link without a restart.

## Theming

Four themes ship in the binary:

| Name | Appearance |
| --- | --- |
| `ansi` (default) | inherits your terminal's own 16 colours |
| `kanagawa-dragon` | dark, after [kanagawa.nvim](https://github.com/rebelot/kanagawa.nvim) |
| `catppuccin-mocha` | dark |
| `catppuccin-latte` | light |

`ansi` is the default because a reader who has not chosen a theme has already
chosen one — their terminal's.

```sh
vademecum --theme catppuccin-mocha README.md
vademecum --list-themes
vademecum --list-syntax-themes
```

### Your own theme

Drop a file at `~/.config/vademecum/theme.toml` (`%APPDATA%\vademecum\` on
Windows) and it becomes the default. Put more in `themes/` beside it and select
them by filename with `--theme`. `--config <file>` beats both.

Theme files are **partial** — anything you leave out keeps its default, so a
two-line file is a valid theme:

```toml
[palette]
accent = "#89b4fa"
```

A colour is an 8-bit index (`208`), hex (`"#89b4fa"`), `"reset"` for the
terminal's own, or one of the 16 ANSI names: `black`, `red`, `green`, `yellow`,
`blue`, `magenta`, `cyan`, `gray`, `dark_gray`, `light_red`, `light_green`,
`light_yellow`, `light_blue`, `light_magenta`, `light_cyan`, `white`.

Every element derives from the palette:

| Slot | Default | Drives |
| --- | --- | --- |
| `background` | `reset` | help popup |
| `foreground` | `reset` | body text, code |
| `muted` | `dark_gray` | rules, table borders |
| `muted_text` | `gray` | hints, quotes, images, footnotes, HTML |
| `subtle` | `dark_gray` | the cursor line |
| `selection_background` / `selection_foreground` | `blue` / `white` | the focused link |
| `error` | `red` | broken links, error messages |
| `success` | `green` | completed task boxes |
| `warning` | `yellow` | inline code, search matches |
| `accent` | `cyan` | headings 1–2, list bullets, popup border |
| `chrome` | `cyan` | the header title |
| `highlight` | `blue` | links, headings 3–6 |
| `notice` | `magenta` | wikilinks, transient messages, current match |

Individual elements can be overridden too:

```toml
[elements.heading1]
fg = "accent"
modifiers = ["bold", "underline"]

[elements.code_block]
bg = "subtle"     # `none` removes a background; `reset` paints the terminal's
```

The element names are `paragraph`, `heading1`–`heading6`, `emphasis`, `strong`,
`strikethrough`, `inline_code`, `code_block`, `code_block_lang`, `quote`,
`list_bullet`, `list_number`, `task_done`, `task_todo`, `link`, `wikilink`,
`link_focused`, `link_broken`, `image`, `footnote`, `html`, `table_header`,
`table_border`, `hr`, `header_title`, `hint`, `status`, `status_notice`,
`status_error`, `cursor_line`, `search_match`, `search_current` and
`help_window`. Each takes `fg`, `bg` and `modifiers` (`bold`, `italic`,
`underline`, `dim`, `reversed`, `crossed_out`), and each falls back
independently.

### Code colours

Fenced code is highlighted with [syntect](https://github.com/trishume/syntect).
Pick the token colours with `syntax_theme`, naming one of the bundled
`.tmTheme` files or one of your own in `~/.config/vademecum/syntax-themes/`:

```toml
syntax_theme = "base16-ocean.dark"
```

Sublime Text's default languages are bundled, plus Swift, TypeScript, Kotlin and
TOML. Origins and licences for the added ones are in
[`syntaxes/LICENSES.md`](syntaxes/LICENSES.md).

## Keys

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | Move down / up |
| `d` / `u`, `Ctrl-D` / `Ctrl-U` | Half page |
| `Space` / `b`, `PgDn` / `PgUp` | Page |
| `g` / `G`, `Home` / `End` | Top / bottom |
| `Tab` / `Shift-Tab` | Cycle links on the cursor line |
| `Enter` | Follow the focused link |
| `o` | Open an external link in the browser |
| `h` / `Backspace`, `l` | History back / forward |
| `/`, then `n` / `N` | Search, next / previous match |
| `?` | Help |
| `Esc` | Close the overlay, or clear the search |
| `q`, `Ctrl-C` | Quit |
| Left click | Move the cursor to the clicked line, or follow the link under the pointer |

The wheel scrolls three lines a notch and moves nothing else — the cursor keeps
its line and may scroll off screen, so browsing away and back leaves your place
exactly as it was. The next motion key brings the view back to the cursor before
it moves, which is why `line X/Y` always names a line you can see. To read
somewhere you scrolled to, click the line.

Capturing the mouse takes your terminal's own text selection away, so
`--no-mouse` turns it off; most terminals also let you hold `Shift` while
dragging to select through a capturing program.

## Flags

| Flag | Meaning |
| --- | --- |
| `<path>` or `-` | Markdown file to open, or stdin |
| `--plain` | Styled text on stdout even when it is a terminal |
| `--color <auto\|always\|never>` | Colour in stdout mode. `auto` follows the terminal; `NO_COLOR` forces `never` |
| `--width <n>` | Wrap width. Default `min(terminal − 2, 100)` |
| `--theme <name>` | Built-in or user theme |
| `--config <file>` | Explicit theme file, beats `--theme` |
| `--root <dir>` | Vault root for wikilink lookup |
| `--watch` | Re-render on change. Needs a file |
| `--no-mouse` | Give up wheel scrolling, keep the terminal's own selection |
| `--list-themes`, `--list-syntax-themes` | Print available names and exit |
| `--resolve-links` | Print every link and where it resolves, then exit |

## License

MIT.
