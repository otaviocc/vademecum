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

### Homebrew

```sh
brew install otaviocc/apps/vademecum
```

The formula builds from source and brings its own Rust, so there is nothing to
install first — but it does compile, which takes a minute.

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
| `t` | Table of contents |
| `p` | Properties |
| `?` | Help |
| `Esc` | Close the overlay, or clear the search |
| `q`, `Ctrl-C` | Quit |
| Left click | Follow the link under the pointer, on release |
| Left drag | Select text, and copy it when the button is let go |

`t` opens the document's headings in a scrollable list, indented by level, with
a `›` marking the selection. It opens on the section being read. The reading
keys all work in it — `j` / `k`, the arrows, `d` / `u`, `Space` / `b`, `g` / `G`,
and the wheel — and `Enter` or a left click jumps to the selected heading, which
lands on the first line of the viewport with the cursor on it. `t`, `q` or `Esc`
closes the list without moving. A jump is recorded in the history, so `h` returns
to where you were reading.

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

## Properties

A note that opens with YAML or TOML frontmatter has its keys and values read out
of the block. `p` shows them in a window over the document, the way `t` shows the
headings:

```
┌ Properties ───────────────────────────────┐
│› title    Long Note                       │
│  tags     inbox, ideas                    │
│  created  2026-09-08                      │
└───────────────────────────────────────────┘
```

The reading keys walk it — `j` / `k`, the arrows, `d` / `u`, `Space` / `b`,
`g` / `G`, and the wheel — a click selects a row, and `y` copies the selected
value, which is the quickest way to get a tag or a date out of a note. `p`, `q`
or `Esc` closes it, and a click outside the box does too. The document behind is
never touched: opening the window moves nothing and scrolls nothing.

The frontmatter itself is **not** rendered as part of the document, so it does
not appear in the body, `/` does not search it, and `y` outside the window does
not copy it. The window is where it lives. The `title` key becomes the pager's
header title, which is why a note can be called something other than its
filename.

`---` and `+++` both open a block, closed by `---`, `...` or `+++`, and only on
the file's first line — a `---` further down is a horizontal rule, as it should
be.

Values are read without a YAML parser, which is a deliberate limit: keys are
split on the first `:` or `=`, one layer of quotes comes off, `[a, b]` loses its
brackets, and a list or a map written across several indented lines folds into
one row (`tags    inbox, ideas`). A line that fits none of that is shown as it
was written rather than dropped, so nothing in your frontmatter can go missing —
but deeply structured frontmatter is flattened, not rendered faithfully.

Piped output has no windows, so it omits properties entirely, exactly as it
always has.

## Following links

### Between your notes

`[[wikilinks]]`, `[[target|alias]]`, `[[target#Heading]]` and ordinary relative
links like `[text](other.md#section)` are all followable.

`Tab` and `Shift-Tab` cycle the links **on the cursor line** — the statusbar
counts them as `link 2/3`, and the ones you have not landed on are underlined.
`Enter` follows the focused one. Clicking a link follows it directly, whichever
line it is on.

Following a link lands you at the top of the target, or at the heading if the
link named one. `h` (or `Backspace`) and `l` walk your history, which is not
capped, and going back returns you to the exact line you left rather than to the
top. `Y` copies the focused link's target as it is written in the document,
fragment and all.

Images and footnote references are not links: they are rendered as text, so
`Tab` skips them.

### How a target is found

A target is looked for beside the current document first, then anywhere under
the **vault root**. The root is the nearest ancestor directory containing
`.obsidian/`; `--root <dir>` names one instead, and with neither the root is
simply the document's own directory.

The vault index holds `.md` files only, keyed by filename without the
extension and matched case-insensitively — so `[[Note]]` finds `note.md`, and
`[[diagram.png]]` will not resolve however the file is spelled. A target
written with an extension is tried as written before `.md` is appended.
Hidden directories are skipped; symlinked ones are followed, so a shared folder
linked into your notes works, and a file reachable under two names counts once.

Two different files of the same name are reported as ambiguous rather than
guessed at, and a link that resolves to nothing renders struck through. A
`--root` that does not exist, or that is a file, is an error before anything is
rendered.

