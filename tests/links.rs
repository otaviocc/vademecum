//! Link resolution over the vault fixture, end to end.
//!
//! `--resolve-links` is the only place the resolver's answers are visible from
//! outside the binary, which is why it exists: every rule in the README's link
//! model is asserted here against real files on disk.

mod common;

use common::{run, vademecum};

/// The report, with the separators flattened so an assertion reads the same on
/// Windows as it does anywhere else.
fn resolve(args: &[&str]) -> String {
    let mut arguments = vec!["--plain", "--resolve-links"];
    arguments.extend_from_slice(args);
    run(&arguments).replace('\\', "/")
}

/// The same, run from inside `directory`. The vault root is discovered by
/// climbing from the document, so where vademecum was *started* changes the
/// answer and has to be testable.
fn resolve_from(directory: &str, args: &[&str]) -> String {
    let mut command = vademecum();
    command.current_dir(directory).args(["--plain", "--resolve-links"]).args(args);
    let output = command.output().expect("vademecum runs");
    assert!(output.status.success(), "vademecum failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("output is utf-8").replace('\\', "/")
}

/// One reported link: kind, destination, target. A destination can contain a
/// space (`note#A Heading`), so the columns are split on the arrow first and
/// on the first gap second, never on whitespace throughout.
fn parse(line: &str) -> (&str, &str, &str) {
    let (left, target) = line.split_once(" -> ").expect("every line has an arrow");
    let (kind, destination) = left.split_once(' ').expect("every line has a kind");
    (kind, destination.trim(), target)
}

/// The line reporting `destination`, panicking with the whole report when
/// there is none — a missing line is otherwise indistinguishable from a wrong
/// one.
fn entry<'a>(report: &'a str, destination: &str) -> (&'a str, &'a str) {
    report
        .lines()
        .map(parse)
        .find(|(_, reported, _)| *reported == destination)
        .map(|(kind, _, target)| (kind, target))
        .unwrap_or_else(|| panic!("no line for {destination:?} in:\n{report}"))
}

fn target<'a>(report: &'a str, destination: &str) -> &'a str {
    entry(report, destination).1
}

fn kind<'a>(report: &'a str, destination: &str) -> &'a str {
    entry(report, destination).0
}

#[test]
fn a_relative_link_resolves_against_the_document() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert_eq!(kind(&report, "nested/note.md"), "local");
    assert_eq!(target(&report, "nested/note.md"), "tests/fixtures/vault/nested/note.md");
}

#[test]
fn a_fragment_travels_with_the_destination_and_does_not_stop_it_resolving() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert_eq!(target(&report, "nested/note.md#a-heading"), "tests/fixtures/vault/nested/note.md");
    assert_eq!(target(&report, "note#A Heading"), "tests/fixtures/vault/note.md");
}

#[test]
fn a_wikilink_takes_the_file_beside_the_document() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert_eq!(kind(&report, "note"), "wiki");
    assert_eq!(target(&report, "note"), "tests/fixtures/vault/note.md");
}

#[test]
fn a_wikilink_searches_the_vault_when_nothing_is_beside_it() {
    // `index.md` is a directory up, so only a vault search finds it — and the
    // vault root is found in turn by the fixture's `.obsidian/`.
    let report = resolve(&["tests/fixtures/vault/nested/note.md"]);
    assert_eq!(target(&report, "index"), "tests/fixtures/vault/index.md");
}

#[test]
fn a_climbing_relative_link_is_reported_normalized() {
    let report = resolve(&["tests/fixtures/vault/nested/note.md"]);
    assert_eq!(target(&report, "../index.md"), "tests/fixtures/vault/index.md");
}

#[test]
fn a_missing_target_is_broken() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert_eq!(target(&report, "missing"), "(broken)");
}

#[test]
fn an_ambiguous_target_names_every_candidate() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert_eq!(
        target(&report, "dup"),
        "(ambiguous: tests/fixtures/vault/a/dup.md, tests/fixtures/vault/b/dup.md)",
        "both files, in a stable order"
    );
}

#[test]
fn an_external_link_names_no_file_and_a_bare_fragment_names_this_one() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert_eq!(kind(&report, "https://example.com"), "external");
    assert_eq!(target(&report, "https://example.com"), "-");
    assert_eq!(target(&report, "#index"), "tests/fixtures/vault/index.md");
}

#[test]
fn an_explicit_root_is_searched_instead_of_the_marked_one() {
    // Pointed at `nested/`, the vault no longer contains `index.md`, so the
    // wikilink that resolved by searching now has nowhere to look.
    let report = resolve(&["--root", "tests/fixtures/vault/nested", "tests/fixtures/vault/nested/note.md"]);
    assert_eq!(target(&report, "index"), "(broken)");
    // The relative link beside it is unaffected: the root is only consulted
    // after `base_dir`.
    assert_eq!(target(&report, "../index.md"), "tests/fixtures/vault/index.md");
}

#[test]
fn every_link_is_reported_in_source_order() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    let destinations: Vec<&str> = report.lines().map(|line| parse(line).1).collect();
    assert_eq!(
        destinations,
        [
            "nested/note.md",
            "nested/note.md#a-heading",
            "note",
            "note",
            "note#A Heading",
            "missing",
            "dup",
            "https://example.com",
            "#index",
        ]
    );
}

#[test]
fn resolving_links_says_nothing_about_the_document_itself() {
    let report = resolve(&["tests/fixtures/vault/index.md"]);
    assert!(!report.contains("Index"), "the report replaces the render, it does not accompany it:\n{report}");
}

#[test]
fn a_document_with_no_links_reports_nothing_rather_than_failing() {
    let output =
        vademecum().args(["--plain", "--resolve-links", "tests/fixtures/frontmatter.md"]).output().expect("vademecum runs");
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "no links, no lines");
}

#[test]
fn the_vault_is_found_from_inside_it() {
    // `ancestors()` on a relative path ends at the empty path, which `join`
    // then resolves against the working directory — so this used to match the
    // marker at `""`, take the empty path as the root, and find nothing under
    // it. Started from the vault root, every wikilink went broken.
    let report = resolve_from("tests/fixtures/vault", &["nested/note.md"]);
    assert_eq!(target(&report, "index"), "index.md");
}

#[test]
fn the_vault_is_found_by_climbing_above_the_working_directory() {
    // The document is named relatively and the marker is a directory up, so
    // discovery has to leave the working directory to see it.
    let report = resolve_from("tests/fixtures/vault/nested", &["note.md"]);
    assert_eq!(target(&report, "index"), "../index.md");
}

#[test]
fn a_bare_fragment_wikilink_is_the_document_already_open() {
    // `[[#Heading]]` is the wikilink spelling of `[text](#heading)`; it used to
    // resolve as a target named "" and render broken.
    let report = resolve(&["tests/fixtures/vault/note.md"]);
    assert_eq!(target(&report, "#A Heading"), "tests/fixtures/vault/note.md");
}
