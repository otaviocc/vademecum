# AGENTS.md — how to work in this repository

Instructions for AI agents (and humans) contributing to `vademecum`.

## 1. README.md is the source of truth

- `README.md` defines the product, architecture, data model, CLI, theme
  format, keybindings, testing strategy, and milestones. Read it fully before
  changing anything.
- When code and README disagree, the README wins. Fix the code, or amend the
  README **first**, in its own commit, with the reasoning in the commit body.
- Never add behaviour, flags, theme keys, or keybindings that the README does
  not describe. Propose the README change first.
- The README is written for readers, not as a changelog. Keep it consistent
  and current rather than appending notes.

## 2. Work is tracked in GitHub Issues and Milestones

- Repository: `otaviocc/vademecum` (use the `gh` CLI).
- Every plan milestone in README.md has one GitHub Issue, all grouped under
  the GitHub Milestone `v0.1.0`:

  | Issue | Plan milestone |
  | --- | --- |
  | #1 | Milestone 0: Scaffold |
  | #2 | Milestone 1: Stdout renderer |
  | #3 | Milestone 2: Theme files |
  | #4 | Milestone 3: Code highlighting |
  | #5 | Milestone 4: TUI pager |
  | #6 | Milestone 5: Links |
  | #7 | Milestone 6: Live reload (`--watch`) |
  | #8 | Milestone 7: Release |

- Each issue has a **Scope** checklist and **Exit criteria**. Both come from
  the README; the issue is a tracker, not a second specification.
- Milestones are worked **in order**. Do not start a milestone while the
  previous one is open unless the user asks.
- Tick checklist items in the issue as they land (`gh issue edit`). When the
  README changes in a way that affects an issue, update the issue checklist in
  the same PR.
- Bugs and follow-ups found while working get their own issue, assigned to
  the `v0.1.0` milestone if they block the release, otherwise unassigned.
  Do not silently widen the scope of the current issue.

## 3. Milestone session workflow

Each milestone is worked in its own agent session. The user opens the session
and names the milestone (for example "let's do milestone 2"). From there the
agent drives the following, without needing to be told:

### Start of session: plan before coding

1. Read `README.md` in full and `AGENTS.md`. Then read the milestone's GitHub
   Issue (`gh issue view <N>`), including its checklist state and comments,
   and check the previous milestone's issue is closed.
2. Check `git status`, `git log --oneline -10`, and that `main` is up to date
   (`git pull --ff-only`). Confirm `make lint && make test` are green on
   `main` before touching anything (skip for milestone 0, which creates
   the Makefile and crate).
3. Enter plan mode. Produce an implementation plan for **this milestone
   only**: files to create or change (following the README architecture
   tree), the order of work, how each Scope checklist item will be verified,
   and any README ambiguities or gaps discovered. Ask the user only about
   decisions that change the design; make routine calls yourself.
4. If the plan needs a README change (design gap, wrong assumption, new
   dependency), make that change as the first commit of the branch, before
   implementation, and update the issue checklist to match.
5. Only after the plan is approved: create the branch `m<N>-<slug>` and start.

### During the session

- Work through the Scope checklist in order; tick items on the issue as they
  land (`gh issue edit <N> --body ...` or a comment summarising progress).
- Prefer several small PRs over one big one for large milestones (TUI pager,
  Links). Each PR body says `Part of #N`; the last says `Closes #N`.
- Do not start work on the next milestone in the same session.

### End of session: capture learnings

Before declaring the milestone done, and again at the end of any partial
session, do all of the following:

1. **Update `README.md`** with anything learned that changes the design:
   behaviour that differs from what was specified, decisions taken while
   implementing, dependency or version changes, new flags or keys. The README
   must describe the code as it now exists. Own commit, `docs:` prefix.
2. **Update `AGENTS.md`** with process learnings: commands that turned out to
   be needed, pitfalls (toolchain, CI, snapshot handling), conventions that
   emerged. Keep it short and actionable; remove guidance that proved wrong.
3. **Update the issue**: tick completed items, comment with what is done, what
   is left (if partial), and links to the PRs. Close it only when every exit
   criterion is met and the closing PR is merged.
4. **Leave `main` green** and the working tree clean or the branch pushed.
5. Finish with a short handover in the chat: state of the milestone, open
   PRs, what the next session should pick up first.

## 4. Branches, commits, pull requests

- Never commit directly to `main`. One branch per issue, named
  `m<N>-<short-slug>` (for example `m0-scaffold`, `m4-tui-pager`). Large
  milestones (TUI pager, Links) may be split into several PRs from several
  branches; each PR still references the issue.
