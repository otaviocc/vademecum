# vademecum

> "Come with me." — a modern `man` for reading Markdown.

`vademecum` is a terminal-based Markdown reader. A modern version of `man`, but
capable of reading Markdown files directly from the filesystem, with wikilink
navigation, rich element + syntax highlighting, and full TOML theming so folks
ricing their terminals can make it their own.

This README is the **source of truth** for the vademecum project. All decisions,
architecture, and milestones live here.

---

## Mission

Read Markdown documents beautifully in the terminal:

- Render a `.md` file with the same fidelity a modern documenting tool gives you,
  but in your terminal.
- Navigate a collection of notes via **wikilinks** (`[[anotherfile]]`).
- Be fully themeable via TOML, so it fits any terminal setup (dark, light, custom).
- Be fast, safe, and cross-platform: a single Rust binary that compiles for
  Linux, macOS, and Windows.

## Name

**vademecum** derives from Latin *veni mecum* — "come with me" — a handbook or
manual you always carry at hand. Perfect for a personal documentation reader
that follows you from shell to shell.

---

## Features

1. **Interactive TUI pager** — full-screen view (alternate buffer) with scroll,
   search, and navigation history, driven by the same render layer used for
   plain output.
2. **Wikilinks** — `[[target]]` and `[[target|display alias]]`, resolved
   relative to the directory of the currently-open file. Follow with `Enter`;
   navigate history with `h`/`l`.
3. **Markdown element highlighting** — headings, emphasis, strong, inline code,
   blockquotes, lists, tables, links, wikilinks, and horizontal rules are all
   styled and distinct.
4. **Syntax highlighting for code blocks** — fenced code blocks are
   highlighted with `syntect`, using its pure-Rust `fancy-regex` engine (no
   Oniguruma/C dependency). The active syntax theme is selected per-theme.
5. **TOML theme support** — every element has a color + modifier in a TOML
   file. Colors accept named ANSI colors, 8-bit indices, or hex RGB
   (`#rrggbb`). Users can drop their own `.tmTheme` files into the config
   directory for the code-block token colors.
6. **`--plain` ANSI stdout mode** — renders the same styled output to stdout
   with no raw mode, for piping to `less` or other tools; also the basis for
   the test suite.

## Usage

```
vademecum README.md              # interactive TUI
vademecum --plain README.md      # ANSI stdout (pipeable)
vademecum --plain README.md | less
vademecum --config ~/my.toml README.md
vademecum --list-themes          # list built-in + user themes
vademecum --list-syntax-themes   # list syntect themes (built-in + user)
```

## Configuration

Config dir: `~/.config/vademecum/`

| Path | Purpose |
| --- | --- |
| `~/.config/vademecum/theme.toml` | Theme (element styles + `syntax_theme` selection) |
| `~/.config/vademecum/themes/*.tmTheme` | Custom syntect themes, referenced by name |
| `--config <file>` | Override the theme file location |

### Theme format

```toml
# vademecum theme — dark example (Catppuccin-style)
[document]
text = "#cdd6f4"
background = "#1e1e2e"

[heading1]
fg = "#cba6f7"
modifiers = ["bold"]

[heading2]
fg = "#89b4fa"
modifiers = ["bold"]

[heading3]
fg = "#89b4fa"

[emphasis]
modifiers = ["italic"]

[strong]
modifiers = ["bold"]

[inline_code]
fg = "#f38ba8"
bg = "#313244"

[code_block]
fg = "#a6adc8"
bg = "#181825"
syntax_theme = "base16-ocean.dark"

[quote]
fg = "#a6adc8"
prefix = "┃ "

[link]
fg = "#89b4fa"
modifiers = ["underline"]

[wikilink]
fg = "#a6e3a1"
modifiers = ["bold"]

[list_bullet]
fg = "#fab387"

[table]
header_fg = "#cdd6f4"
header_modifiers = ["bold"]
border_fg = "#6c7086"

[hr]
fg = "#6c7086"

[status_bar]
fg = "#1e1e2e"
bg = "#89b4fa"

[search_highlight]
fg = "#1e1e2e"
bg = "#f9e2af"
```

The `syntax_theme` key selects a syntect theme by name (from syntect's bundled
set or user-installed `.tmTheme` files), cleanly separating **element styling**
(our TOML) from **code token coloring** (syntect).

## Dependencies

| Crate | Role |
| --- | --- |
| `pulldown-cmark` | CommonMark + GFM parser (tables, task lists, strikethrough) |
| `ratatui` | TUI framework (cross-platform, no ncurses) |
| `crossterm` | Terminal backend (raw mode, alternate screen, events) |
| `syntect` | Syntax highlighting, `default-fancy` feature (pure Rust) |
| `serde` / `serde_derive` | Theme config deserialization |
| `toml` | Config file parsing |
| `clap` | CLI argument parsing |
| `dirs` | Resolve `~/.config/vademecum/` |

## Architecture