The vault in this repository has one of each case:

```
tests/fixtures/vault/
├── .obsidian/          ← marks the vault root
├── index.md
├── note.md             ← [[note]] resolves beside index.md, without searching
├── spaced note.md      ← reached as [a spaced note](spaced%20note.md)
├── a/dup.md            ┐ the same name in two places, so
├── b/dup.md            ┘ [[dup]] is ambiguous rather than guessed
└── nested/note.md      ← [the nested note](nested/note.md)
```

### External links

`o` opens the focused external link in your browser. `Enter` does not — it
follows links inside your notes, and does nothing on an external one, so the two
can never be confused. `mailto:` addresses and bare autolinks count as external.

In piped output, external links become OSC-8 hyperlinks, so terminals that
support them (Kitty, iTerm2, WezTerm, Ghostty, tmux 3.4+) make them clickable.

### Checking links

`--resolve-links` prints every link in a document and what it resolves to, then
exits. It needs no terminal and loads no theme, so it works in a script, and it
is the quickest way to find out why a link is not behaving:

```console
$ vademecum --resolve-links tests/fixtures/vault/index.md
local     nested/note.md            -> tests/fixtures/vault/nested/note.md
local     nested/note.md#a-heading  -> tests/fixtures/vault/nested/note.md
wiki      note                      -> tests/fixtures/vault/note.md
wiki      note                      -> tests/fixtures/vault/note.md
wiki      note#A Heading            -> tests/fixtures/vault/note.md
wiki      missing                   -> (broken)
wiki      dup                       -> (ambiguous: tests/fixtures/vault/a/dup.md, tests/fixtures/vault/b/dup.md)
local     spaced%20note.md          -> tests/fixtures/vault/spaced note.md
external  https://example.com       -> -
local     #index                    -> tests/fixtures/vault/index.md
```

That is the whole output for that file, unedited — `note` appears twice because
`[[note]]` and `[[note|the note]]` are the same target under two spellings, and
an alias is reported by what it points at. A document with no links prints
nothing.

## Live reload

`--watch` re-renders the open document whenever it changes on disk, keeping you
on the line you were reading. Edit in one window, read in the other.

It watches the file's *directory* rather than the file, because editors save by
renaming a new file into place — so a document you delete and restore comes back
on its own, and a note you create elsewhere in the vault stops being a broken
link without a restart.

## Themes

Thirteen themes ship in the binary, listed in the order `--list-themes` prints
them: the default first, the merge base second, then the rest alphabetically.