- Commit messages follow Conventional Commits with a short imperative
  subject and a body explaining *why*:
  `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, `ci:`.
  Existing history: `docs: rebuild project plan after design audit`.
- Keep commits focused. A README amendment is its own commit.
- Pull requests:
  - Title: `<type>: <summary>` matching the main commit.
  - Body: what changed, how it was verified (paste the commands run), and
    `Closes #N` only on the PR that completes the issue's exit criteria;
    otherwise `Part of #N`.
  - CI must be green (`fmt`, `clippy -D warnings`, tests on Linux, macOS,
    Windows, MSRV, audit) before asking for review.
  - Do not merge, tag, or publish without the user's explicit request.

## 5. Before opening a PR

Run locally and make sure all pass:

```sh
cargo fmt --all -- --check
make lint     # cargo clippy --all-targets -- -D warnings
make test     # cargo test
```

Plus the milestone's exit criteria from its issue (for rendering work,
review the `insta` snapshot diff deliberately; never `--accept` blindly).

## 6. Code conventions

- Rust edition 2024, MSRV in `Cargo.toml` (`rust-version`), `rustfmt.toml`
  as committed (`max_width = 130`, `use_small_heuristics = "Max"`).
- No `unsafe`. No `unwrap` outside tests; `expect` only with an invariant
  message. `anyhow` at the binary boundary, `thiserror` inside modules the UI
  matches on.
- Module layout, type names, and the `RenderedLine` contract are fixed by the
  README's Architecture and Core data model sections. Put code where the
  tree says it goes.
- Every new construct, flag, or theme element needs: a unit test, an entry
  in `tests/fixtures/elements.md` (if renderable), and a snapshot update.
- Dependencies: only those listed in the README dependency table. Adding one
  requires a README change first.