Single binary crate with clear modules — easy to split into a Cargo workspace
later if needed.

```
vademecum/
├── Cargo.toml
├── README.md                # this file — source of truth
├── themes/                  # bundled default TOML themes
│   └── dark.toml
└── src/
    ├── main.rs              # entry point; wires everything; picks theme
    ├── cli.rs               # clap argument parsing
    ├── theme.rs             # Theme struct (serde) + ThemeLoader: default,
    │                        #   file, --config; color parsing (name/8-bit/hex);
    │                        #   built-in presets
    ├── document.rs          # load & cache markdown by path; canonical path + base dir
    ├── markdown/
    │   ├── mod.rs
    │   ├── ast.rs           # pulldown-cmark events → renderable AST
    │   │                    #   (headings, lists, tables, code w/ lang,
    │   │                    #   blockquotes, hr, links)
    │   └── links.rs         # wikilink detect/resolve ([[t]], [[t|alias]])
    ├── render/
    │   ├── mod.rs           # AST → styled lines (word-wrap to width)
    │   ├── styles.rs        # Theme → ratatui Style + ANSI strings (shared)
    │   ├── code.rs          # syntect wrapper: SyntaxSet lazily loaded,
    │   │                    #   per-block HighlightState, token styles → style
    │   └── layout.rs        # wrap text to area width, compute line positions
    └── ui/
        ├── mod.rs
        ├── app.rs           # TUI state machine (Normal / Search), history
        └── pager.rs         # ratatui view: scroll, /search + n/N, status bar,
                             #   Enter-follows-wikilink
```

### Data flow

1. **CLI** (`cli.rs`) parses args → target file path, mode, theme selection.
2. **Load** (`document.rs`) reads the `.md` file; records canonical path + base
   dir.
3. **Parse** (`markdown/ast.rs`) feeds the source to `pulldown-cmark`, building
   a lightweight renderable AST. Wikilinks (`[[...]]`) are detected in text
   events (`markdown/links.rs`), stripped of link syntax, styled distinctly,
   and recorded with their resolved target path.
4. **Render** (`render/`) lays the AST out into logical lines matched to
   terminal width, producing styled spans (both for the TUI and `--plain`).
5. **TUI** (`ui/pager.rs`) drives a ratatui view: scroll offset, keybindings,
   search highlight, `Enter`-on-wikilink → `document.rs` navigates to the
   linked file and re-renders (with navigation history).

### Syntax highlighting (`render/code.rs`)

- `syntect::parsing::SyntaxSet` with bundled syntaxes (`default-fancy`),
  `syntect::highlighting::ThemeSet`.
- For each fenced block: look up syntax by language tag; fall back to plain
  text for unknown languages.
- `HighlightLines` / `HighlightState` per block for correct multi-line context.
- Each token's syntect `Style` (color + font style) maps to the shared style
  used by both ratatui and the `--plain` ANSI writer.
- Lazy-load the syntax/theme sets once (they're expensive); cache parsed blocks
  per file + language so scrolling stays smooth.
- `--list-syntax-themes`; user-installed `.tmTheme` files in the config dir.

### TUI keybindings

| Key | Action |
| --- | --- |
| `j` / `k` / arrows | Scroll |
| `PgUp` / `PgDn` | Page scroll |
| `g` / `G` | Top / bottom |
| `/` | Search |
| `n` / `N` | Next / previous match |
| `Enter` | Follow wikilink under cursor / selection |
| `h` / `l` | Navigation history (back / forward) |
| `q` | Quit (restore terminal) |

## Testing

- **Unit tests**
  - `theme.rs` — color/TOML parsing (names, 8-bit, hex), unknown-key handling.
  - `links.rs` — resolution rules, alias parsing, missing-file handling.
  - `ast.rs` — block/inline event coverage.
  - `code.rs` — a fenced Rust block produces distinct token styles; unknown
    language falls back gracefully.
- **Integration tests** — spawn `vademecum --plain` against fixture `.md`
  files (including a wikilink pair) and assert on styled output; theme file
  loading with a fixture TOML.

## Verification

```
cargo build
cargo clippy
cargo test
```

Manual smoke tests: open `README.md` in the TUI, follow `[[...]]` links,
exercise `--plain`, `--list-themes`, `--list-syntax-themes`, and a custom
`--config` theme.

## Milestones

1. **Scaffold + `--plain` renderer** — crate, CLI, parser → AST → styles →
   ANSI stdout with the default theme. Verifiable: `vademecum --plain file.md`.
2. **Theme module** — TOML loading, color parsing, `--config`, built-in
   presets, wired into rendering.
3. **syntect code highlighting** — fenced block highlighting unified with the
   element theme.
4. **TUI pager** — alternate screen, word-wrap layout, scroll, search.
5. **Wikilinks** — detection, navigation, history, status bar.
6. **Tests, polish, docs** — full unit/integration suite, `--list-*` flags,
   release polish.

## License

TBD (pending decision, likely MIT).