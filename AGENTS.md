# AGENTS.md — how to work in this repository

Instructions for AI agents (and humans) contributing to `vademecum`.

## 1. The code is the source of truth

- There is no design document. Behaviour is defined by the code and pinned by
  the tests; when you want to know what something does, read it or read the
  test that covers it.
- `README.md` is a **product page** — what vademecum is, how to install it, how
  to use it, and the reference a user needs to write a theme. It is written for
  someone who wants to use the program, not for someone changing it.
- A change that alters what a user sees — a flag, a key, a theme key, a default
  — updates the README in the same PR. A change that alters only how the code
  works does not touch it.
- Keep the README accurate rather than exhaustive. Its tables (flags, palette
  slots, element names, keys) are checked against the code when they change; a
  wrong table is worse than a missing one.
- Explanations belong in commit messages and PR bodies, which are dated and
  cannot go stale. See §6 for why they do not belong in comments.

## 2. Work is tracked in GitHub Issues and Milestones

- Repository: `otaviocc/vademecum` (use the `gh` CLI).
- The `v0.1.0` milestone was built as eight tracked milestones, one issue each,
  all now closed:

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

- An issue carries a **Scope** checklist and **Exit criteria**. That is where a
  piece of work is specified; the issue is the record, not a second document.
- Milestones are worked **in order**. Do not start a milestone while the
  previous one is open unless the user asks.
- Tick checklist items in the issue as they land (`gh issue edit`).
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
4. If the work changes what a user sees, the README changes with it in the same
   PR.
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
- Keep commits focused.
- Pull requests:
  - Title: `<type>: <summary>` matching the main commit.
  - Body: what changed, how it was verified (paste the commands run), and
    `Closes #N` only on the PR that completes the issue's exit criteria;
    otherwise `Part of #N`.
  - CI must be green (`fmt`, `clippy -D warnings`, tests on Linux, macOS,
    Windows, MSRV, audit) before asking for review.
  - Do not merge, tag, or publish without the user's explicit request.
- A milestone split into stacked PRs is **merged as a stack**, not one PR at a
  time. `gh stack` (the official `github/gh-stack` extension) does it
  atomically: everything up to the chosen PR lands in one all-or-nothing
  operation, so nothing has to be retargeted or rebased between merges.

  ```sh
  gh stack link 23 24 30      # bottom to top; PR numbers, URLs, or branches
  gh stack merge --yes --merge
  ```

  `link` needs no local tracking state — it takes PRs that already exist and
  chains their bases — so a stack opened one `gh pr create` at a time can be
  adopted after the fact. `gh stack merge` checks only that each PR is open and
  not a draft; branch protection is evaluated by GitHub when the merge runs.
- Do **not** merge a stack PR-by-PR with `--delete-branch`. Deleting the base of
  the PR above **closes** that PR rather than retargeting it, and a closed PR
  whose base branch is gone cannot be recovered directly: `gh pr edit --base
  main` refuses ("cannot change the base branch of a closed pull request") and
  so does `gh pr reopen`. Undoing it means pushing the deleted branch back at
  its old SHA, reopening, retargeting, then deleting it again.

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
- **No comments.** A file may carry a single `//!` line saying what it is, for
  navigation. Nothing else: no `///`, no `//`.

  The reasoning is that a comment is a claim nobody checks. It goes stale
  silently, and worse, it lends authority to whatever it sits above — a comment
  explaining why something is done a certain way makes a bug read as a decision,
  and the next person leaves it alone. The code and the tests are the two things
  CI keeps honest, so those are what a reader should have to trust.

  Say it in the commit message and the PR body instead. Those are dated, tied to
  a diff, and never claim to describe code they have drifted from. If a piece of
  code needs a paragraph to be understandable, prefer a name, a smaller
  function, or a test that demonstrates the case.
- Every new construct, flag, or theme element needs: a unit test, an entry
  in `tests/fixtures/elements.md` (if renderable), and a snapshot update.
- Dependencies: add one only when it earns its place, and say why in the commit
  body.
