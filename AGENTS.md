# AGENTS.md — how to work in this repository

Instructions for AI agents (and humans) contributing to `vademecum`, a single
Rust binary that renders Markdown to a terminal, as styled stdout or as a
full-screen pager.

## 1. The code is the source of truth

- There is no design document. Behaviour is defined by the code and pinned by
  the tests; when you want to know what something does, read it or read the
  test that covers it.
- `README.md` is a **product page** — what vademecum is, how to install it, how
  to use it, and the reference a user needs to write a theme. It is written for
  someone who wants to use the program, not for someone changing it. It carries
  no architecture section; §2 below is the only map.
- A change that alters what a user sees — a flag, a key, a theme key, a default
  — updates the README in the same PR. A change that alters only how the code
  works does not touch it.
- Keep the README accurate rather than exhaustive. Its tables (flags, palette
  slots, element names, keys) are checked against the code when they change; a
  wrong table is worse than a missing one.
- Explanations belong in commit messages and PR bodies, which are dated and
  cannot go stale. See §6 for why they do not belong in comments.

## 2. The map

`src/main.rs` is the whole control flow and is worth reading first: it parses
`cli.rs`, handles the exit-early flags (`--list-themes`, `--list-syntax-themes`,
`--resolve-links`), loads the document, and then forks.

```
Document (src/document.rs, file or stdin)
  → markdown::ast::parse         SourceBlock tree over pulldown-cmark
  → markdown::links              wikilink collection and vault resolution
  → render::layout::render       SourceBlock + Ctx{theme, links} → Vec<RenderedLine>
      render::code               syntect highlighting, process-global cache
      render::line::merge        joins adjacent same-style spans
  → render::ansi::write_lines    stdout path
  or ui::run                     pager path (TTY and not --plain)
```

- The pager is a reducer plus a view. `ui::input::action` turns a crossterm
  event into an `Action`; `ui::app::App::apply` is the only thing that mutates
  state; `ui::view` and `ui::outline` are `Widget` impls that read it.
  `ui::mod::event_loop` owns the terminal, drains bursts of events, draws once
  per burst, and calls `drain_copy` after every **wake**.
- `App` never writes to the terminal. A copy is left in `App::take_copy` for the
  loop to hand to `ui::clipboard`; that is what makes copying testable.
- The pager's windows (contents, properties, help) share two things a fourth one
  must reuse rather than copy: `ui::listing` holds selection, revealing,
  snapping, wheel-scrolling and row hit-testing as pure functions over
  `(selected, top, last, height)`, and `view::centred` sizes and centres the
  popup. Both exist because the second copy was about to be written.
- Themes are three layers in `src/theme/`: `palette` (named colour slots),
  `elements` (per-element styles), `loader` (built-ins, user file, merging).
- `build.rs` bakes `syntaxes/*.sublime-syntax` plus syntect's defaults into a
  binary pack at compile time. `themes/*.toml` are `include_str!`ed by
  `theme::loader::BUILT_IN`. Nothing is read from disk at runtime except the
  document, an optional user theme, and the vault.

Unit tests live in a `mod tests` at the bottom of every source file.
`tests/stdout.rs` (snapshots, CLI behaviour) and `tests/links.rs` are the
integration suites; `tests/common/mod.rs` builds the `Command`.

## 3. Working on an issue

- Repository: `otaviocc/vademecum` (use the `gh` CLI). Work is tracked as
  GitHub issues; the issue's **Scope** checklist and **Exit criteria** are the
  specification. The `v0.1.0` milestone is closed and issues are now picked up
  individually — there is no ordering to respect.
- Start of session: read `README.md` and this file, then `gh issue view <N>`
  including its comments (decisions and measurements are recorded there).
  Check `git status`, `git pull --ff-only`, and that `make lint && make test`
  are green on `main` before touching anything.
- **Plan before coding.** Produce a plan for this issue only: files to change,
  order of work, how each Scope item will be verified, and any README gaps
  found. Ask the user only about decisions that change the design.
- Then branch: `t<issue>-<slug>` (`t92-toc-overlay`). Untracked work uses a
  descriptive slug instead (`docs-product-readme`, `fix-clipboard-native-helper`).
- Tick checklist items on the issue as they land. New bugs get their own issue
  rather than widening the current one.
- End of session: update `README.md` if anything user-visible changed (own
  commit, `docs:` prefix), add process learnings to this file, update the issue,
  leave `main` green, and give a short handover in the chat.
