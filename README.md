# vademecum

> "Go with me." — a modern `man` for reading Markdown.

`vademecum` is a terminal Markdown reader. It renders `.md` files with the
fidelity of a modern documentation site, but in your terminal: styled
elements, syntax-highlighted code blocks, wikilink navigation across a
collection of notes, and full TOML theming so folks ricing their terminals can
make it their own.

This README is the **source of truth** for the vademecum project. All
decisions, architecture, and milestones live here. When code and README
disagree, the README wins and the code is fixed (or the README is amended
first, deliberately).

---

## Mission

Read Markdown documents beautifully in the terminal:

- Render a `.md` file with the same fidelity a modern documentation tool gives
  you, but in your terminal.
- Navigate a collection of notes via **wikilinks** (`[[other-note]]`) and
  ordinary relative links (`[text](other.md)`).
- Be fully themeable via TOML, so it fits any terminal setup (dark, light,
  custom).
- Be fast, safe, and cross-platform: a single Rust binary that compiles for
  Linux, macOS, and Windows.

## Name

**vademecum** comes from Latin *vade mecum* — "go with me" (`vade` is the
imperative of `vadere`, "to go"). A *vade mecum* is a handbook or manual you
always carry at hand. Perfect for a personal documentation reader that follows
you from shell to shell.

---

## Features

1. **Interactive TUI pager** — full-screen view (alternate buffer) with a
   cursor line, scroll, search, and navigation history, driven by the same
   render layer used for stdout output.
2. **Stdout mode** — the same styled output written to stdout with no raw
   mode. Selected automatically when stdout is not a TTY (like `bat`), or
   explicitly with `--plain`. Pipeable to `less -R`; also the basis of the
   test suite.
3. **Wikilinks and local links** — `[[target]]`, `[[target|alias]]`,
   `[[target#Heading]]`, and relative `[text](other.md#heading)` links are all
   followable. Focus a link with `Tab`, follow with `Enter`, go back/forward
   with `h`/`l`.
4. **Markdown element highlighting** — headings 1–6, paragraphs, emphasis,
   strong, strikethrough, inline code, block quotes, bullet/ordered/nested
   lists, task lists, tables, links, wikilinks, images, footnotes, horizontal
   rules, and raw HTML are each styled and distinct.
5. **Syntax highlighting for code blocks** — fenced code blocks are
   highlighted with `syntect` using its pure-Rust `fancy-regex` engine (no
   Oniguruma/C dependency). Token colors come from a syntect `.tmTheme`
   selected per vademecum theme.
6. **TOML theming** — a `[palette]` of semantic colors drives every element,
   with optional per-element overrides. Colors accept palette names, named
   ANSI colors, 8-bit indices, or hex RGB. Four built-in themes ship in the
   binary; users can add their own theme files and `.tmTheme` files.
