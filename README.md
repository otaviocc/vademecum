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
7. **UI chrome** — header (bold title + live right-aligned shortcut hints,
    hairline rule below) and statusbar (priority: error > transient status >
    `file — N lines`, hairline rule above), mirroring the TUI conventions of
    Holodeck. `?` opens a help overlay listing every keybinding.
8. **Mouse + URL support** — mouse-wheel scrolling in the TUI; `[text](url)`
    external links render as OSC-8 clickable hyperlinks in supported terminals
    (Kitty, iTerm2, tmux, etc.).
9. **Stdin support** — `vademecum -` reads markdown from stdin (pairs well
    with `git show … | vademecum -`).
10. **YAML frontmatter stripping** — `---`-delimited frontmatter is stripped
    from rendering (common in note-taking tools).
11. **Live reload** — `--watch` re-renders the document when it changes on
    disk, preserving scroll position.

## Usage

```
vademecum README.md                # interactive TUI
vademecum --plain README.md        # ANSI stdout (pipeable)
vademecum --plain README.md | less
vademecum --config ~/my.toml README.md
vademecum --watch README.md
cat notes.md | vademecum -         # read from stdin
vademecum --list-themes            # list built-in + user themes
vademecum --list-syntax-themes     # list syntect themes (built-in + user)
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

[chrome]
fg = "#89b4fa"
modifiers = ["bold"]

[hint]
fg = "#6c7086"

[cursor_line]
bg = "#313244"

[help_window]
fg = "#cdd6f4"
bg = "#181825"
border_fg = "#89b4fa"
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
| `notify` | File watching (`--watch`) |
| `unicode-width` | CJK-aware text width / wrapping |

## Architecture

Single binary crate with clear modules — easy to split into a Cargo workspace
later if needed.

```
vademecum/
├── Cargo.toml
├── README.md                # this file — source of truth
├── Makefile                 # build/run/test/clean/fmt/lint
├── themes/                  # bundled default TOML themes
│   └── dark.toml
├── .github/
│   └── workflows/
│       ├── ci.yml           # fmt, clippy, tests (linux/mac/win), MSRV, audit
│       └── release.yml      # tag → test, publish crates.io, build+attach binaries
└── src/
    ├── main.rs              # entry point; wires everything; picks theme
    ├── cli.rs               # clap argument parsing; stdin ("-") and --watch
    ├── theme.rs             # Theme struct (serde) + ThemeLoader: default,
    │                        #   file, --config; color parsing (name/8-bit/hex);
    │                        #   built-in presets
    ├── document.rs          # load & cache markdown by path (or stdin);
    │                        #   canonical path + base dir; frontmatter strip
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
        ├── app.rs           # TUI state machine (Browse / Search / Help),
        │                    #   history, watch-reload flag
        └── pager.rs         # header, statusbar, help overlay, mouse events,
                             #   OSC-8 links; scroll, /search + n/N,
                             #   Enter-follows-wikilink
```

### Data flow

1. **CLI** (`cli.rs`) parses args → target file path (or stdin `-`), mode,
   theme selection, `--watch`.
2. **Load** (`document.rs`) reads the `.md` file (or stdin); records canonical
   path + base dir (CWD for stdin); strips YAML frontmatter.
3. **Parse** (`markdown/ast.rs`) feeds the source to `pulldown-cmark`, building
   a lightweight renderable AST. Wikilinks (`[[...]]`) are detected in text
   events (`markdown/links.rs`), stripped of link syntax, styled distinctly,
   and recorded with their resolved target path.
4. **Render** (`render/`) lays the AST out into logical lines matched to
   terminal width, producing styled spans (both for the TUI and `--plain`).
5. **TUI** (`ui/pager.rs`) drives a ratatui view: header + statusbar + help
   overlay, scroll offset, mouse wheel, search highlight, OSC-8 links,
   `Enter`-on-wikilink → `document.rs` navigates to the linked file and
   re-renders (with navigation history). `--watch` re-renders on file change.

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

### UI layout & chrome

The TUI follows the conventions used by Holodeck: a slim header, a content
area, and a statusbar — separated by hairline `─` rules.

```
 vademecum                        ? help  / search  ⏎ follow  h/l history  q quit
 ─────────────────────────────────────────────────────────────────────────────
 <rendered markdown>                                                cursor ↓
 ─────────────────────────────────────────────────────────────────────────────
 notes.md — 132 lines
