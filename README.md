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

<img width="983" height="863" alt="Screenshot" src="https://github.com/user-attachments/assets/7b7a173e-8908-4d79-9d5c-622f149cf194" />

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
on the cursor line — the statusbar counts them as `link 2/3`, and the ones you
have not landed on are underlined — `Enter` follows the focused one, and `h` and
`l` walk your history. Clicking a link follows it directly, whichever line it is
on.

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

Five themes ship in the binary:

| Name | Appearance |
| --- | --- |
| `handbook` (default) | your terminal's own background, with colours of its own on top |
| `ansi` | inherits your terminal's own 16 colours |
| `kanagawa-dragon` | dark, after [kanagawa.nvim](https://github.com/rebelot/kanagawa.nvim) |
| `catppuccin-mocha` | dark |
| `catppuccin-latte` | light |

`handbook` leaves the background and the body text to your terminal — a reader
who has not chosen a theme has already chosen those — and names a colour for
everything else, because the sixteen ANSI colours cannot promise what matters:
`dark_gray` is bright black, which most schemes make a *mid* grey, so under
`ansi` the cursor line is a heavy washed-out bar. It is tuned against a dark
terminal; on a light one, `catppuccin-latte` is the better start. `ansi` remains
the only theme that asserts nothing at all.

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

Those defaults are `ansi`'s, not `handbook`'s: a partial file merges over the
terminal's own sixteen colours, whichever theme happens to be shipped as the
default. So the two-line file above is `ansi` with a different accent. To start
from `handbook` or one of the others, copy its `[palette]` out of
[`themes/`](themes) and edit that.

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
| `subtle` | `dark_gray` | the code band, in themes that ask for one |
| `cursor` | `dark_gray` | the cursor line |
| `selection_background` / `selection_foreground` | `blue` / `white` | the focused link, copied text |
| `error` | `red` | broken links, error messages |
| `success` | `green` | completed task boxes |
| `warning` | `yellow` | inline code, search matches |
| `accent` | `cyan` | headings 1–2, list bullets, popup border |
| `chrome` | `cyan` | the header title |
| `highlight` | `blue` | links, headings 3–6 |
| `notice` | `magenta` | wikilinks, transient messages, current match |

`cursor` falls back to `subtle` when a theme names one and not the other, so a
theme file written while the two were a single slot still looks the way it did.

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
`link_focused`, `link_unfocused`, `link_broken`, `image`, `footnote`, `html`, `table_header`,
`table_border`, `hr`, `header_title`, `hint`, `status`, `status_notice`,
`status_error`, `cursor_line`, `search_match`, `search_current`, `selection`
and `help_window`. Each takes `fg`, `bg` and `modifiers` (`bold`, `italic`,
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
| `Tab` / `Shift-Tab` | Cycle the links on the cursor line, counted on the statusbar |
| `Enter` | Follow the focused link |
| `o` | Open an external link in the browser |
| `y` / `Y` | Copy the cursor line / the focused link's target |
| `h` / `Backspace`, `l` | History back / forward |
| `/`, then `n` / `N` | Search, next / previous match |
| `?` | Help |
| `Esc` | Close the overlay, or clear the search |
| `q`, `Ctrl-C` | Quit |
| Left click | Follow the link under the pointer, on release |
| Left drag | Select text, and copy it when the button is let go |

Copying uses the platform's own clipboard tool when one is on `PATH` —
`pbcopy` on macOS, `wl-copy` under Wayland, `xclip` or `xsel` under X11,
`clip.exe` on Windows. It also always sends an `OSC 52` escape, which is what
carries a copy back over `ssh`. Terminals differ on whether they accept that
escape: Apple Terminal and GNOME Terminal ignore it entirely, tmux wants
`set -g set-clipboard on`, and a few others have it off by default — hence the
native tool, which does not care. If the copy fails outright the statusbar says
so rather than claiming success.

The wheel scrolls three lines a notch and brings the reading position with it:
the cursor line is pulled to the nearest line still on screen, the top row when
you scroll down and the bottom row when you scroll up. So `j` after a scroll
carries on from where you are looking, `Tab` and `Enter` act on a line you can
see, and the statusbar's `line X/Y` is never describing something off screen.
Scrolling away and back does not restore the line you started on — the cursor
came along.

Dragging selects the text as it is painted — wrapped where the page wrapped,
without the gutter or the colours — and letting go copies it. Capturing the
mouse takes your terminal's own selection away, so `--no-mouse` turns it off and
hands it back; most terminals also let you hold `Shift` while dragging to select
through a capturing program.

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
