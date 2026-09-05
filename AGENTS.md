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