- Report test results faithfully — say what was not verified (MSRV, Windows,
  macOS) rather than implying it was.

## 4. Commits, pull requests, stacks

- Never commit directly to `main`, with one exception: the release version bump
  (§5). One branch per issue.
- Conventional Commits with a short imperative subject and a body explaining
  *why*: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, `ci:`.
- Stage commits explicitly. `git add -A` sweeps unrelated edits into one commit;
  add the paths you mean.
- PR title `<type>: <summary>`; body says what changed and pastes the commands
  used to verify it. `Closes #N` only on the PR that meets the exit criteria,
  `Part of #N` otherwise. Prefer several small PRs to one large drop.
- Do not merge, tag, or publish without the user's explicit request.
- A milestone split into stacked PRs is **merged as a stack**, not one PR at a
  time, with the `github/gh-stack` extension:

  ```sh
  gh stack link 23 24 30      # bottom to top; PR numbers, URLs, or branches
  gh stack merge --yes --merge
  ```

  `link` needs no local state, so a stack opened one `gh pr create` at a time
  can be adopted after the fact.
- Do **not** merge a stack PR-by-PR with `--delete-branch`. Deleting the base of
  the PR above **closes** it rather than retargeting, and a closed PR whose base
  is gone cannot be recovered with `gh pr edit --base` or `gh pr reopen` —
  undoing it means pushing the branch back at its old SHA first.
- `gh pr edit --base` is also refused while a PR belongs to a stack. Reshaping
  goes `gh stack unstack <n>` → `gh pr edit --base` → `gh stack link` again.
- When a stacked PR is dropped and the one above needs its plumbing, do not
  reach for `git rebase --onto`: a commit that *edits* a function the dropped PR
  added conflicts in every hunk. Branch fresh off the new base,
  `git checkout <old-branch> -- <paths>` to take the final file state, remove
  what belonged to the dropped PR, and commit that as the intended history.
- Rebasing across a rewritten base conflicts in the `mod tests` block almost
  every time, because both sides append tests in the same place. Both sides are
  usually wanted: keep the two blocks and close the brace by hand.

## 5. Commands

```sh
cargo fmt --all -- --check
make lint                      # cargo clippy --all-targets -- -D warnings
make test                      # cargo test
cargo test --test stdout <name>   # one integration test, e.g. while accepting snapshots
cargo test theme::loader          # one module's unit tests
```

All three must pass before a PR. CI additionally runs the tests on macOS and
Windows, an MSRV `cargo check --locked --all-targets`, and `cargo audit --deny
warnings`.