| Name | Appearance |
| --- | --- |
| `handbook` (default) | your terminal's own background, with colours of its own on top |
| `ansi` | inherits your terminal's own 16 colours |
| `catppuccin-latte` | light, after [Catppuccin](https://github.com/catppuccin/catppuccin) |
| `catppuccin-mocha` | dark, after [Catppuccin](https://github.com/catppuccin/catppuccin) |
| `gruvbox-dark` | dark, after [gruvbox](https://github.com/morhetz/gruvbox) |
| `gruvbox-light` | light, after [gruvbox](https://github.com/morhetz/gruvbox) |
| `kanagawa-dragon` | dark, after [kanagawa.nvim](https://github.com/rebelot/kanagawa.nvim) |
| `nord` | dark, after [Nord](https://www.nordtheme.com) |
| `solarized-dark` | dark, after [Solarized](https://ethanschoonover.com/solarized/) |
| `solarized-light` | light, after [Solarized](https://ethanschoonover.com/solarized/) |
| `tokyo-night` | dark, after [Tokyo Night](https://github.com/folke/tokyonight.nvim) |
| `tokyo-night-day` | light, after [Tokyo Night](https://github.com/folke/tokyonight.nvim) |
| `vesper` | dark, near-monochrome, after [Vesper](https://github.com/raunofreiberg/vesper) |

### Using a built-in theme

```sh
vademecum --theme nord README.md
vademecum --list-themes
```

`handbook` leaves the background and the body text to your terminal — a reader
who has not chosen a theme has already chosen those — and names a colour for
everything else, because the sixteen ANSI colours cannot promise what matters:
`dark_gray` is bright black, which most schemes make a *mid* grey, so under
`ansi` the cursor line is a heavy washed-out bar. It is tuned against a dark
terminal; on a light one, pick one of the four light themes, which name a
background of their own rather than borrowing yours. `ansi` remains the only
theme that asserts nothing at all.

To make one your default, put a single line in `~/.config/vademecum/theme.toml`
(`%APPDATA%\vademecum\theme.toml` on Windows — macOS uses the `~/.config` path,
not `~/Library`):

```toml
base = "handbook"
```

`--theme` and `--config <file>` still beat that file.

### Writing your own theme

Theme files are **partial** — anything you leave out keeps its default, so a
two-line file is a valid theme:

```toml
[palette]
accent = "#89b4fa"
```

Those defaults are `ansi`'s, not `handbook`'s: a file that names no base merges
over the terminal's own sixteen colours, whichever theme happens to be shipped
as the default. So the two-line file above is `ansi` with a different accent.

To start from something else, name it as your **base**:

```toml
base = "catppuccin-mocha"

[palette]
accent = "#89b4fa"
```

Everything you leave out then comes from that theme rather than from `ansi`, all
the way down a chain if the base names a base of its own. A base can be any
built-in or any theme in your `themes/` directory, and the nearer file always
wins. One thing a base cannot do is name the built-in it shadows: a
`themes/handbook.toml` containing `base = "handbook"` means itself, and is
reported as a loop — rename the file.

Put your own themes in `themes/` beside `theme.toml` and select them by filename
with `--theme`; `--config <file>` beats both.

#### Colours

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
| `highlight` | `blue` | external links, headings 3–6 |
| `notice` | `magenta` | wikilinks and local links, transient messages, current match |

A palette slot takes a colour, never another slot's name. `cursor` falls back to
`subtle` when no theme in the chain names one, so a theme file written while the
two were a single slot still looks the way it did.

#### Elements

Individual elements can be overridden, and here a colour may also name a palette
slot:

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
and `help_window`. Each takes `fg`, `bg` and `modifiers`, and each falls back
independently. Note that `link` is external links only — a local or wiki link
that resolves is painted with `wikilink`, and one that does not with
`link_broken`.

The modifiers are `bold`, `italic`, `underline`, `dim`, `reversed` and
`crossed_out`. A `modifiers` list **replaces** whatever the element inherited
rather than adding to it, so `modifiers = []` is how you take the bold off a
heading.

#### A whole theme

```toml
base = "nord"
name = "nord-quiet"
syntax_theme = "base16-ocean.dark"

[palette]
accent    = "#8fbcbb"
highlight = "#81a1c1"
notice    = "#b48ead"

[elements.heading1]
modifiers = ["bold", "underline"]

[elements.quote]
fg = "muted_text"
modifiers = ["italic"]
```

Save that as `~/.config/vademecum/themes/nord-quiet.toml` and it is
`--theme nord-quiet`.

#### When you get it wrong

An unknown key or an unknown element name is a warning on stderr — the theme
still loads without it:

```
vademecum: /home/you/.config/vademecum/theme.toml: unknown key `pallete`
```

A colour or modifier that cannot be read is fatal, and names the key that holds
it. A base that loops prints the chain it found. Warnings always name the file
that contains the mistake rather than the one you asked for, which is what makes
a chain of bases debuggable.

### Code colours

Fenced code is highlighted with [syntect](https://github.com/trishume/syntect).
Pick the token colours with `syntax_theme`, naming one of the bundled
`.tmTheme` files or one of your own in `~/.config/vademecum/syntax-themes/`:

```toml
syntax_theme = "base16-ocean.dark"
```

`vademecum --list-syntax-themes` prints the names it will accept. A name it does
not recognise falls back to `base16-ocean.dark` and says so once.

Sublime Text's default languages are bundled, plus Swift, TypeScript, Kotlin and
TOML. Origins and licences for the added ones are in
[`syntaxes/LICENSES.md`](syntaxes/LICENSES.md).

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
