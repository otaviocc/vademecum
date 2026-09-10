# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`vademecum` is a single Rust binary that renders Markdown to a terminal, either
as styled stdout or as a full-screen pager.

## Commands

```sh
make build                        # cargo build --release
make test                         # cargo test
make lint                         # cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
make install                      # cargo install --path . --locked --force

cargo test --test stdout <name>   # one integration test
cargo test theme::loader          # one module's unit tests
```

`fmt`, `lint` and `test` must all pass before a PR. CI additionally runs the
tests on macOS and Windows, an MSRV `cargo check --locked --all-targets`, and
`cargo audit --deny warnings`. The MSRV lives only in `Cargo.toml`
(`rust-version`) and the CI job greps it out of there.

Building against the MSRV locally needs `rustup`, which is not present on
every machine this repo is worked on. Check (`command -v rustup`) before
claiming an MSRV build was verified; where it is missing, the `msrv` CI job is
the only proof, the release workflow's `rustup target add` steps cannot be
rehearsed either, and both should be reported as unverified rather than
implied. The same goes for the platforms you are not on — CI covers macOS,
Windows and Linux; a local run covers one.

## Architecture

`src/main.rs` is the whole control flow and is worth reading first: it parses
`cli.rs`, handles the exit-early flags (`--list-themes`, `--list-syntax-themes`,
`--resolve-links`), loads the document, then forks between the two output paths.

```
Document (src/document.rs, file or stdin; frontmatter split off the front)
  → markdown::ast::parse         SourceBlock tree over pulldown-cmark
  → markdown::links              wikilink collection and vault resolution
  → render::layout::render       SourceBlock + Ctx{theme, links} → Vec<RenderedLine>
      render::code               syntect highlighting, process-global cache
      render::line::merge        joins adjacent same-style spans
  → render::ansi::write_lines    stdout path
  or ui::run                     pager path (TTY and not --plain)
```

**The pager is a reducer plus a view.** `ui::input::action` turns a crossterm
event into an `Action`; `ui::app::App::apply` is the only thing that mutates
state; `ui::view`, `ui::outline` and `ui::properties` are `Widget` impls that
read it. `ui::mod::event_loop` owns the terminal, drains bursts of events and
draws once per burst. `App` never writes to the terminal — a copy is left in
`App::take_copy` for the loop to hand to `ui::clipboard`, which is what makes
copying testable.

The pager's list windows (contents, properties, help) share `ui::listing`
(selection, revealing, snapping, wheel-scrolling, row hit-testing as pure
functions over `(selected, top, last, height)`) and `view::centred` (popup
sizing and centring). A fourth window reuses both rather than copying them.

Supporting modules: `src/watch.rs` (the `--watch` file watcher, `notify` +
`notify-debouncer-mini`), `src/ui/search.rs` (finding a query in rendered text),
`src/ui/pulse.rs` (the marks a reload leaves on changed lines, and the clock
that cuts them — the only user of `similar`), `src/config.rs`
(`XDG_CONFIG_HOME`/`APPDATA` → `…/vademecum`).

**Themes are three layers in `src/theme/`:** `palette` (named colour slots),
`elements` (per-element styles), `loader` (built-ins, user file, merging), with
`color` holding the hand-written `ColorSpec` deserializer. `handbook` is the
default a reader gets (`DEFAULT`); `ansi` is the merge base a partial user file
inherits — they are different themes, and `BUILT_IN[0]` means neither.

`build.rs` bakes `syntaxes/*.sublime-syntax` plus syntect's defaults into a
binary pack at compile time; `themes/*.toml` are `include_str!`ed by
`theme::loader::BUILT_IN`. Nothing is read from disk at runtime except the
document, an optional user theme, and the vault.

## Tests

Unit tests live in a `mod tests` at the bottom of every source file.
`tests/stdout.rs` (snapshots, CLI behaviour), `tests/links.rs` and
`tests/pipe.rs` (a real pty via `rustix::pty`) are the integration suites;
`tests/common/mod.rs` builds the `Command`.

- `cargo-insta` is not installed. A new snapshot lands as
  `tests/snapshots/*.snap.new`: read it in full, `mv` it over the `.snap`, and
  delete the `assertion_line:` header. `insta` stops at the first failing
  assertion, so a test holding several snapshots needs one accept-and-rerun
  round per snapshot; narrow with `cargo test --test stdout <name>`.
