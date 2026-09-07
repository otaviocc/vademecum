//! Stdout mode, end to end.

mod common;

use common::{run, vademecum};

const ESC: char = '\x1b';

fn themed(theme: &str) -> String {
    run(&["--plain", "--color", "always", "--width", "80", "--theme", theme, "tests/fixtures/elements.md"])
}

#[test]
fn every_construct_renders_in_color() {
    let output = run(&["--plain", "--color", "always", "--width", "80", "tests/fixtures/elements.md"]);
    insta::assert_snapshot!("elements-handbook", output);
    assert_eq!(output, themed("handbook"), "the default is not the handbook theme");
}

#[test]
fn every_built_in_theme_renders() {
    insta::assert_snapshot!("elements-ansi", themed("ansi"));
    insta::assert_snapshot!("elements-kanagawa-dragon", themed("kanagawa-dragon"));
    insta::assert_snapshot!("elements-catppuccin-mocha", themed("catppuccin-mocha"));
    insta::assert_snapshot!("elements-catppuccin-latte", themed("catppuccin-latte"));
}

#[test]
fn a_theme_file_repaints_what_it_names_and_nothing_else() {
    let default = themed("ansi");
    let partial = run(&[
        "--plain",
        "--color",
        "always",
        "--width",
        "80",
        "--config",
        "tests/fixtures/theme.toml",
        "tests/fixtures/elements.md",
    ]);

    assert!(default.contains("\x1b[0;36;1mHeading 1\x1b[0m"), "the merge base heading is bold cyan: {default:?}");
    assert!(partial.contains("\x1b[0;38;2;255;0;0;1mHeading 1\x1b[0m"), "the heading did not turn red: {partial:?}");

    assert!(default.contains("\x1b[0;34;4mexternal link"), "the merge base link is underlined blue");
    assert!(partial.contains("\x1b[0;38;5;208mexternal link"), "the link kept its color or its underline");

    let unchanged = "\x1b[0;34;1mHeading 3\x1b[0m";
    assert!(default.contains(unchanged) && partial.contains(unchanged), "an unnamed element moved");
}

#[test]
fn themes_are_listed_without_a_document() {
    let output = run(&["--list-themes"]);
    let names: Vec<&str> = output.lines().collect();
    assert_eq!(names, ["handbook", "ansi", "kanagawa-dragon", "catppuccin-mocha", "catppuccin-latte"]);
}

#[test]
fn syntax_themes_are_listed_without_a_document() {
    let output = run(&["--list-syntax-themes"]);
    let names: Vec<&str> = output.lines().collect();
    assert_eq!(
        names,
        [
            "InspiredGitHub",
            "Solarized (dark)",
            "Solarized (light)",
            "base16-eighties.dark",
            "base16-mocha.dark",
            "base16-ocean.dark",
            "base16-ocean.light",
        ]
    );
}

#[test]
fn a_rust_fence_is_highlighted_and_an_unknown_one_is_not() {
    let output = run(&["--plain", "--color", "always", "--width", "80", "tests/fixtures/elements.md"]);

    let rust = line_containing(&output, "syntect highlighting arrives");
    let colors = foregrounds(rust);
    assert!(colors.len() > 1, "the Rust fence is one color: {rust:?}");

    let unknown = line_containing(&output, "A fence tagged with a language");
    let plain = foregrounds(unknown);
    assert!(plain.len() <= 1, "an unhighlighted fence took colors from the .tmTheme: {unknown:?}");
    assert!(colors.iter().any(|color| !plain.contains(color)), "the Rust fence has no color of its own: {rust:?}");

    let shebang = line_containing(&output, "usr/bin/env python3");
    assert!(foregrounds(shebang).len() > 1, "the shebang did not name a language: {shebang:?}");
}

fn line_containing<'a>(output: &'a str, needle: &str) -> &'a str {
    output.lines().find(|line| line.contains(needle)).unwrap_or_else(|| panic!("no line contains {needle:?}"))
}

fn foregrounds(line: &str) -> Vec<String> {
    let channels = |rest: &str| {
        rest.split(';')
            .take(3)
            .map(|channel| channel.trim_end_matches(|byte: char| !byte.is_ascii_digit()))
            .collect::<Vec<_>>()
            .join(";")
    };
    let mut found: Vec<String> = line.split("38;2;").skip(1).map(channels).collect();
    found.sort();
    found.dedup();
    found
}

#[test]
fn an_unknown_theme_is_an_error_that_names_the_alternatives() {
    let output = vademecum().args(["--theme", "nonesuch", "tests/fixtures/elements.md"]).output().expect("runs");
    assert!(!output.status.success(), "an unknown theme should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("nonesuch"), "{stderr}");
    assert!(stderr.contains("kanagawa-dragon"), "{stderr}");
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
    let output = run(&["--width", "80", "tests/fixtures/elements.md"]);
    assert!(!output.contains(ESC), "a piped run emitted colors");
}

#[test]
fn nothing_is_wider_than_the_requested_width() {
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
fn watching_stdin_is_refused_without_reading_it() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let mut child = vademecum()
        .args(["--watch", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("vademecum runs");
    let pipe = child.stdin.take().expect("stdin is piped");

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().expect("vademecum can be waited on") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("vademecum --watch - is waiting on stdin instead of refusing it");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    drop(pipe);

    let output = child.wait_with_output().expect("vademecum can be collected");
    assert!(!status.success(), "watching a pipe was accepted");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot watch stdin"), "the reason was not given: {stderr}");
    assert!(output.stdout.is_empty(), "a document was rendered anyway: {:?}", String::from_utf8_lossy(&output.stdout));
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