- UI conventions (header, hairline rules, statusbar priority, help popup)
  mirror [Holodeck](https://github.com/otaviocc/Holodeck); when in doubt,
  look at how Holodeck does it.

## 7. Communication

- State assumptions explicitly in the PR body when the README is ambiguous,
  and open a README amendment PR or issue to remove the ambiguity.
- Report test results faithfully. If something was not verified (for
  example, Windows behaviour on a macOS machine), say so.
- Prefer small, reviewable PRs over one large drop per milestone.

## 8. Toolchain notes

- `Cargo.lock` is committed: CI runs `cargo test --locked` and the MSRV job
  `cargo check --locked`. Regenerate it deliberately (`cargo update -p <crate>`),
  never by deleting it.
- Before pinning a dependency version from the README table, check it exists
  and that its own MSRV is ≤ our `rust-version`. `cargo generate-lockfile`
  prints "Locking N packages to latest Rust <msrv> compatible versions", which
  confirms cargo honoured `rust-version` during resolution.
- The MSRV lives only in `Cargo.toml`; the CI job greps it out, so bumping it
  is a one-line change plus the README's Project conventions section.
- This machine has a Homebrew `rustc`/`cargo` and **no `rustup`**, so a real
  MSRV build cannot be run locally — the `msrv` CI job is the only proof. Say
  so in the PR body rather than claiming it was verified locally.
- `clippy -D warnings` turns `dead_code` into a build failure, and in a binary
  crate every `pub` item unreachable from `main` is dead. When a milestone is
  split so that code lands before its caller, put
  `#[allow(dead_code, reason = "…")]` on the `mod` declaration and remove it in
  the PR that wires the code up. `#[expect(dead_code)]` does **not** work here:
  the test build uses those items, so the expectation goes unfulfilled and
  fails the other half of `--all-targets`.
- `cargo-insta` is not installed. A new snapshot is written as
  `tests/snapshots/*.snap.new`; read it in full, then `mv` it over the `.snap`
  and delete the `assertion_line:` header, which otherwise churns the file
  whenever the test moves in `stdout.rs`. `insta` stops a test at its first
  failing assertion, so a test holding several snapshots writes one `.snap.new`
  per run: accept, rerun, repeat. When several snapshots are the same document
  under different settings, check them against each other by stripping the
  escapes (`perl -pe 's/\e\[[0-9;]*m//g'`) and comparing hashes — identical
  text proves only the styling moved.
- Integration tests must neutralise the environment they run in, not just
  `NO_COLOR`: `vademecum()` in `tests/stdout.rs` also pins `XDG_CONFIG_HOME`
  and `APPDATA` at paths that do not exist, or a `theme.toml` in the
  developer's own config directory repaints every snapshot.
- `#[serde(untagged)]` does **not** survive `#[serde(flatten)]`: the buffered
  deserializer flatten uses does not hand an untagged enum the integer type the
  file wrote, so `fg = 208` fails to match a `u8` variant. `ColorSpec` has a
  hand-written visitor instead. Reach for one whenever a value can be more than
  one TOML type inside a struct that captures unknown keys.
- A raw string in a test that contains a hex color needs `r##"…"##`: `"#` is
  what closes `r#"…"#`, so `accent = "#ff0000"` inside one ends the literal
  early and the error points at the Rust, not at the string.
- syntect's `load_defaults_newlines()` is the Sublime Text default package
  set. It has no Swift, TypeScript, Kotlin or TOML (#16). Check what is
  actually bundled before promising a language: a quick throwaway binary
  printing `SyntaxSet::syntaxes()` settles it in a minute.
- syntect emits one region per token, and neighbouring tokens routinely share
  a style. `render::line::merge` joins them; without it every code line costs a
  dozen SGR sequences and the snapshots become unreadable. Reach for it
  whenever spans are built mechanically rather than by hand.
- A `.snap.new` is written per failing assertion, and a test holding three
  snapshots therefore needs three accept-and-rerun rounds. `cargo test --test
  stdout <name>` narrows it to the one under review.
- `insta` renders escapes as `␛`, which `diff` cannot line up. Comparing the
  snapshot bodies with `difflib` in a throwaway `python3` heredoc (repr-ing
  only the changed lines) shows what actually moved.
- Integration assertions cannot look for a phrase that spans several styles:
  once a line is highlighted, `fn main()` has escapes inside it. Anchor on a
  substring that lives inside one span, such as a string literal.
- Stage commits explicitly. `git add -A` after editing both the README and the
  source sweeps them into one commit, and a README amendment has to stand
  alone (§4).
- `clippy -D warnings` also makes an unconstructed enum variant a build
  failure, which is what decides how a TUI milestone splits. Slicing it by
  module (search.rs, then app.rs, then view.rs) leaves `Mode::Search` and
  `Status::Error` unbuilt for a PR or two; slicing it by **feature** — the
  shell, then search and help — means every PR wires up what it adds. Leave a
  variant out entirely until the milestone that constructs it.
- `ratatui::try_init()` installs a panic hook that *chains*: it restores the
  terminal and then calls whatever hook was already there. Anything of ours
  that has to run on a panic — releasing mouse capture, say — must therefore be
  installed **before** `try_init`, not after.
- `Frame::buffer_mut` does exist in 0.30, but writing the view as `Widget`
  impls is still worth it: a test can then render straight into a
  `Buffer::empty(rect)` with no `Terminal` at all, and use
  `Terminal::new(TestBackend::new(w, h))` plus `terminal.backend().buffer()`
  only for whole frames. Assert on the buffer read back as text
  (`buffer[(x, y)].symbol()`), not on `insta`: a second snapshot tree under
  `src/` is churn the view tests do not need.
- `Buffer::set_stringn(x, y, s, max, style) -> (u16, u16)` is the right
  primitive for painting a `StyledSpan`: it clips to the area, skips control
  characters and advances by display columns. Watch for it returning the same
  `x` it was given — that is a full line, and a loop that ignores it spins.
- Case-insensitive search must fold one character at a time against the
  original text. Matching over a `to_lowercase()` copy of the line is easier
  but its byte offsets index the copy, and `İ` folds to two characters, so the
  highlight lands on the wrong bytes.
- The TUI can be smoke-tested for real: `printf 'jjq' | script -q /dev/null sh
  -c 'stty rows 24 cols 80; ./target/debug/vademecum README.md'`, then strip
  the escapes with `perl -pe 's/\e\[[0-9;?]*[a-zA-Z]//g'`. Without the `stty`
  the pty is 0x0 and the pager correctly draws nothing, which looks like a bug
  and is not one. Keys reach it because crossterm reads `/dev/tty`, which is
  also why `cat x.md | vademecum -` works.
- Three things make that smoke test lie unless they are handled. The event
  loop **drains** a burst of events and draws once, so `printf 'jjq'` quits
  before painting anything the keys did: send them a `sleep 0.25` apart. The
  backend redraws only the cells that **changed**, so grepping for a whole
  phrase fails when the frame before it shared a prefix — grep for a string the
  previous frame did not contain (a statusbar notice works, a header title does
  not), or read the tail of the stream and follow the fragments. And `q` inside
  the help overlay closes the overlay, so a run that opens it needs a second
  `?` before the quit or it hangs forever.
- `layout::render` takes a context (theme + links) rather than more parameters.
  Anything layout needs to know about the document belongs in `Ctx`; adding a
  parameter instead ripples through every `*_to_lines` signature and every one
  of the ~20 layout tests.
- `Action` derives `Copy`, so a new variant cannot carry a `String`. Split the
  payload out (`Focus { forward: bool }`, not `Focus(String)`) or the derive
  goes, and with it the terse reducer tests.
- macOS's filesystem is case-insensitive, so a test for the resolver's *own*
  case folding has to put the file somewhere the relative rule cannot reach —
  a subdirectory — or the filesystem answers first and the test proves nothing.
- `App::new` takes `&Options`. A pager option that layout or resolution needs
  goes in there rather than into the parameter list, which is already four
  long and was five before.