```

- **Header** — app title (` vademecum `) in bold `chrome` style, live shortcut
  hints right-aligned in `hint` style, hairline rule below.
- **Statusbar** — priority chain: `last_error` (error style) > transient
  status message (e.g. "Opened x.md") > `file — N lines`; hairline rule above.
- **Help overlay** — press `?` for a centered popup (60% width, auto height
  clamped to 40–90% of the terminal) listing every keybinding. Any key closes
  it. Uses the `help_window` theme style.
- The **cursor line** (reader position) is highlighted with the `cursor_line`
  theme style; `Enter` follows the wikilink on that line.

### TUI keybindings

| Key | Action |
| --- | --- |
| `j` / `k` / arrows | Move cursor / scroll |
| `PgUp` / `PgDn` | Page scroll |
| `g` / `G` | Top / bottom |
| `Mouse wheel` | Scroll (in supported terminals) |
| `/` | Search |
| `n` / `N` | Next / previous match |
| `Enter` | Follow wikilink under cursor |
| `h` / `l` | Navigation history (back / forward) |
| `?` | Help overlay (any key closes) |
| `q` | Quit (restore terminal) |

External URLs (`[text](https://…)`) are rendered as **OSC-8 clickable
hyperlinks**, so clicking them in a supported terminal opens your browser.

## Testing

- **Unit tests**
  - `theme.rs` — color/TOML parsing (names, 8-bit, hex), unknown-key handling,
    UI chrome keys (`chrome`, `hint`, `cursor_line`, `help_window`).
  - `links.rs` — resolution rules, alias parsing, missing-file handling.
  - `ast.rs` — block/inline event coverage.
  - `code.rs` — a fenced Rust block produces distinct token styles; unknown
    language falls back gracefully.
- **Integration tests** — spawn `vademecum --plain` against fixture `.md`
  files (including a wikilink pair) and assert on styled output; stdin via
  `echo … | vademecum -`; YAML frontmatter stripping; theme file loading with
  a fixture TOML.

## Verification

```
cargo build
cargo clippy
cargo test
```

Manual smoke tests: open `README.md` in the TUI, follow `[[...]]` links,
exercise `--plain`, `--list-themes`, `--list-syntax-themes`, `--watch`, stdin
(`vademecum -`), the `?` help overlay, and a custom `--config` theme.

## Makefile

Everyday tasks are wrapped in a Makefile so the common commands stay simple:

```sh
make build    # cargo build --release
make run      # cargo run --release --
make test     # cargo test
make clean    # cargo clean
make fmt      # cargo fmt
make lint     # cargo clippy --all-targets -- -D warnings
```

## Continuous Integration

GitHub Actions on `@otaviocc/vademecum`:

- **`.github/workflows/ci.yml`** — on `main` pushes and pull requests:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test` on a **matrix** of `ubuntu-latest`, `macos-latest`,
    `windows-latest` (guarantees cross-platform compilation)
  - an **MSRV** job that builds at the `rust-version` declared in `Cargo.toml`
  - a **cargo-audit** job for dependency vulnerability scanning
- **`.github/workflows/release.yml`** — on tags `v*.*.*`:
  1. verify the tag version matches `Cargo.toml`
  2. run the test suite
  3. `cargo publish` to crates.io (idempotent — a re-run after a partial
     failure treats "already uploaded" as success)
  4. build release binaries: Linux (`x86_64-unknown-linux-gnu`), macOS
     (aarch64 + x86_64 universal via `lipo`), Windows (`x86_64-pc-windows-msvc`)
  5. create a GitHub Release with the test results, install notes, and all
     platform binaries attached

## Project conventions

- `rustfmt.toml` — style-edition 2024, `use_small_heuristics = "Max"`,
  `max_width = 130` (matches Holodeck).
- Rust edition 2024, `rust-version` pinned in `Cargo.toml` (verified by the
  CI MSRV job).
- License: MIT.

## Milestones

1. **Scaffold + `--plain` renderer** — crate, CLI, parser → AST → styles →
   ANSI stdout with the default theme; stdin (`-`) and frontmatter stripping.
   Verifiable: `vademecum --plain file.md`.
2. **Theme module** — TOML loading, color parsing, `--config`, built-in
   presets, wired into rendering; `--list-themes` / `--list-syntax-themes`.
3. **syntect code highlighting** — fenced block highlighting unified with the
   element theme.
4. **TUI pager** — alternate screen, header + statusbar + `?` help overlay,
   word-wrap layout, scroll (keys + mouse), search, OSC-8 links.
5. **Wikilinks** — detection, navigation, history, status messages.
6. **`--watch` live reload** — file watcher re-renders preserving scroll.
7. **CI + tooling** — `ci.yml`, `release.yml` (compile + distribute), Makefile,
   rustfmt, LICENSE (MIT).
8. **Tests, polish, docs** — full unit/integration suite, `--list-*` flags,
   release polish.

## License

MIT.