- UI conventions (header, hairline rules, statusbar priority, help popup)
  mirror [Holodeck](https://github.com/otaviocc/Holodeck); when in doubt,
  look at how Holodeck does it.

## 7. Communication

- State assumptions explicitly in the PR body, and open an issue for anything
  left unresolved rather than leaving it implied.
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
  developer's own config directory repaints every snapshot. **A manual run has
  no such harness**, so a pty smoke test of a theme needs
  `XDG_CONFIG_HOME=/nonexistent` on the command line — on this machine
  `~/.config/vademecum/theme.toml` makes `kanagawa-dragon` the default, and
  without the override a run "showing" none of a new theme's colours is
  reporting the reader's own configuration back at you.
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
- Stage commits explicitly. `git add -A` sweeps unrelated edits into one
  commit; add the paths you mean.
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
- The draw path was measured for #75 and does not need optimising; do not
  re-litigate it. Release build, 200x50 terminal, a 28,199-line document:
  `terminal.draw` averages **120µs** a frame (p99 142µs) while scrolling, against
  a ~16ms notch budget. Two thirds of that is ratatui's diff and flush —
  painting into a bare `Buffer` is 38µs — so it is not ours to optimise. A
  standing query with 34,000 matches adds 4µs and a drag across the whole
  viewport adds 11µs, which is what `RenderedLine::text()` costs; it is not on
  the ordinary scrolling path at all, because `view::highlight` and
  `App::selected` both return before calling it. And the event loop's `try_recv`
  drain really does coalesce: 300 wheel notches piped into a pty come out as one
  frame, with the run taking the same 0.02s as quitting immediately. The
  O(document) costs all sit in the reducer, where a reader pays them once per
  action: the plain mirror for search is 1.4ms and `search::find` 2.5ms over
  28,199 lines, so a `SearchConfirm` is ~3.9ms and a `SearchStep` 42ns.
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
- That smoke test drives the **mouse** too: capture is on unless `--no-mouse`,
  and SGR wheel events fed
  on stdin work like keys — `\033[<65;10;10M` is a notch down and `\033[<64;10;10M`
  a notch up. What it cannot easily prove is the statusbar, because the backend
  redraws changed cells only: moving one line in a long document repaints the
  single digit that changed, so grepping for `line 6/79` finds nothing even
  though the pager is right. Anchor on document text instead — a fixture of
  `LINE-001`…`LINE-040` makes the viewport's travel readable as a list of
  fragments, and the last frame's `ESC[<row>;1H` plus the `cursor_line`
  background says exactly which line the cursor ended on.
- Three things make that smoke test lie unless they are handled. The event
  loop **drains** a burst of events and draws once, so `printf 'jjq'` quits
  before painting anything the keys did: send them a `sleep 0.25` apart. On this
  machine even that fails — `{ sleep 0.5; printf "$J"; sleep 0.6; printf 'q'; } |
  script -q /dev/null …` delivered the whole sequence in one drain and the
  capture held exactly one frame, the initial one, so the keys were invisible.
  When only the *painting* is in question and not the navigation, sidestep it:
  point the run at a fixture that puts what you want to see in the first frame
  (a code fence on line 1 shows the band and the cursor line over it) and send
  nothing but `q`. The
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
- cargo-audit's `[output]` table in `.cargo/audit.toml` has no serde defaults:
  writing `deny = ["warnings"]` alone fails to parse with `missing field quiet`,
  so every field has to be spelled out. `[advisories] ignore = [...]` is fine on
  its own, which is why the ignore list lives in the file and `--deny warnings`
  stays on the command line in CI.
- A GitHub Action that posts a check run (`rustsec/audit-check`) needs
  `checks: write`, and declaring it still leaves the job red on a pull request
  from a fork, whose token is read-only whatever the workflow asks for. Prefer
  running the tool directly when there is one.
- A bundled `.sublime-syntax` has to work under **`fancy-regex`**, not merely
  under Sublime Text. The pure-Rust engine is what keeps vademecum free of a C
  dependency on Oniguruma, and it does not implement regex *subroutine calls*
  (`\g<1>`): a definition using one fails to load outright with
  `FeatureNotYetSupported`. Named backreferences (`\k<name>`) are fine. And
  loading is only half the test — one syntax loaded cleanly and turned out to
  have no `keyword` scopes at all, so a fence came out in a single colour.
  Check a candidate on a sample with **no string and no number in it**; a
  sample containing `"hi"` passes on the strength of that one literal.
- `SyntaxSetBuilder::build` relinks contexts across all ~200 bundled
  definitions, so adding one syntax costs about as much as adding twenty:
  ~8ms to load syntect's dump, ~93ms to take it apart and put it back adding
  nothing. That is why `build.rs` bakes the pack. Loading the baked one is ~1ms
  (measured 1.5-1.8ms), which is the packaging's whole cost.
  The **first** paint of a given language is much dearer than "a few ms", and
  that was measured for #75 on this machine, release build: rust 30ms,
  typescript **89ms**, python 15ms, ruby 10ms, swift 10ms, kotlin 8ms, go 8ms,
  toml 2ms, json 0.5ms. It is once per language per process — a second rust
  fence costs 120µs and a third 50µs — and it is fancy-regex compiling that
  syntax's patterns, not anything vademecum does. So the cost of opening a
  document scales with the number of **distinct languages** in it, not with its
  length: `tests/fixtures/elements.md` (140 lines, six highlighted languages)
  opens in 150ms end to end, while 200 copies of it — 28,199 lines — lay out in
  13.8ms once the patterns are compiled. Ordinary documents are not affected:
  this repo's README (toml and sh) and AGENTS.md (sh) both open in 20ms. See #83.
- **Do not time `layout::render` through the tests' `detached()` helper.** It
  roots the vault at `.`, and `walk` skips only dot-directories, so resolving the
  first wikilink walks the entire crate — `target/` included — for about 110ms on
  this machine. An earlier version of the note above reported 277ms to lay the
  fixture out and blamed syntax highlighting for all of it; 110ms of that was the
  test harness indexing the build directory. Time the binary instead
  (`/usr/bin/time -p ./target/release/vademecum --plain …`), or use a fixture
  with no links in it.
- `entry.file_type()` does **not** follow symlinks and `std::fs::metadata` does.
  That one substitution is the whole of "follow symlinked directories" — and it
  needs a `visited` set of *canonical* directory paths, or a vault linking back
  to its own root recurses forever. A test for that fails without the guard and
  **hangs** without the test.
- Following symlinks means one file can be reached by several names. Dedupe the
  index by canonical path or a note reachable twice is reported as ambiguous
  with itself, which makes it unfollowable — worse than not following symlinks
  at all. Sort `read_dir` entries too: without it, which of two names the reader
  is shown depends on what the filesystem happened to return first.
- Adding a built-in theme is three edits: the file in `themes/`, one
  `include_str!` row in `BUILT_IN` (whose array length is written out), and an
  `insta::assert_snapshot!` line plus its committed `.snap`. The invariants —
  every element resolves, no `base`, no unknown keys, the code band, `cursor`
  distinct from `subtle` — derive their theme list from `BUILT_IN` and cover a
  new theme without being touched. The two listing tests stay explicit on
  purpose: their subject is the order. Check the frame, not just the snapshot,
  by looking for each slot's `r;g;b` in a pty capture — `background` and often
  `muted` are legitimately absent, which the existing themes confirm.
- Adding a palette slot is six edits in `src/theme/palette.rs` — the struct,
  `slot()`, `Default`, `PaletteFile`, the `slots()` array *and its length*, and
  `set()` — and `every_slot_reads_back_what_was_written_to_it` catches a missed
  one in `slot()`/`set()` for free, because it iterates `slots()`. Splitting an
  existing slot in two needs one more thing the compiler cannot ask for: a
  theme file already in the wild set the old slot and says nothing about the new
  one, so the merge has to make the new one follow the old (`resolve_palette`
  does this for `cursor` and `subtle`) or every user theme silently regresses.
- A fix belongs in the **element defaults**, not in `themes/ansi.toml`. A
  partial theme file merges over the defaults, so an override that lives only in
  the built-in file leaves every user theme with the old behaviour while the
  shipped one looks right. The drift test between the two is what keeps this
  honest; if it needs an exceptions list, the fix is probably on the wrong side.
  Note the default theme and the merge base are **two different themes**:
  `handbook` is what a reader gets, `ansi` is what a partial file inherits. So
  `BUILT_IN[0]` means neither — say `DEFAULT`, or look one up by name the way
  the `built_in` test helper does.
- A "warn once" that deduplicates against a list it also drains says the thing
  again after every collection. Keep a separate set of what has been said and
  never drain it — otherwise, since `theme_for` runs before the cache lookup,
  "once" means once per code block per layout.
- Process-global state (the highlight cache, the warning list) makes tests race
  under `cargo test`'s default parallelism. The lock has to be held by
  **readers** too: a test asserting `Arc::ptr_eq` across two calls raced with
  the test that fills the cache, because only the writer took it.
- `git rebase` across a stack whose base was rewritten conflicts in the test
  module almost every time, since both sides add tests at the same place. Both
  sides are usually wanted; keep the two blocks and close the brace by hand.
- CI jobs that fail in ~2s with `steps: []` are not a code failure. Read
  `gh api repos/OWNER/REPO/check-runs/<id>/annotations` — in this repo it was
  GitHub refusing to start jobs over account billing, which no amount of
  re-running fixes. Free Actions need the repository to be public.
- A `workflow_dispatch` workflow is only dispatchable once the file is on the
  **default branch**. A release workflow therefore cannot be rehearsed from its
  own PR: merge it first — which is safe, since it answers only to tags and
  manual runs — then dispatch from `main`.
- A version number in an issue is not a fix; `runs.using` in the action's own
  `action.yml` at that tag is. #91 prescribed `@v5` for the Node 20 deprecation,
  but only `checkout@v5` is `node24` — `upload-artifact@v5` and
  `download-artifact@v5` still declare `node20`, and `download-artifact` needed
  `@v8`. Read it before believing a bump:
  `gh api "repos/actions/<a>/contents/action.yml?ref=<tag>" --jq .content | base64 -d | grep using`.
  Quote that URL in zsh, or the `?` globs and the call fails as "no matches".
  `upload-artifact` and `download-artifact` move **together**: the upload major
  that adds unzipped uploads and the download major that stops assuming a zip
  are a matched pair, and only the latter runs in the tag-only `github release`
  job, so a PR cannot prove it — the next real tag does.
  The proof is zero annotations, not a green run: the jobs were always green,
  since GitHub was force-running Node 24 anyway. Count them per job with
  `gh api repos/OWNER/REPO/check-runs/<id>/annotations --jq length`.
- GitHub skips any job whose `needs` were skipped. Gating a job on
  `github.event_name == 'push'` to make it tag-only will silently skip the whole
  chain behind it and report the run as successful. Put the condition on the
  *step* and let the job always run, so its `outputs` still reach everything
  downstream.
- Cross-compiling is worth avoiding when a build script is involved: `build.rs`
  runs on the host, so the target build needs a linker installed and the
  host/target split has to be right. `runs-on: ubuntu-24.04-arm` makes host and
  target the same machine and the question disappears. Free on public repos.
- To capture a TUI frame for documentation, **replay the stream, do not strip
  it**. ratatui positions the cursor and paints runs, and never emits the spaces
  between them, so `perl -pe 's/\e\[[0-9;?]*[a-zA-Z]//g'` yields a wall of
  run-together words. A ~30-line Python emulator that honours `CUP`, `EL`, `ED`
  and `CUF` into a fixed grid reproduces the frame exactly — and, because it
  replays cursor moves, it also reconstructs a frame built from several partial
  repaints, which is what defeats the grep-for-a-fragment trick above.
- The same smoke test drives a **drag**: press, drag and release are
  `\033[<0;C;RM`, `\033[<32;C;RM` and `\033[<0;C;Rm`, and those coordinates are
  1-based while everything in the reducer is 0-based — a report at row 5 is the
  third content line, not the fourth. It proves the clipboard too, since OSC 52
  goes out through the same stream: `\x1b]52;c;<base64>` is in the capture and
  decodes to exactly what was copied. Note ratatui emits the SGR **once** for a
  run of cells that share a style, so a selection spanning two lines shows as
  one colour sequence followed by two cursor moves; counting colour sequences
  undercounts what was painted.
- OSC 52 alone is **not** a clipboard. `crossterm`'s `osc52` feature costs no
  package in the lock file (`base64` is already there) and is the only thing
  that carries a copy back over `ssh`, but it is a request the terminal is free
  to ignore — and Apple Terminal and GNOME Terminal implement no clipboard write
  at all, so on a stock macOS or Fedora desktop it does nothing (#69). Spawning
  the platform's own tool (`pbcopy`, `wl-copy`, `xclip`, `xsel`, `clip.exe`)
  costs no crate either, so `src/ui/clipboard.rs` does both: the native tool
  decides the result when one is installed, OSC 52 covers the rest. `arboard`
  would have been a dozen crates and X11/Wayland linkage.
- Gate the X11 and Wayland helpers on `DISPLAY` / `WAYLAND_DISPLAY`. A headless
  host often has `xclip` installed and it always fails there; without the gate
  every `ssh` copy reports an error over a copy that OSC 52 delivered fine.
  Keep the choice a pure function of `(target, wayland, x11)` so all five arms
  are testable on one machine — spawning is not.
- Before believing a clipboard bug, split emission from delivery:
  `printf 'y' | script -q /dev/null sh -c 'stty rows 24 cols 80; ./target/debug/vademecum README.md'`
  piped through a base64 decode of `\x1b]52;c;([A-Za-z0-9+/=]*)`. If the text
  comes back out, the Rust is right and the terminal is dropping it. Note the
  key sequence must end in `q` or the run hangs, and `tools/replay-frame.py`
  skips OSC entirely, so it cannot see this.
- `cargo build` in this working tree can report the binary `Fresh` while
  `target/debug/vademecum` is a day old, so a pty smoke test silently runs the
  previous build. Two remedies work: `rm -rf target/debug/.fingerprint/vademecum-*`
  makes the next `cargo build` rebuild the binary in place, which is what
  `cargo test` needs, and a scratch `CARGO_TARGET_DIR` gives a manual run a
  binary that is certainly current. Touching `src/main.rs` does neither, and
  neither does deleting `target/debug/vademecum`: cargo puts the same stale file
  back, mtime and all. `cargo test` uses that copy too (`Command::cargo_bin`,
  `tests/common/mod.rs`), so the whole integration suite, snapshots included,
  can pass against a build from yesterday — **or fail against it on a `main`
  that is green**, which is how this last surfaced: `--list-themes` was missing
  a theme the source had had for a day. Checking `strings` for a string only the new code has works
  only if the string really is new: a hex colour picked out of a theme file is a
  bad probe, because the same value usually sits in another slot.
- The reducer must not write to the terminal, or nothing about copying is
  testable. `App` fills an outbox that the event loop drains after every **wake**
  — not just after an input event, or a copy produced by a reload is dropped —
  which keeps "what would be copied" a value a test can assert on. The drain is
  also where a failed copy turns into a statusbar notice: the reducer sets the
  optimistic one, `App::report` overwrites it.
- A press that might start a drag cannot also be a click: the click has to move
  to the **release**, and the release decides which it was by whether anything
  moved. Existing tests that applied a click action directly need a helper that
  presses and releases, not a rename.
- Adding a row to the help table moves the popup's height, and
  `the_overlay_takes_three_fifths_of_the_width_and_sits_in_the_middle` pins it.
  Expect to update that test with every new binding.
- Sending a single `q` gives exactly one full frame: the loop draws, reads the
  key, and quits without drawing again, so there is no partial repaint to
  reassemble.
- `App::new` takes `&Options`. A pager option that layout or resolution needs
  goes in there rather than into the parameter list, which is already four
  long and was five before.
- A `notify` watch has to be armed on the **canonicalised** path. A document
  opened through a symlink lives in another directory, and watching the link's
  own would watch a directory nothing ever writes to — the reader would see
  nothing reload, ever, with no error to explain it. `unwatch`, on the other
  hand, must be handed back the *literal* `PathBuf` that `watch` was given:
  macOS compares the string, not the path.
- Watching one directory non-recursively makes "is this event ours?" a
  comparison of file names, which is also what makes it immune to the three
  ways a path can be spelled (as given, absolutised, canonicalised — on macOS
  the difference between `/var` and `/private/var`). A watcher over a *tree*
  has to strip a prefix and therefore has to keep all three spellings; ours
  does not, and should not grow them back.
- A debouncer error (`DebounceEventResult::Err`) is a reason to re-read, not to
  report: an inotify overflow loses exactly the events you would need in order
  to know what you missed, and re-reading one file is cheaper than being wrong.
- A filesystem test writes in a **loop** until the watch sees it, against a
  generous deadline a passing run never pays. A backend is not always armed the
  instant `watch` returns — FSEvents especially — and a single write landing in
  that window is simply lost. Never write the negative test: proving no event
  arrives is a race, and the pure matching functions cover the negatives
  deterministically.
- The pty smoke test's statusbar lies twice over, not once. The backend paints
  changed cells only, so `Opened note.md` followed by `Reloaded note.md` comes
  out of the stream as `Reloaed note.md` — the `d` was already there and was
  not repainted. Grep for a fragment (`Reload`), never a phrase, and read the
  raw tail before believing a notice is missing.
- `gh pr edit --base` is **refused** while a PR belongs to a `gh stack`:
  "Cannot change the base branch because the pull request is part of a stack."
  Reshaping a stack — dropping a middle PR, retargeting — therefore goes
  `gh stack unstack <n>`, then `gh pr edit --base`, then `gh stack link` again.
  Unstacking removes only the grouping on GitHub; the PRs and their branches are
  untouched.
- When a stacked PR is dropped and the one above it needs its plumbing, do not
  reach for `git rebase --onto`. It works only for commits that never touch what
  the dropped PR created; a commit that *edits* a function the dropped PR added
  conflicts in every hunk. Branch fresh off the new base, `git checkout
  <old-branch> -- <paths>` to take the final file state, remove what belonged to
  the dropped PR, and commit it as the intended history. Commits that are
  genuinely independent (the layer above) still rebase cleanly with `--onto`.
- `apply`'s epilogue in `src/ui/app.rs` picks **one** of two opposite clamps per
  action, and which one is the whole design. `reveal()` moves the viewport onto
  the cursor; `snap()` moves the cursor onto the viewport. `Action::Scroll` gets
  `snap` — it must not join `moves_cursor`, because `reveal` after a notch would
  drag the view back and undo the scroll. `Action::Resize` gets `reveal` without
  joining that set either: a window manager tiling the terminal is not a
  navigation intent, so the view moves to the reader's line rather than the
  reverse. Everything in `moves_cursor` gets `reveal`, and only actions that
  **always** move the cursor belong there — a click that lands on nothing would
  otherwise drag the view somewhere the reader did not ask for.
  Both clamps run **after** `bound()`, never inside a handler: `bound` is what
  finally lowers `top`, so a clamp computed before it uses a `top` that then
  changes. In a document shorter than the viewport that is the difference
  between a no-op and silently moving the reading position.
- A smoke test whose fixture is edited by a background `( sleep N; ... ) &`
  proves nothing unless N is past the keystrokes: an edit that lands *before*
  the navigation is already in the file when it opens, and the run looks like a
  successful reload while no reload happened. Time the writer against the whole
  key sequence, not against the start.