- Never accept a rendering diff blindly. Strip the escapes
  (`perl -pe 's/\e\[[0-9;]*m//g'`) and compare the text: identical text proves
  only the styling moved.
- `tests/common/mod.rs` pins `XDG_CONFIG_HOME` and `APPDATA` at paths that do
  not exist, or a developer's own `theme.toml` repaints every snapshot. **A
  manual run has no such harness** — put `XDG_CONFIG_HOME=/nonexistent` on the
  command line before concluding a theme is not applying.
- Every new construct, flag, or theme element needs a unit test, an entry in
  `tests/fixtures/elements.md` if it is renderable, and a snapshot update.

## Code conventions

- Rust edition 2024, `rustfmt.toml` as committed (`max_width = 130`,
  `use_small_heuristics = "Max"`).
- No `unsafe`. No `unwrap` outside tests; `expect` only with an invariant
  message. `anyhow` at the binary boundary, `thiserror` inside modules the UI
  matches on.
- **No comments in Rust.** A file may carry a single `//!` line saying what it
  is, for navigation. Nothing else: no `///`, no `//`. A comment is a claim
  nobody checks, and it lends authority to whatever it sits above. Put the
  explanation in the commit message and PR body, which are dated and tied to a
  diff. If code needs a paragraph to be understood, prefer a name, a smaller
  function, or a test. (TOML and YAML in the repo are commented; the rule is
  about code.)
- **Optional content belongs in a window, not in the document.** Anything
  rendered into `lines` has a height, so showing it moves the body and the
  reading position and must be reconciled with `bound`/`reveal`/`snap`. A
  window costs none of that and leaves `render::layout` and the stdout
  snapshots untouched.
- `clippy -D warnings` makes `dead_code` and an unconstructed enum variant
  build failures. That decides how a large feature splits: slice it by feature,
  not by module, so every PR constructs what it adds.
- `Action` derives `Copy`, so a new variant cannot carry a `String`
  (`Focus { forward: bool }`, not `Focus(String)`).
- `layout::render` takes `Ctx`, and `App::new` takes `&Options`. New inputs go
  in those rather than into parameter lists that ripple through every signature
  and every test.
- Add a dependency only when it earns its place, and say why in the commit body.

## Workflow

- Repository `otaviocc/vademecum`, tracked as GitHub issues (`gh` CLI); an
  issue's **Scope** checklist and **Exit criteria** are the specification.
- Never commit directly to `main`. Branch `t<issue>-<slug>` (`t92-toc-overlay`),
  or a descriptive slug for untracked work (`docs-product-readme`).
- Conventional Commits with a short imperative subject and a body explaining
  *why*. Stage paths explicitly — `git add -A` sweeps unrelated edits in.
- PR title `<type>: <summary>`; the body says what changed and pastes the
  commands used to verify it. `Closes #N` only on the PR that meets the exit
  criteria. Do not merge, tag, or publish without being asked.
- `README.md` is a **product page** for someone using the program, with no
  architecture section. A change to a flag, key, theme key, or default updates
  it in the same PR (own commit, `docs:` prefix); a change to how the code works
  does not touch it.

## Smoke-testing the TUI

```sh
# BSD/macOS script(1): [file [command ...]]
printf 'q' | script -q /dev/null sh -c 'stty rows 24 cols 80; ./target/debug/vademecum README.md'
# util-linux script(1) needs -c instead
printf 'q' | script -q -c 'stty rows 24 cols 80; ./target/debug/vademecum README.md' /dev/null
```

Without the `stty` the pty is 0x0 and the pager correctly draws nothing. The key
sequence must end in `q` or the run hangs — and `q` inside an overlay closes the
overlay, so opening one needs a second `?`/`t` first. The event loop drains a
burst and draws once, so `printf 'jjq'` can quit before painting what the keys
did; when only the painting is in question, use a fixture that puts it in the
first frame and send nothing but `q`.

Read the capture with `tools/replay-frame.py <rows> <cols> <capture>` rather
than stripping escapes — ratatui paints runs, never the spaces between them, and
repaints changed cells only, so grep for a fragment (`Reload`), never a phrase.

`cargo build` can report the binary `Fresh` while `target/debug/vademecum` is a
day old, and `cargo test` uses that same copy via `Command::cargo_bin` — so the
whole integration suite can pass against a stale binary. Fix it with
`rm -rf target/debug/.fingerprint/vademecum-*`; deleting the binary or touching
`src/main.rs` does not work.