Releasing (maintainer's request only): a `chore: release X.Y.Z` commit on
`main` bumping `version` in `Cargo.toml` and `Cargo.lock`, then a `vX.Y.Z` tag.
The tag is what `release.yml` answers to — it builds four targets, publishes to
crates.io and creates the GitHub release. `workflow_dispatch` rehearses
everything up to the publish.

## 6. Code conventions

- Rust edition 2024, MSRV in `Cargo.toml` (`rust-version`, currently 1.88),
  `rustfmt.toml` as committed (`max_width = 130`, `use_small_heuristics = "Max"`).
- No `unsafe`. No `unwrap` outside tests; `expect` only with an invariant
  message. `anyhow` at the binary boundary, `thiserror` inside modules the UI
  matches on.
- **No comments in Rust.** A file may carry a single `//!` line saying what it
  is, for navigation. Nothing else: no `///`, no `//`. (TOML and YAML in the
  repo are commented; that rule is about code.)

  A comment is a claim nobody checks. It goes stale silently, and it lends
  authority to whatever it sits above — a comment explaining why something is
  done a certain way makes a bug read as a decision, and the next person leaves
  it alone. Say it in the commit message and the PR body instead: those are
  dated and tied to a diff. If a piece of code needs a paragraph to be
  understandable, prefer a name, a smaller function, or a test that
  demonstrates the case.
- Every new construct, flag, or theme element needs a unit test, an entry in
  `tests/fixtures/elements.md` if it is renderable, and a snapshot update.
- **Optional content belongs in a window, not in the document.** Anything
  rendered into `lines` has a height, so showing or hiding it moves the body,
  moves the reading position, and has to be reconciled with `bound`/`reveal`/
  `snap` — #98 was built that way first and had to be rewritten. A window costs
  none of that, leaves `render::layout` untouched (so the stdout path and its
  snapshots cannot move), and the machinery above is already there.
- Dependencies: add one only when it earns its place, and say why in the commit
  body.
- UI conventions (header, hairline rules, statusbar priority, help popup)
  mirror [Holodeck](https://github.com/otaviocc/Holodeck).
- `Action` derives `Copy`, so a new variant cannot carry a `String`
  (`Focus { forward: bool }`, not `Focus(String)`) or the derive goes and the
  terse reducer tests go with it.
- `layout::render` takes `Ctx` rather than more parameters. Anything layout
  needs to know about the document goes in there; a new parameter ripples
  through every `*_to_lines` signature and every layout test.
- `App::new` takes `&Options`. A pager option that layout or resolution needs
  goes in there, not into the parameter list.
- `clippy -D warnings` makes `dead_code` **and an unconstructed enum variant**
  build failures. That is what decides how a large feature splits: slice it by
  feature (the shell, then search and help), not by module, so every PR
  constructs what it adds. Where code must land before its caller, put
  `#[allow(dead_code, reason = "…")]` on the `mod` declaration and remove it in
  the wiring PR — `#[expect(dead_code)]` fails instead, because the test build
  uses the items and the expectation goes unfulfilled.

## 7. Tests and snapshots

- `cargo-insta` is not installed. A new snapshot is written as
  `tests/snapshots/*.snap.new`: read it in full, `mv` it over the `.snap`, and
  delete the `assertion_line:` header, which otherwise churns the file whenever
  the test moves. `insta` stops at the first failing assertion, so a test
  holding several snapshots needs one accept-and-rerun round per snapshot;
  narrow with `cargo test --test stdout <name>`.
- Never `--accept` a rendering diff blindly. When several snapshots are the same
  document under different settings, compare them by stripping the escapes
  (`perl -pe 's/\e\[[0-9;]*m//g'`) and hashing: identical text proves only the
  styling moved. `insta` writes escapes as `␛`, which `diff` cannot line up —
  a throwaway `python3` heredoc using `difflib` and `repr` shows what moved.
- Integration tests neutralise the environment, not just `NO_COLOR`:
  `tests/common/mod.rs` pins `XDG_CONFIG_HOME` and `APPDATA` at paths that do
  not exist, or a `theme.toml` in the developer's config directory repaints
  every snapshot. **A manual run has no such harness** — put
  `XDG_CONFIG_HOME=/nonexistent` on the command line before concluding that a
  theme is not applying.
- An integration assertion cannot look for a phrase that spans several styles:
  once a line is highlighted, `fn main()` has escapes inside it. Anchor on a
  substring inside one span, such as a string literal.
- **A pager test whose fixture is shorter than the viewport cannot catch a
  clamping bug.** `bound()` lowers `top` to 0 in a document that fits on screen,
  so a fault that leaves the viewport pointing somewhere it should not be is
  silently corrected. #98's first design shipped one that a reader found and CI
  could not: the pty fixture (`tests/fixtures/frontmatter.md`, 8 lines) fits on
  screen, and the reducer test moved into the body first, so neither exercised
  the top of a long document — where every reader starts. Use `numbered(40)` and
  assert from line 0 whenever a change moves `top` or `cursor`.
- `app()`, `paged()` and `on_disk()` in `src/ui/app.rs` pin `width: Some(40)`,
  and `layout::wrap_width` returns an explicit width unchanged — so
  `Action::Resize` never re-wraps in those apps and never reaches `rerender`. A
  test about relayout has to build an `App` whose width follows the terminal
  (`Options::default()`), or it silently proves nothing.
- Process-global state (the highlight cache, the warning list) makes tests race
  under `cargo test`'s parallelism. **Readers** must take the lock too: a test
  asserting `Arc::ptr_eq` across two calls raced with the test that fills the
  cache, because only the writer took it.
- A filesystem-watch test writes in a **loop** until the watch sees it, against
  a generous deadline a passing run never pays: a backend is not always armed
  the instant `watch` returns. Never write the negative test — proving no event
  arrives is a race, and the pure matching functions cover negatives
  deterministically.
- CI runs macOS, whose filesystem is case-insensitive. A test for the resolver's
  *own* case folding has to put the file somewhere the relative rule cannot
  reach — a subdirectory — or the filesystem answers first and the test proves
  nothing.
- A raw string in a test containing a hex colour needs `r##"…"##`: `"#` closes
  `r#"…"#`, so `accent = "#ff0000"` ends the literal early and the error points
  at the Rust rather than the string.
- The layout tests' `detached()` helper roots the vault at `.`, which used to
  mean a missing wikilink walked the whole crate, `target/` included (~110ms).
  Since #103 a document outside a vault indexes only its own directory, so that
  is one `read_dir` of the crate root and the footgun is gone. Timing through
  `detached()` is still worth avoiding on principle, but it no longer lies by
  two orders of magnitude.
- **A vault is opt-in, and the tests must say which they are.** `folder()` in
  `src/markdown/links.rs` builds a directory, `vault()` builds one with
  `.obsidian/` in it. Before #103 there was only `vault()` and it created the
  marker *only if a test listed it* — so fourteen tests named for vault
  behaviour (symlinks, ambiguity, the dedupe, the whole invalidate cycle) were
  quietly leaning on the unbounded fallback and stopped testing their own names
  the moment it was bounded. Reach for `folder()` only when the point is the
  outside-a-vault rule.

## 8. Smoke-testing the TUI

The pager can be driven for real:

```sh
printf 'q' | script -q /dev/null sh -c 'stty rows 24 cols 80; ./target/debug/vademecum README.md'
```

Without the `stty` the pty is 0x0 and the pager correctly draws nothing. A run's
key sequence must end in `q` or it hangs — and `q` inside the help or contents
overlay closes the overlay, so a run that opens one needs a second `?`/`t`
before the quit.

- Keys reach the pager because `ui::tty::adopt_controlling_terminal` makes fd 0
  the terminal: when stdin is a pipe and stdout is a tty it opens
  `ttyname(stdout)` and `dup2`s it onto stdin, before `ratatui::try_init`. Do
  **not** rely on crossterm's own `/dev/tty` fallback (#95). It reaches for
  `/dev/tty` whenever stdin is not a tty, and on macOS kqueue refuses to
  register the `/dev/tty` clone device with `EINVAL`, so
  `UnixInternalEventSource::new` fails, `event::read()` returns
  `"Failed to initialize input reader"` for the rest of the process, and the
  pager paints one frame and quits. Opening `/dev/tty` succeeds — only the
  kqueue registration fails — so nothing errors where the problem is. The pty
  slave behind the same terminal (`/dev/ttys007`) registers fine, which is what
  the `dup2` gets us. Linux is unaffected: epoll accepts `/dev/tty`.
- `tests/pipe.rs` is the regression test, and it drives a real pty from Rust
  through `rustix::pty` rather than `script`. It covers piping with **no
  controlling terminal**, because a `cargo test` process has none and giving the
  child one needs `setsid`/`TIOCSCTTY` behind `unsafe`. That is a different
  failure from the kqueue one — without the fix the test dies in
  `ratatui::try_init` instead, since enabling raw mode also goes through
  `/dev/tty` — so the macOS event-source case is still only covered by the
  manual smoke test above. Both have the same cause and the same fix.
- Assert on a **positive** frame, not on the pager still being alive: send `G`
  and wait for the last line of a `LINE-001`…`LINE-400` fixture to appear. That
  is independent of the terminal size the child ends up believing in, which is
  not fully determined — crossterm sizes from `/dev/tty` when it can open one,
  so a `cargo test` run from a developer's terminal may size from *that*.

- **Read the frame with `tools/replay-frame.py <rows> <cols> <capture>`, do not
  strip the escapes.** ratatui positions the cursor and paints runs, never the
  spaces between them, so a stripped capture is a wall of run-together words.
  The replay also reassembles a frame built from several partial repaints.
- The backend repaints **changed cells only**, so a grep for a whole phrase
  fails when the previous frame shared a prefix: `Opened note.md` then
  `Reloaded note.md` comes out as `Reloaed note.md`. Grep for a fragment
  (`Reload`), never a phrase, and read the raw tail before believing a notice
  is missing. Statusbar counters are the worst case — anchor on document text,
  e.g. a fixture of `LINE-001`…`LINE-040`.
- The event loop **drains** a burst and draws once, so `printf 'jjq'` can quit
  before painting anything the keys did — even with `sleep`s between them. When
  only the painting is in question, sidestep navigation: use a fixture that puts
  what you want in the first frame and send nothing but `q`, which gives exactly
  one full frame.
- Mouse capture is on unless `--no-mouse`, and SGR reports fed on stdin work
  like keys: `\033[<65;10;10M` a wheel notch down, `\033[<64;10;10M` up,
  `\033[<0;C;RM` / `\033[<32;C;RM` / `\033[<0;C;Rm` press, drag and release.
  Those coordinates are 1-based while the reducer is 0-based. ratatui emits the
  SGR once per run of same-styled cells, so counting colour sequences
  undercounts what was painted.
- A background `( sleep N; … ) &` that edits the fixture proves nothing unless
  N is past the whole key sequence: an edit landing before the navigation is
  already in the file when it opens, and the run looks like a successful reload
  when none happened.
- `cargo build` in this tree can report the binary `Fresh` while
  `target/debug/vademecum` is a day old, so a smoke test silently runs the
  previous build — and so does `cargo test`, which uses the same copy via
  `Command::cargo_bin`. The whole integration suite can pass, or fail, against
  a stale binary on a green `main`. Fix it with
  `rm -rf target/debug/.fingerprint/vademecum-*` before `cargo build`, or a
  scratch `CARGO_TARGET_DIR` for a manual run. Deleting the binary or touching
  `src/main.rs` does not work: cargo restores the same file, mtime and all.

## 9. Theming

- Adding a built-in theme is three edits: the file in `themes/`, one
  `include_str!` row in `theme::loader::BUILT_IN` (whose array length is written
  out), and an `insta::assert_snapshot!` line in `tests/stdout.rs` plus its
  committed `.snap`. The invariants — every element resolves, no `base`, no
  unknown keys, the code band, `cursor` distinct from `subtle` — derive their
  list from `BUILT_IN` and need no edit. The two listing tests stay explicit on
  purpose: their subject is the order. Verify the frame as well as the snapshot
  by looking for each slot's `r;g;b` in a pty capture; `background` and often
  `muted` are legitimately absent.
- Adding a palette slot is six edits in `src/theme/palette.rs`: the struct,
  `slot()`, `Default`, `PaletteFile`, the `slots()` array *and its length*, and
  `set()`. `every_slot_reads_back_what_was_written_to_it` catches a miss in
  `slot()`/`set()` because it iterates `slots()`. **Splitting** an existing slot
  needs one thing the compiler cannot ask for: theme files in the wild set the
  old slot and say nothing about the new one, so the merge must make the new one
  follow the old (`resolve_palette` does this for `cursor` and `subtle`) or
  every user theme silently regresses.
- A fix belongs in the **element defaults**, not in `themes/ansi.toml`. A
  partial theme file merges over the defaults, so an override living only in the
  built-in file leaves every user theme with the old behaviour. The drift test
  between the two keeps this honest; if it needs an exceptions list, the fix is
  on the wrong side.
- The default theme and the merge base are two different themes: `handbook` is
  what a reader gets (`DEFAULT`), `ansi` is what a partial file inherits.
  `BUILT_IN[0]` means neither — say `DEFAULT`, or look one up by name.
- `#[serde(untagged)]` does **not** survive `#[serde(flatten)]`: flatten's
  buffered deserializer does not hand the enum the integer type the file wrote,
  so `fg = 208` fails to match a `u8` variant. `ColorSpec` has a hand-written
  visitor instead — reach for one whenever a value can be more than one TOML
  type inside a struct that captures unknown keys.
- A "warn once" that deduplicates against a list it also drains says the thing
  again after every collection. Keep a separate set of what has been said and
  never drain it: `theme_for` runs before the cache lookup, so otherwise "once"
  means once per code block per layout.

## 10. Rendering, syntax and the pager

- A bundled `.sublime-syntax` must work under **`fancy-regex`**, not merely
  under Sublime Text — the pure-Rust engine is what keeps vademecum free of an
  Oniguruma C dependency. It has no regex *subroutine calls* (`\g<1>`): a
  definition using one fails to load with `FeatureNotYetSupported`. Named
  backreferences (`\k<name>`) are fine. Loading is half the test: one candidate
  loaded cleanly and had no `keyword` scopes at all. Check it on a sample with
  **no string and no number in it**, or one literal will carry the whole test.
- syntect's own defaults have no Swift, TypeScript, Kotlin or TOML — hence
  `syntaxes/`. Check what is bundled before promising a language.
- syntect emits one region per token and neighbours routinely share a style;
  `render::line::merge` joins them. Without it every code line costs a dozen SGR
  sequences and the snapshots become unreadable. Reach for it whenever spans are
  built mechanically.
- `Buffer::set_stringn(x, y, s, max, style) -> (u16, u16)` is the right
  primitive for painting a `StyledSpan`: it clips, skips control characters and
  advances by display columns. Watch for it returning the `x` it was given —
  that is a full line, and a loop ignoring it spins.
- Write views as `Widget` impls: a test can then render into a bare
  `Buffer::empty(rect)` with no `Terminal`, and use `TestBackend` only for whole
  frames. Assert on cells read back as text (`buffer[(x, y)].symbol()`), not
  `insta`.
- Adding a row to the help table moves the popup's height, which
  `the_overlay_takes_three_fifths_of_the_width_and_sits_in_the_middle` pins.
  Expect to update it with every new binding.
- `App::apply`'s epilogue picks **one** of two opposite clamps per action, and
  which one is the whole design. `reveal()` moves the viewport onto the cursor;
  `snap()` moves the cursor onto the viewport. `Action::Scroll` gets `snap` —
  joining `moves_cursor` would let `reveal` drag the view back and undo the
  notch. `Action::Resize` gets `reveal` without joining that set: tiling the
  terminal is not a navigation intent. Only actions that **always** move the
  cursor belong in `moves_cursor`; a click landing on nothing must not drag the
  view. Both clamps run **after** `bound()`, never inside a handler — `bound` is
  what finally lowers `top`.
- **A linewise highlight cannot be routed through `App::selected`.** It returns
  `None` for an empty range, and a blank rendered line has no text — so, since
  Markdown puts a blank rendered line after every paragraph, list and heading, a
  selection painted that way comes out as alternating bars. Its right edge is
  ragged too, and differently ragged over a fence, whose trailing pad belongs to
  the line. Whole-line indicators follow `CursorLine` instead: one
  `set_style` over `area.width`. `selected()` means "the columns a drag
  covered" and nothing else. #109 was designed the other way first.
- Case-insensitive search folds one character at a time against the original
  text. Matching over a `to_lowercase()` copy is easier but its byte offsets
  index the copy, and `İ` folds to two characters, so the highlight lands on the
  wrong bytes.
- `ratatui::try_init()` installs a panic hook that *chains*. Anything of ours
  that must run on a panic — releasing mouse capture — is installed **before**
  `try_init`, not after.

## 11. Links, watching, clipboard

- `entry.file_type()` does **not** follow symlinks and `std::fs::metadata`
  does. That substitution is the whole of "follow symlinked directories", and it
  needs a `visited` set of *canonical* directory paths or a vault linking back
  to its root recurses forever. A test for that fails without the guard and
  **hangs** without the test.
- Following symlinks means one file is reachable by several names. Dedupe the
  index by canonical path, or a note reachable twice is reported as ambiguous
  with itself. Sort `read_dir` entries too, or which of two names the reader
  sees depends on the filesystem.
- A `notify` watch is armed on the **canonicalised** path: a document opened
  through a symlink lives elsewhere, and watching the link's own directory
  watches somewhere nothing is ever written. `unwatch`, conversely, must be
  handed back the *literal* `PathBuf` `watch` was given — macOS compares the
  string.
- Watching one directory non-recursively makes "is this event ours?" a
  comparison of file names, which is what makes it immune to the three ways a
  path can be spelled (`/var` vs `/private/var` on macOS). A tree watcher would
  have to keep all three; ours does not and should not grow them back.
- A debouncer error (`DebounceEventResult::Err`) is a reason to re-read, not to
  report: an inotify overflow loses exactly the events you would need to know
  what you missed.
- OSC 52 alone is **not** a clipboard. `crossterm`'s `osc52` feature costs no
  package in the lock file and is the only thing that carries a copy back over
  `ssh`, but Apple Terminal and GNOME Terminal implement no clipboard write at
  all. `src/ui/clipboard.rs` therefore does both: the platform tool (`pbcopy`,
  `wl-copy`, `xclip`, `xsel`, `clip.exe`) decides the result when installed,
  OSC 52 covers the rest. `arboard` would have been a dozen crates and
  X11/Wayland linkage.
- Gate the X11 and Wayland helpers on `DISPLAY` / `WAYLAND_DISPLAY`: a headless
  host often has `xclip` installed and it always fails there, so without the
  gate every `ssh` copy reports an error over a copy OSC 52 delivered. Keep the
  choice a pure function of `(target, wayland, x11)` — spawning is not testable,
  that is.
- Before believing a clipboard bug, split emission from delivery: capture a
  `printf 'y'` run and base64-decode `\x1b]52;c;([A-Za-z0-9+/=]*)` out of it. If
  the text comes back, the Rust is right and the terminal is dropping it.
  `tools/replay-frame.py` skips OSC entirely and cannot see this.
- A press that might start a drag cannot also be a click: the click moves to the
  **release**, which decides which it was by whether anything moved. Tests need
  a press-and-release helper, not a renamed click.

## 12. Toolchain, CI, performance

- `Cargo.lock` is committed; CI runs `cargo test --locked` and the MSRV job
  `cargo check --locked`. Regenerate deliberately (`cargo update -p <crate>`),
  never by deleting it. Before pinning a version, check its own MSRV is ≤ ours —
  `cargo generate-lockfile` prints "Locking N packages to latest Rust <msrv>
  compatible versions", which confirms cargo honoured `rust-version`.
- The MSRV lives only in `Cargo.toml`; the CI job greps it out, so bumping it is
  a one-line change plus the README.
- This machine has a distro `rustc`/`cargo` (Fedora, currently 1.98) and **no
  `rustup`**, so an MSRV build cannot be run locally — the `msrv` CI job is the
  only proof, and the release workflow's `rustup target add` steps cannot be
  rehearsed here either. Say so rather than claiming local verification.
- `.cargo/audit.toml` holds the accepted advisories. Its `[output]` table has no
  serde defaults — `deny = ["warnings"]` alone fails with `missing field quiet`
  — which is why CI passes `--deny warnings` on the command line and only
  `[advisories] ignore` lives in the file.
- Prefer running a tool directly over an Action that posts a check run
  (`rustsec/audit-check` needs `checks: write` and is red on fork PRs regardless).
- A `workflow_dispatch` workflow is only dispatchable once the file is on the
  **default branch**, so a release workflow cannot be rehearsed from its own PR.
- GitHub skips any job whose `needs` were skipped. Gating a job on
  `github.event_name == 'push'` silently skips the whole chain behind it and
  reports success. Put the condition on the *step* so `outputs` still flow.
- Cross-compiling is worth avoiding when a build script is involved:
  `runs-on: ubuntu-24.04-arm` makes host and target the same machine.
- CI jobs failing in ~2s with `steps: []` are not a code failure. Read
  `gh api repos/OWNER/REPO/check-runs/<id>/annotations`; here it was GitHub
  refusing to start jobs over billing, which needs the repo to be public.
- The Node 20 deprecation on `actions/checkout@v4` and the artifact actions was
  looked at in #91 and **deliberately left alone**; do not bump them
  opportunistically. If it is picked up, `@v5` does not fix it —
  `upload-artifact@v5` and `download-artifact@v5`/`@v6` still declare node20.
  The working set is checkout `@v7`, upload-artifact `@v7`, download-artifact
  `@v8`, and the artifact pair must move together (v4 and v5 artifacts are not
  cross-compatible).
- **Performance is settled; do not re-litigate it** (#75, #83). Release build,
  this machine. Draw path: `terminal.draw` averages 120µs a frame (p99 142µs)
  while scrolling a 28,199-line document against a ~16ms budget, two thirds of
  it ratatui's own diff and flush. A standing query with 34,000 matches adds
  4µs; a full-viewport drag adds 11µs. 300 wheel notches piped into a pty
  coalesce into one frame. The O(document) costs are in the reducer, once per
  action: `SearchConfirm` ~3.9ms, `SearchStep` 42ns.
- Opening a document scales with the number of **distinct languages** in it, not
  its length: the first fence of a language costs fancy-regex compiling that
  syntax (rust 30ms, typescript 89ms, python 15ms, ruby 10ms, swift 10ms,
  kotlin 8ms, go 8ms, toml 2ms, json 0.5ms), a second fence 120µs.
  `tests/fixtures/elements.md` (six languages) opens in 150ms; 200 copies of it
  lay out in 13.8ms once compiled; this repo's README opens in 20ms.
- `SyntaxSetBuilder::build` relinks contexts across all ~200 definitions, so
  adding one syntax costs as much as adding twenty (~93ms). That is why
  `build.rs` bakes the pack; loading the baked one is ~1.5ms.