7. **UI chrome** — header (title + right-aligned shortcut hints, hairline rule
   below) and statusbar (error > notice > `file · line X/Y · N%`, hairline rule
   above), mirroring the TUI conventions of
   [Holodeck](https://github.com/otaviocc/Holodeck). `?` opens a help overlay
   listing every keybinding.
8. **Search** — `/` searches the rendered text (smart-case), highlights every
   match, `n`/`N` jump between them.
9. **URLs** — `o` opens the focused external link in the system browser. In
   stdout mode, external links are emitted as OSC-8 hyperlinks so supporting
   terminals (Kitty, iTerm2, WezTerm, Ghostty, tmux 3.4+) make them clickable.
10. **Stdin** — `vademecum -` reads Markdown from stdin and still opens the
    TUI (pairs well with `git show HEAD:README.md | vademecum -`).
11. **Frontmatter** — leading YAML (`---`) or TOML (`+++`) frontmatter is
    hidden from the rendered document; a `title:` key becomes the header title.
12. **Live reload** — `--watch` re-renders the document when it changes on
    disk, keeping the reader on the line they were reading. It watches the
    file's *directory* rather than the file, because editors save by rename.
13. **Mouse (opt-in)** — `--mouse` enables wheel scrolling. Off by default
    because mouse capture disables the terminal's native text selection.

## Usage

```
vademecum README.md                   # interactive TUI
vademecum --plain README.md           # force stdout mode even on a TTY
vademecum README.md | less -R         # stdout mode is automatic when piped
vademecum --color always README.md > out.ansi
vademecum --width 80 README.md        # wrap width (default: min(terminal - 2, 100))
vademecum --theme catppuccin-mocha README.md
vademecum --config ~/my-theme.toml README.md
vademecum --root ~/notes README.md    # vault root for wikilink lookup
vademecum --watch README.md
vademecum --mouse README.md
cat notes.md | vademecum -            # read from stdin
vademecum --list-themes               # built-in + user themes
vademecum --list-syntax-themes        # syntect themes (built-in + user)
```

### CLI flags

| Flag | Meaning |
| --- | --- |
| `<path>` or `-` | Markdown file to open, or stdin |
| `--plain` | Stdout mode even when stdout is a TTY |
| `--color <auto\|always\|never>` | ANSI colors in stdout mode. `auto` (default) colors only when stdout is a TTY; `NO_COLOR` forces `never` |
| `--width <n>` | Wrap width. Default `min(terminal columns − 2, 100)`; `100` when not a TTY |
| `--theme <name>` | Built-in or user theme by name |
| `--config <file>` | Explicit theme file (overrides `--theme`) |
| `--root <dir>` | Vault root for wikilink resolution |
| `--watch` | Re-render on file change. Needs a file: refused with stdin, ignored in stdout mode |
| `--mouse` | Enable mouse capture (wheel scroll) |
| `--list-themes`, `--list-syntax-themes` | Print available names and exit |
| `--resolve-links` | Debug: print each link and its resolved path, then exit (stdout mode) |

## Configuration

Config directory (resolved explicitly, not via the OS "application support"
folder, so it is the same on macOS and Linux):

| Platform | Path |
| --- | --- |
| Linux, macOS | `$XDG_CONFIG_HOME/vademecum/`, defaulting to `~/.config/vademecum/` |
| Windows | `%APPDATA%\vademecum\` |

| Path | Purpose |
| --- | --- |
| `<config>/theme.toml` | The theme used when neither `--theme` nor `--config` is given |
| `<config>/themes/*.toml` | User themes, selectable by file stem with `--theme <name>` |
| `<config>/syntax-themes/*.tmTheme` | User syntect themes, referenced by name in `syntax_theme` |

**Theme precedence:** `--config <file>` > `--theme <name>` >
`<config>/theme.toml` > built-in `ansi`.

Theme files are **partial**: any palette slot or element you omit falls back
to the built-in default, and within `[elements.x]` the three keys fall back
independently — set only `fg` and the default `bg` and modifiers survive.
`modifiers = []` is how you clear them.

Unknown keys produce a warning on stderr, never an error. A value that cannot
be read — malformed TOML, a color that is none of the accepted forms — is a
different matter: it makes the theme unusable, so vademecum reports it on
stderr and exits non-zero rather than rendering something the file did not
ask for. So does an unknown `--theme <name>`, which lists the names it does
know.

A user theme whose file stem matches a built-in **shadows** it, so
`<config>/themes/kanagawa-dragon.toml` retunes that theme rather than
forcing a new name for it.

`--list-themes` prints one name per line: the built-ins in the order of the
Built-in themes table, then user themes alphabetically, each name once.

### Built-in themes

| Name | Appearance | Source |
| --- | --- | --- |
| `ansi` (default) | inherits terminal | 16 ANSI color names and `reset` only; follows whatever scheme the terminal uses |
| `kanagawa-dragon` | dark | [kanagawa.nvim](https://github.com/rebelot/kanagawa.nvim) Dragon variant |
| `catppuccin-mocha` | dark | Catppuccin canonical palette |
| `catppuccin-latte` | light | Catppuccin canonical palette |

`ansi` is the default because a reader who has not asked for a theme has
already chosen one — their terminal's. It asserts no color of its own beyond
the 16 the terminal defines, so vademecum looks like everything else on the
screen until told otherwise.

That is also why `ansi` gives code no background. The 16 ANSI colors have no
dark grey in them: the nearest, `dark_gray`, is *bright black*, which most
terminal schemes render as a mid grey. As the background of inline code and of
whole code blocks that reads as a washed-out slab, and it collided with the
cursor line, which used the same slot — so the bar vanished exactly where the
README promises it stays visible. `ansi` therefore styles code by foreground
alone and keeps `subtle` for the cursor line, which is the one place a filled
band is wanted and the one place bright black is right. The three themed
built-ins set `subtle` to a real near-background tint and keep their code
backgrounds.

### Theme format

```toml
# ~/.config/vademecum/theme.toml
name = "my-theme"
# syntect .tmTheme by name: bundled (base16-ocean.dark, base16-eighties.dark,
# base16-mocha.dark, base16-ocean.light, InspiredGitHub, Solarized (dark),
# Solarized (light)) or a file in <config>/syntax-themes/.
syntax_theme = "base16-ocean.dark"

# Semantic palette. Every element style derives from these slots. A slot takes
# an 8-bit index (208), hex ("#89b4fa"), "reset" for the terminal's own color,
# or one of the 16 ANSI names: black, red, green, yellow, blue, magenta, cyan,
# gray, dark_gray, light_red, light_green, light_yellow, light_blue,
# light_magenta, light_cyan, white. A slot cannot name another slot.
[palette]
background           = "#1E1E1E"
foreground           = "#FFFFFF"
muted                = "#4D4D4D"   # rules, borders, table lines
muted_text           = "#8E8E8E"   # hints, footers, html passthrough
subtle               = "#2A2A2A"   # cursor line, code block background
selection_background = "#54554A"
selection_foreground = "#FFFFFF"
error                = "#FC4651"
success              = "#2EA85B"
warning              = "#FFE76D"
accent               = "#56D0B3"   # headings, popup borders
chrome               = "#56D0B3"   # header title
highlight            = "#35B0D8"   # links
notice               = "#F2248C"   # wikilinks, transient status

# Optional per-element overrides. `fg`/`bg` accept everything a palette slot
# does, plus the name of a slot ("accent") — resolved against this file's
# palette. Modifiers: bold, italic, underline, dim, reversed, crossed_out.
[elements.heading1]
fg = "accent"
modifiers = ["bold", "underline"]

[elements.inline_code]
fg = "warning"
bg = "none"      # no background at all — see below
```

`bg = "none"` is the one value that is not a color: it removes an element's
background rather than changing it, leaving whatever is underneath. It is
accepted for `bg` alone — a foreground is what a glyph is drawn in and cannot be
absent — and it is what `reset` is not: `reset` paints the terminal's own
background *over* what is beneath, while `none` paints nothing.

Every slot a file leaves out keeps its default, which is the `ansi` built-in:

| Slot | Default | Slot | Default |
| --- | --- | --- | --- |
| `background` | `reset` | `success` | `green` |
| `foreground` | `reset` | `warning` | `yellow` |
| `muted` | `dark_gray` | `accent` | `cyan` |
| `muted_text` | `gray` | `chrome` | `cyan` |
| `subtle` | `dark_gray` | `highlight` | `blue` |
| `selection_background` | `blue` | `notice` | `magenta` |
| `selection_foreground` | `white` | | |
| `error` | `red` | | |

`subtle` is the one slot `ansi` cannot leave as `reset`: it is the background of
the cursor line, and a background equal to the terminal's own would make the bar
invisible. Under `ansi` it is the cursor line's alone — see the note on that
theme above.

Every element and its default derivation from the palette:

| Element | Default fg | Default bg | Modifiers | Notes |
| --- | --- | --- | --- | --- |
| `paragraph` | foreground | — | — | |
| `heading1` | accent | — | bold | rendered with `# ` prefix hidden, blank line after |
| `heading2` | accent | — | bold | |
| `heading3` | highlight | — | bold | |
| `heading4` … `heading6` | highlight | — | — | |
| `emphasis` | — | — | italic | |
| `strong` | — | — | bold | |
| `strikethrough` | muted_text | — | crossed_out | |
| `inline_code` | warning | subtle | — | |
| `code_block` | foreground | subtle | — | bg applies to the whole block; syntect fg colors layered on top |
| `code_block_lang` | muted_text | subtle | — | language tag shown on the fence line |
| `quote` | muted_text | — | italic | `┃ ` gutter in `muted` |
| `list_bullet` | accent | — | — | `•`, `◦`, `▪` by nesting depth |
| `list_number` | accent | — | — | honors ordered-list start numbers |
| `task_done` | success | — | — | `☑` |
| `task_todo` | muted_text | — | — | `☐` |
| `link` | highlight | — | underline | external `[text](url)` |
| `wikilink` | notice | — | bold | `[[target]]` and local `[text](file.md)` |
| `link_focused` | selection_foreground | selection_background | bold | the link `Enter` would follow |
| `link_broken` | error | — | crossed_out | local target does not exist |
| `image` | muted_text | — | italic | rendered as `[image: alt]` |
| `footnote` | muted_text | — | — | `[^1]` markers and definitions |
| `html` | muted_text | — | dim | raw HTML shown verbatim |
| `table_header` | foreground | — | bold | |
| `table_border` | muted | — | — | box-drawing characters |
| `hr` | muted | — | — | full-width `─` |
| `header_title` | chrome | — | bold | |
| `hint` | muted_text | — | — | shortcut hints, rules |
| `status` | foreground | — | — | `file · line X/Y · N%` |
| `status_notice` | notice | — | — | transient messages ("Opened x.md") |
| `status_error` | error | — | bold | |
| `cursor_line` | — | subtle | — | applied over the whole line |
| `search_match` | background | warning | — | |
| `search_current` | background | notice | bold | |
| `help_window` | foreground | background | — | border in accent |

The `ansi` built-in maps every slot to a `Color::Reset` or a 16-color ANSI
name so it never asserts truecolor. Being the default, it is also the base
every partial theme file merges over.

The `syntax_theme` key cleanly separates **element styling** (our TOML) from
**code token coloring** (syntect). Only the syntect theme's *foreground*
colors and font styles are used; its background is ignored in favor of
`code_block.bg` so code blocks always match the surrounding theme.

## Dependencies

Versions are the current generation as of the start of the project; bump
deliberately.

| Crate | Version | Role |
| --- | --- | --- |
| `pulldown-cmark` | 0.13 | CommonMark + GFM parser; `ENABLE_TABLES`, `ENABLE_STRIKETHROUGH`, `ENABLE_TASKLISTS`, `ENABLE_FOOTNOTES`, `ENABLE_WIKILINKS` |
| `ratatui` | 0.30 (`features = ["crossterm"]`) | TUI framework |
| `crossterm` | 0.29 | Terminal backend (raw mode, alternate screen, events) |
| `syntect` | 5.3 (`default-features = false, features = ["default-fancy"]`) | Code highlighting, pure Rust |
| `serde` | 1 (`features = ["derive"]`) | Theme deserialization |
| `toml` | 1 | Theme file parsing |
| `clap` | 4 (`features = ["derive"]`) | CLI |
| `notify` + `notify-debouncer-mini` | 8 / 0.6 | `--watch` |
| `unicode-width` | 0.2 | CJK/emoji-aware wrapping |
| `open` | 5 | `o` opens URLs in the browser |
| `anyhow`, `thiserror` | 1 / 2 | Error handling (binary / library-style modules) |
| dev: `assert_cmd`, `predicates`, `insta`, `tempfile` | — | Integration + snapshot tests |

Not used: `dirs` (its macOS config dir contradicts the documented
`~/.config` path; resolution is done by hand in `config.rs`).

## Architecture

Single binary crate with clear modules — easy to split into a Cargo workspace
later if needed.

```
vademecum/
├── Cargo.toml
├── README.md                # this file — source of truth
├── LICENSE                  # MIT
├── rustfmt.toml
├── Makefile                 # build/run/test/clean/fmt/lint
├── themes/                  # built-in themes, embedded with include_str!
│   ├── ansi.toml
│   ├── kanagawa-dragon.toml
│   ├── catppuccin-mocha.toml
│   └── catppuccin-latte.toml
├── .cargo/
│   └── audit.toml           # cargo-audit policy: deny warnings, accepted advisories
├── .github/workflows/
│   ├── ci.yml               # fmt, clippy, tests (linux/mac/win), MSRV, audit
│   └── release.yml          # tag → test, build binaries, publish, GitHub Release
├── src/
│   ├── main.rs              # entry; panic hook restores terminal; picks
│   │                        #   TUI vs stdout mode (isatty + --plain)
│   ├── cli.rs               # clap definitions (flags table above)
│   ├── config.rs            # config dir resolution (XDG / %APPDATA%)
│   ├── document.rs          # Document { path, base_dir, source, title };
│   │                        #   load from file or stdin; frontmatter split
│   ├── theme/
│   │   ├── mod.rs           # Theme = Palette + resolved element Styles
│   │   ├── palette.rs       # Palette slots + serde
│   │   ├── elements.rs      # ElementStyle overrides + defaults table
│   │   ├── color.rs         # "accent" | "blue" | 208 | "#rrggbb" → Color
│   │   └── loader.rs        # precedence, merging, built-ins, --list-themes
│   ├── markdown/
│   │   ├── mod.rs
│   │   ├── ast.rs           # pulldown events → Block / Inline tree
│   │   └── links.rs         # classify + resolve link destinations
│   ├── render/
│   │   ├── mod.rs           # AST → Vec<RenderedLine> at a given width
│   │   ├── line.rs          # RenderedLine / StyledSpan / LinkRef model
│   │   ├── layout.rs        # word wrap, indentation, tables, unicode width
│   │   ├── code.rs          # syntect: lazy sets, per-block highlight cache
│   │   └── ansi.rs          # RenderedLine → SGR + OSC-8 text for stdout
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── app.rs           # AppState (mode, scroll, cursor, focus, history)
│   │   ├── input.rs         # key/mouse → Action (reducer style, like Holodeck)
│   │   ├── view.rs          # header, rules, content, statusbar, help popup
│   │   └── search.rs        # match computation over RenderedLines
│   └── watch.rs             # debounced directory watcher, re-armed on navigation
└── tests/
    ├── stdout.rs            # assert_cmd + insta snapshots (--color always --width 80)
    ├── links.rs             # vault fixture navigation via --plain --resolve-links
    ├── common/mod.rs        # the hermetic `vademecum()` command both tests use
    └── fixtures/
        ├── elements.md      # every Markdown construct
        ├── frontmatter.md
        ├── theme.toml       # partial override file
        ├── note.md          # elements.md's wikilink target
        ├── other.md         # its local link target
        ├── nested/note.md   # its nested target, for the fragment link
        └── vault/           # .obsidian/, index.md, nested/note.md, [[note]] links
```

### Core data model

```rust
// markdown/ast.rs
enum Block { Heading{level, inlines, anchor}, Paragraph(Vec<Inline>), Quote(Vec<SourceBlock>),
             List{ordered: Option<u64>, items: Vec<ListItem>}, CodeBlock{lang, text},
             Table{header, rows, alignments}, Rule, Html(String),
             FootnoteDef{label, blocks: Vec<SourceBlock>} }
enum Inline { Text(String), Emphasis(..), Strong(..), Strike(..), Code(String),
              Link{kind: LinkKind, inlines}, Image{alt, url}, Html(String),
              FootnoteRef(String), SoftBreak, HardBreak, TaskMarker(bool) }
enum LinkKind { External(Url), Local{path: PathBuf, fragment: Option<String>},
                Wiki{target: String, fragment: Option<String>} }
struct SourceBlock { line: usize, block: Block }
struct ListItem   { blocks: Vec<SourceBlock> }

// render/line.rs — the contract shared by the TUI and the stdout writer
struct StyledSpan { text: String, style: Style }
struct LinkRef    { span_range: Range<usize>, kind: LinkKind, resolved: Option<PathBuf> }
struct RenderedLine { spans: Vec<StyledSpan>, links: Vec<LinkRef>,
                      anchor: Option<String>, source_line: usize }
```

`Url` is a newtype over `String`: destinations are kept verbatim and never
re-serialized, so there is no URL-parsing dependency.

`parse()` returns `Vec<SourceBlock>`, pairing every block — at any depth, so
a quote and a list item locate as precisely as a paragraph — with the 1-based
line it starts on (from `Parser::into_offset_iter`). That line travels into
every `RenderedLine` the block produces, which is what lets a resize re-layout
and still put the cursor back on the same source line.

`Inline::TaskMarker` stays where the parser puts it, first in a list item's
opening paragraph; the renderer lifts it into the bullet column. Raw HTML has
both a block and an inline form, and both are styled with `html`.

Everything after parsing works on `Vec<RenderedLine>`: the TUI paints spans
into the ratatui buffer, the stdout writer serializes them to ANSI, search
scans their text, and cursor/Tab/Enter/`o` read `links`.

### Data flow

1. **CLI** (`cli.rs`) parses args → source (path or stdin), mode (TUI if
   stdout is a TTY and `--plain` absent), color policy, width, theme
   selection, vault root, watch, mouse.
2. **Load** (`document.rs`) reads the file (or stdin to EOF). Stdin still
   supports the TUI because crossterm reads events from `/dev/tty` (Unix) or
   `CONIN$` (Windows) when stdin is not a terminal. Frontmatter is split off
   when the source starts with `---` (YAML, closed by `---` or `...`) or `+++`
   (TOML); its `title` feeds the header. `base_dir` is the file's directory,
   or the CWD for stdin.
3. **Parse** (`markdown/ast.rs`) runs `pulldown-cmark` with the GFM options
   plus `ENABLE_WIKILINKS`, producing the `Block`/`Inline` tree. Heading
   anchors are GitHub-style slugs. `links.rs` classifies each destination as
   External, Local, or Wiki.
4. **Render** (`render/`) lays the tree out at the target width into
   `Vec<RenderedLine>`. The AST is cached per document; a terminal resize
   only re-runs this step.
5. **Output** — either `render/ansi.rs` writes the lines to stdout, or
   `ui/` runs the ratatui event loop.
6. **Navigate** — following a link pushes `(Document, scroll, cursor)` onto
   the history, loads and renders the target, and re-arms the watcher.
7. **Wake** — the pager waits on one channel. A thread forwards terminal
   events onto it and, with `--watch`, the watcher posts changes onto the same
   channel, so the loop blocks on a single `recv` and there is no timer
   anywhere — an idle reader costs nothing whether or not they are watching.

### Layout rules

- **Wrap width**: `--width`, else `min(terminal columns − 2, 100)`; `100`
  when stdout is not a TTY. Content is left-aligned with a one-column gutter,
  which is taken *out* of the wrap width, so `--width 80` means 80 columns.
- **Word wrap** on whitespace using `unicode-width` for display width; a word
  longer than the width is broken hard.
- **Code blocks** never wrap: lines longer than the width are truncated with
  `…`. A blank fence line shows the language tag right-aligned, a second one
  closes the block, and every line is padded to the full width so the
  background is an unbroken rectangle — invisible, and harmless, under a theme
  that gives code no background.
- **Tables**: column width = max cell width; if the sum exceeds the wrap
  width, shrink columns proportionally (min 3) and wrap cells. Box-drawing
  borders in `table_border`, a rule under the header and none between body
  rows, and the column alignments from the source applied to the padding. The
  minimum of 3 is a goal, not a promise: at a narrow enough width, holding it
  would push the table past the wrap width, so it gives way first.
- **Lists** indent 2 columns per level; ordered lists use the source start
  number; task markers replace the bullet. Bullets cycle `•`, `◦`, `▪` and
  begin again at the fourth level. A nested list follows its item's text with
  no blank line between them — the separator would split the list in two.
- **Quotes** get a `┃ ` gutter per nesting level and wrap inside it.
- **Footnote definitions** hang under a `[^1] ` marker in `footnote`.
- **No line is ever wider than the wrap width.** Wrapping keeps text inside it,
  but a table's borders and padding have a floor a narrow width cannot pay for,
  and a single character can be wider than the whole line. Whatever is left over
  is cut with `…`, the same mark a long code line gets.
- **Body text inherits a base style**: `paragraph` at document level, `quote`
  inside a quote. Inline styles patch on top of it, so a link inside a quote
  is a link and the text around it is quoted.
- Headings are followed by one blank line; blocks are separated by one
  blank line; the document never ends with trailing blank lines.

### Link model

**Classification** (`markdown/links.rs`):

| Source | Kind |
| --- | --- |
| `[t](https://…)`, `<https://…>`, `mailto:` | External |
| `[t](other.md)`, `[t](../x/y.md#sec)`, `[t](#sec)` | Local (path relative to `base_dir`; empty path = current file) |
| `[[target]]`, `[[target\|alias]]`, `[[target#Heading]]`, `[[#Heading]]` | Wiki |

**Wikilink resolution** (in order, first hit wins):

1. `base_dir/target` and `base_dir/target.md`.
2. A **unique** file named `target.md` (case-insensitive) anywhere under the
   vault root, skipping hidden directories. Ambiguity is reported as an
   error in the statusbar listing the candidates.

The vault root is `--root` if given, else the nearest ancestor of the start
file containing `.obsidian/`, else the start file's directory. A `--root` that
is not a readable directory is a mistake in the command, not in the document:
vademecum reports it on stderr and exits non-zero before rendering anything,
as it does for an unknown `--theme`. Silence there would show a document of
broken links and no reason for them. Everything *below* the root stays silent —
one unreadable subdirectory does not stop a vault resolving — and a discovered
root needs no check, having been found by looking. A target that
cannot be followed — missing, or ambiguous — renders in `link_broken` style,
and `Enter` shows the reason instead of navigating.

Resolution runs during layout, alongside the styling it decides, so a link's
element depends on whether its target exists: `link` for External, `wikilink`
for a Local or Wiki target that resolves, `link_broken` for one that does not.

**Fragments** (`#heading`) jump to the first heading whose slug matches after
the target loads, and put it at the top of the view; a fragment alone stays in
the current document — `[[#Heading]]` and `[text](#heading)` alike — and going
back returns the reader to where they jumped from. One that matches no heading leaves the document at its top rather than
reporting anything.

`--resolve-links` prints what resolution made of every link in the document
and exits, which is how the vault fixture is tested. One line per link in
source order: kind, destination, and target, in three columns.

```
external  https://example.com/docs   -> -
local     other.md                   -> tests/fixtures/other.md
local     nested/note.md#a-heading   -> tests/fixtures/nested/note.md
wiki      note                       -> tests/fixtures/vault/note.md
wiki      missing                    -> (broken)
wiki      duplicate                  -> (ambiguous: a/dup.md, b/dup.md)
```

An External link has no file to name, so its target column is `-`. The
destination is the link as written, fragment included; the target is the path
as resolved, relative to wherever vademecum was run from.

**Focus and follow** in the TUI: the cursor line is the reader position. If
it contains one link, that link is focused; if several, `Tab`/`Shift-Tab`
cycle focus among them. Moving the cursor drops the focus back to the first
link: one on a line the reader has left is not the link they meant.

`Enter` follows a Local or Wiki link (push history, open, render, statusbar
notice "Opened x.md"), and leaves an External one to `o`, which opens it with
the `open` crate and in turn leaves a Local or Wiki link alone (opening files
in `$EDITOR` is out of scope). Neither says anything about the link it is not
for. A document opens at its top, unless the link carried a fragment.

`h`/`Backspace` go back, `l` goes forward. A history entry is the document,
the scroll, the cursor **and** the focus, so coming back puts the reader on
the link they left from. Going somewhere new drops the way forward. The vault
root is discovered once, from the document vademecum was started on: following
a link stays inside the collection the reader started in.

A standing search query follows the reader across: it is matched again against
each document as it opens, so `n` keeps working without being retyped.

### Syntax highlighting (`render/code.rs`)

- `SyntaxSet::load_defaults_newlines()` and `ThemeSet::load_defaults()`, both
  created lazily once (`OnceLock`), plus user `.tmTheme` files from
  `<config>/syntax-themes/` via `ThemeSet::add_from_folder`.
- Language lookup by fence tag (`find_syntax_by_token`), then by first-line
  shebang, else plain text. The bundled set is syntect's own: Rust, C, C++,
  Objective-C, Java, Scala, Go, Python, Ruby, Perl, PHP, JavaScript, HTML, CSS,
  shell, Makefile, SQL, XML, YAML, JSON, LaTeX and the rest of the Sublime
  Text defaults. Swift, TypeScript, Kotlin and TOML are *not* in it; a fence
  tagged with one of those renders as plain code until their syntaxes are
  bundled.
- Each block is highlighted once with `HighlightLines` and cached by
  `(syntax, syntax theme, text hash)` — the three things its colors depend on;
  the `fancy-regex` engine is slower than Oniguruma, so highlighting is never
  redone on scroll or resize.
- syntect `Style` → our `Style`: foreground color and bold/italic/underline
  only; background comes from `code_block.bg`.
- A `syntax_theme` naming a theme that is neither bundled nor present in
  `<config>/syntax-themes/` is a warning on stderr, printed once, not an error:
  the document still renders, highlighted with `base16-ocean.dark`.
- `--list-syntax-themes` prints bundled and user theme names.

### UI layout & chrome

The TUI follows the conventions used by Holodeck: a slim header, a content
area, and a statusbar, separated by hairline `─` rules.

```
 vademecum · README.md          ? help  / search  ⇥ link  ⏎ follow  h/l back/fwd  q quit
 ──────────────────────────────────────────────────────────────────────────────────────
 <rendered markdown, cursor line highlighted>
 ──────────────────────────────────────────────────────────────────────────────────────
 README.md · line 42/310 · 13%
```

- **Header** — ` vademecum · <title> ` in `header_title` (title from
  frontmatter, else the filename, else `stdin` when the document came from
  there), shortcut hints right-aligned in `hint`, hairline rule below. The
  hints list the bindings the build actually has, so they grow as milestones
  land; when the title and the hints would collide, the hints give way.
- **Statusbar** — priority: the `/` prompt in Search mode (the reader has to
  see what they type) > `status_error` (e.g. "note.md: not found") >
  `status_notice` (transient, cleared on next key) > `status`
  (`file · line X/Y · N%`, where `file` is the filename or `stdin`, Y is the
  rendered line count, and N is `cursor · 100 / (Y − 1)` — the cursor line's
  position, so the first line reads 0% and the last 100%). It reports the
  cursor even when the wheel has scrolled it off-screen: the cursor is the
  reading position, and where the reader is reading is what the statusbar is
  for. Whenever a search
  query is active, ` · match i/n` is appended.
- **Help overlay** — `?` opens a centered popup (60% width, height clamped
  to 40–90% of the terminal) listing every keybinding; `?`, `Esc`, or `q`
  close it. It does not scroll: a terminal too short for the whole table
  clips it. While it is open the document does not move — motion keys and the
  wheel are swallowed, so closing the overlay returns the reader exactly where
  they were.
- **Cursor line** — highlighted with `cursor_line` across the full terminal
  width, and over whatever background the line's own spans carry, so the
  cursor stays visible inside a code block. It moves with `j`/`k`, scrolling
  the viewport when it hits the edge. A page is the viewport height less one
  line of context and a half page is half the viewport height; both move
  cursor and viewport together. A motion key pressed while the wheel has left
  the cursor off-screen moves it to the nearest visible line **first**, and
  then moves: a reader who scrolled to a new section and pressed `j` meant to
  carry on reading there, not to be yanked back. Only a motion key does this —
  scrolling away and back leaves the cursor untouched.
- **Search** — `/` collects a query and matching runs on `Enter`, not on every
  keystroke, so typing stays responsive in a long document. The cursor jumps
  to the first match at or after it, wrapping; every match is highlighted. A
  query that matches nothing leaves the cursor where it is and says so in the
  statusbar. Confirming an *empty* query changes nothing, so a standing query
  and its highlights survive a stray `/`. `Esc` while typing abandons the text
  and leaves the previous
  query and its highlights standing; `Esc` in Browse mode clears the query,
  the matches and the highlight. Neither moves the cursor.
- **Mouse** — with `--mouse`, a wheel notch scrolls the viewport three lines
  and moves nothing else. The cursor keeps its document line and may scroll off
  the screen; scrolling back reveals it exactly where it was. The wheel is
  browsing, not reading: it never costs the reader the place they were at, nor
  the link focused there.
- **Resize** — re-layout from the cached AST, keep the cursor on the same
  source line, recompute search matches. Only a change of *width* re-lays the
  document out; a taller or shorter terminal costs nothing.
- **Live reload** — with `--watch`, a write to the open document re-reads it,
  re-parses it and lays it out again, putting the reader back exactly as a
  resize does: on the same source line, at the same height on the screen, with
  a standing search matched against the new text. It says `Reloaded <file>` as
  a notice — unless the statusbar is already carrying an error, which is the
  answer to something the reader asked for and did not get: someone else saving
  the file is no reason to take that off the screen before they have acted on
  it. What is watched is the file's **directory**, not the file: editors
  save by renaming a new file into place, so a watch on the file itself goes
  deaf after the first save, and a document that is deleted and restored comes
  back on its own. Writes are debounced, so one save is one reload. A file that
  cannot be read — deleted, or unreadable — is a statusbar error and nothing
  more: the document stays on screen and the next write reloads it. The help
  overlay does not suppress a reload; it swallows the reader's keys, not the
  world's changes. `--watch -` names no file to re-read, and a pipe offers no
  second chance to be read at all, so it is refused — `--watch: cannot watch
  stdin; pass a file path instead` on stderr, and a non-zero exit **before**
  stdin is drained, which is what keeps `vademecum --watch -` from hanging at a
  terminal waiting for input it would never use.
- **Panic hook** — leaves raw mode and the alternate screen before printing
  the panic, so a bug never leaves the terminal broken.
- **A terminal too short** for the chrome draws whatever rows it has and no
  more. The pager is readable at four rows and does not fail below that.
- **Errors** during navigation or reload never exit the program; they land
  in the statusbar. Errors before the TUI starts (missing file, bad theme)
  print to stderr and exit non-zero.

### TUI keybindings

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | Move cursor line down / up |
| `d` / `u`, `Ctrl-D` / `Ctrl-U` | Half page down / up |
| `Space` / `b`, `PgDn` / `PgUp` | Page down / up |
| `g` / `G`, `Home` / `End` | Top / bottom |
| `Tab` / `Shift-Tab` | Cycle link focus on the cursor line |
| `Enter` | Follow focused local/wiki link |
| `o` | Open focused external link in the browser |
| `h` / `Backspace`, `l` | History back, forward |
| `/` | Search (type, `Enter` to confirm, `Esc` to cancel) |
| `n` / `N` | Next / previous match |
| `?` | Help overlay |
| `Esc` | Close overlay; abandon a search being typed; clear the search highlight |
| `q`, `Ctrl-C` | Quit (restore terminal) |
| Mouse wheel | Scroll the viewport three lines a notch, cursor unmoved (only with `--mouse`) |

### Stdout mode (`render/ansi.rs`)

- Selected when stdout is not a TTY or `--plain` is given.
- `--color auto|always|never`; a non-empty `NO_COLOR` → `never`, whatever
  `--color` says. With `never` the output is the wrapped text with no escape
  codes at all, and trailing padding is trimmed so piping into `grep` and
  `diff` stays clean.
- External links are wrapped in OSC-8 (`ESC ] 8 ; ; url ESC \ … ESC ] 8 ; ; ESC \`)
  when color is on. ratatui cannot emit OSC-8, so this is stdout-only.
- Styles are emitted as SGR sequences with a reset at the end of every line
  so `less -R` and `grep` behave.

## Testing

- **Unit tests** (in-module)
  - `theme/color.rs` — palette names, ANSI names, indices, hex, invalid input.
  - `theme/loader.rs` — precedence chain, partial-file merging, unknown-key
    warning, every built-in parses and resolves every element.
  - `config.rs` — XDG / `%APPDATA%` resolution with env overrides.
  - `document.rs` — frontmatter split (YAML, TOML, `...` close, none, `---`
    not at start), title extraction, stdin.
  - `markdown/ast.rs` — one test per construct in the element table.
  - `markdown/links.rs` — classification table; wiki resolution (relative,
    `.md` appended, vault search, ambiguity, missing, case-insensitive, hidden
    dirs skipped); fragment slugs.
  - `render/layout.rs` — wrap at width, CJK width, long-word break, table
    shrink/wrap, nested list indent, code truncation.
  - `render/code.rs` — a Rust fence yields multiple distinct foreground
    colors; unknown language yields one style; cache hit on repeated block.
  - `ui/search.rs` — smart-case, match positions, wrap-around for `n`/`N`.
  - `ui/input.rs` — key → action table, mode transitions (Browse/Search/Help).
  - `ui/app.rs` — reload keeps the source line and the screen row; a file that
    shrank leaves the cursor on a line that exists; an unreadable one is a
    statusbar error that leaves the document on screen.
  - `watch.rs` — the armed directory and name of a path (a symlink arms on its
    target's directory); which event paths name the open document; a watcher
    error counts as a change. One test writes to a real file and waits for the
    watch to see it.
- **Integration tests** (`tests/`, via `assert_cmd`)
  - `vademecum --plain --color always --width 80 fixtures/elements.md`
    snapshot with `insta` (one snapshot per built-in theme).
  - `--color never` snapshot contains no `ESC`.
  - Stdin: `echo '# hi' | vademecum -` (stdout mode, since stdout is piped).
  - Frontmatter fixture: frontmatter absent from output.
  - `--config fixtures/theme.toml` changes heading color in the snapshot.
  - `--list-themes` / `--list-syntax-themes` list expected names.
  - `--watch -` exits non-zero with a clear message.
  - Vault fixture: a `--plain` debug flag `--resolve-links` prints each link
    and its resolved path, asserting relative, nested, and broken cases.
- TUI behaviour is tested at the reducer level (`ui/input.rs`, `ui/app.rs`)
  with `ratatui::backend::TestBackend` for the view; no PTY tests.

## Verification

```
make lint          # cargo clippy --all-targets -- -D warnings
make test          # cargo test
cargo fmt --all -- --check
```

Manual smoke tests: open `README.md` in the TUI, `Tab`/`Enter` through the
links in `tests/fixtures/vault/`, `h`/`l` history, `/` search, `?` help,
resize the terminal, `--plain | less -R`, `--theme` for each built-in,
`--watch` while editing the file, `cat x.md | vademecum -`, and `o` on an
external link.

## Makefile

```sh
make build    # cargo build --release
make run      # cargo run --release -- README.md
make test     # cargo test
make clean    # cargo clean
make fmt      # cargo fmt
make lint     # cargo clippy --all-targets -- -D warnings
```

## Continuous Integration

GitHub Actions on `otaviocc/vademecum`, modelled on Holodeck's workflows:

- **`.github/workflows/ci.yml`** — on `main` pushes and pull requests:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --locked` on a matrix of `ubuntu-latest`, `macos-latest`,
    `windows-latest`
  - an **MSRV** job that builds with the `rust-version` read from `Cargo.toml`
  - a **cargo-audit** job, running `cargo audit` directly rather than through a
    check-run action, so it needs no write permission on the repository and
    works the same on a fork's pull request as on `main`. It runs with
    `--deny warnings`, and the advisories the project has accepted are listed
    in `.cargo/audit.toml` with the reason for each — a file a local
    `cargo audit` reads too. An informational `unmaintained` notice for a crate
    that is not on that list therefore turns the build red rather than
    scrolling past, and accepting one is an edit with a rationale next to it
- **`.github/workflows/release.yml`** — on tags `v*.*.*`:
  1. verify the tag matches `Cargo.toml` `version`
  2. `cargo test --locked`
  3. build release archives: `x86_64-unknown-linux-gnu`,
     `aarch64-unknown-linux-gnu`, macOS universal (`aarch64` + `x86_64` via
     `lipo`), `x86_64-pc-windows-msvc`
  4. `cargo publish --locked` (idempotent: "already uploaded" is success).
     Publishing runs **after** the binaries build so a build failure never
     leaves a published crate without a release
  5. create the GitHub Release with install notes and all archives attached

## Project conventions

- `rustfmt.toml` — `edition = "2024"`, `style_edition = "2024"`,
  `use_small_heuristics = "Max"`, `max_width = 130` (identical to Holodeck).
- Rust edition 2024, `rust-version = "1.88"` (verified by the CI MSRV job).
- `[profile.release]` — `lto = "thin"`, `codegen-units = 1`.
- Errors: `anyhow` at the binary boundary, `thiserror` enums inside modules
  that the UI needs to match on (link resolution, theme loading).
- No `unsafe`. No `unwrap` outside tests; `expect` only with an invariant
  message.
- License: MIT.

## Milestones

Each milestone ends with a verifiable state and a green `make lint && make test`.

0. **Scaffold** — `Cargo.toml` (metadata, pinned deps, MSRV, release
   profile), `LICENSE`, `rustfmt.toml`, `Makefile`, `ci.yml`, `cli.rs`
   skeleton. Verifiable: `vademecum --version`; CI green on all three OSes.
1. **Stdout renderer** — `config.rs`, `document.rs` (file, stdin,
   frontmatter), `markdown/ast.rs` + `links.rs` (classification only),
   `theme/` with the built-in `ansi` palette and the element defaults
   table (no file loading yet), `render/` (`line`, `layout`, `ansi`; code
   blocks unhighlighted), `--plain`, `--color`, `--width`, automatic non-TTY
   detection. Verifiable: `vademecum --plain README.md` renders every
   construct in `fixtures/elements.md`; `insta` snapshots committed.
2. **Theme files** — `theme/loader.rs`: TOML parsing, palette + overrides,
   partial merging, precedence, `--theme`, `--config`, `--list-themes`, the
   four built-ins. Verifiable: snapshot per theme; `--config fixtures/theme.toml`.
3. **Code highlighting** — `render/code.rs`, `syntax_theme`, user
   `.tmTheme`, `--list-syntax-themes`. Verifiable: highlighted Rust fence in
   the snapshot.
4. **TUI pager** — alternate screen, header/rules/statusbar, help overlay,
   cursor line, scroll/page/jump keys, search, resize, panic hook, `--mouse`.
   Verifiable: reducer tests + `TestBackend` view tests; manual smoke.
5. **Links** — wiki resolution + vault root, local links, fragments, Tab
   focus, Enter/history, `o`, broken-link styling, `--resolve-links`.
   Verifiable: `tests/links.rs` on the vault fixture.
6. **Live reload** — `watch.rs` with debouncing, directory watching,
   re-arming on navigation, scroll preservation, stdin rejection.
7. **Release** — `release.yml`, install notes, `cargo publish --dry-run`,
   README polish, `v0.1.0` tag.

## License

MIT.
