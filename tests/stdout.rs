//! Stdout mode, end to end.
//!
//! The snapshot in `snapshots/` is the readable record of what every construct
//! renders to. Review its diff deliberately; never accept one blind.

use std::process::Command;

use assert_cmd::prelude::*;

/// The escape byte every `--color never` run must be free of.
const ESC: char = '\x1b';

fn vademecum() -> Command {
    let mut command = Command::cargo_bin("vademecum").expect("the binary is built by the test harness");
    // Colors would otherwise depend on the environment the tests run in.
    command.env_remove("NO_COLOR");
    command
}

fn run(args: &[&str]) -> String {
    let output = vademecum().args(args).output().expect("vademecum runs");
    assert!(output.status.success(), "vademecum {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("output is utf-8")
}

#[test]
fn every_construct_renders_in_color() {
    let output = run(&["--plain", "--color", "always", "--width", "80", "tests/fixtures/elements.md"]);
    insta::assert_snapshot!("elements-color", output);
}

#[test]
fn every_construct_renders_without_color() {
    let output = run(&["--plain", "--color", "never", "--width", "80", "tests/fixtures/elements.md"]);
    insta::assert_snapshot!("elements-plain", output);
}

#[test]
fn color_never_emits_no_escapes() {
    let output = run(&["--plain", "--color", "never", "--width", "80", "tests/fixtures/elements.md"]);
    assert!(!output.contains(ESC), "an escape survived --color never");
}

#[test]
fn no_color_in_the_environment_forces_never() {
    let output = vademecum()
        .env("NO_COLOR", "1")
        .args(["--plain", "--color", "always", "--width", "80", "tests/fixtures/elements.md"])
        .output()
        .expect("vademecum runs");
    let stdout = String::from_utf8(output.stdout).expect("output is utf-8");
    assert!(!stdout.contains(ESC), "NO_COLOR did not force colors off");
}

#[test]
fn a_pipe_gets_no_color_without_being_asked() {
    // stdout is captured, so it is not a terminal: `auto` must resolve to off.
    let output = run(&["--width", "80", "tests/fixtures/elements.md"]);
    assert!(!output.contains(ESC), "a piped run emitted colors");
}

#[test]
fn nothing_is_wider_than_the_requested_width() {
    // The narrow widths matter: a table's borders and padding have a floor
    // that a narrow width cannot pay for, so the renderer has to cut instead.
    for width in [8, 12, 20, 40, 80, 100] {
        let output = run(&["--color", "never", "--width", &width.to_string(), "tests/fixtures/elements.md"]);
        for line in output.lines() {
            let columns = unicode_width::UnicodeWidthStr::width(line);
            assert!(columns <= width, "at --width {width}, {line:?} is {columns} columns");
        }
    }
}

#[test]
fn frontmatter_is_hidden() {
    let output = run(&["--color", "never", "--width", "80", "tests/fixtures/frontmatter.md"]);
    assert!(!output.contains("tags:"), "frontmatter leaked into the output: {output}");
    assert!(!output.contains("A Document With Frontmatter"), "the title leaked into the body: {output}");
    assert!(output.contains("The Body"), "the body is missing: {output}");
}

#[test]
fn stdin_is_read_from_a_dash() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = vademecum()
        .args(["--color", "never", "--width", "80", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("vademecum runs");
    child.stdin.as_mut().expect("stdin is piped").write_all(b"# hi\n").expect("writing to stdin");

    let output = child.wait_with_output().expect("vademecum exits");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), " hi");
}

#[test]
fn a_closed_pipe_is_not_an_error() {
    use std::io::Read;
    use std::process::Stdio;

    // Big enough that the writer is still going when the reader walks away.
    let long = "A paragraph, repeated until the pipe buffer cannot hold it.\n\n".repeat(20_000);
    let document = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(document.path(), long).expect("write");

    let mut child = vademecum()
        .args(["--color", "never", "--width", "80"])
        .arg(document.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("vademecum runs");

    // Read a little, then drop the read end: this is `| less -R` and quitting.
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut head = [0u8; 64];
    let _ = stdout.read(&mut head);
    drop(stdout);

    let output = child.wait_with_output().expect("vademecum exits");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "a closed pipe should exit cleanly, got {}: {stderr}", output.status);
    assert!(stderr.is_empty(), "a closed pipe should say nothing: {stderr}");
}

#[test]
fn a_missing_file_fails_with_a_message() {
    let output = vademecum().args(["does-not-exist.md"]).output().expect("vademecum runs");
    assert!(!output.status.success(), "a missing file should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does-not-exist.md"), "the error should name the file: {stderr}");
}
