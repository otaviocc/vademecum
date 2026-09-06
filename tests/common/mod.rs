//! What every integration test needs: the binary, run in an environment that
//! cannot leak the developer's own configuration into an assertion.

use std::process::Command;

use assert_cmd::prelude::*;

pub fn vademecum() -> Command {
    let mut command = Command::cargo_bin("vademecum").expect("the binary is built by the test harness");
    // Colors would otherwise depend on the environment the tests run in.
    command.env_remove("NO_COLOR");
    // So would the theme: a `theme.toml` in the developer's own config
    // directory would otherwise repaint every snapshot.
    command.env("XDG_CONFIG_HOME", "/nonexistent-vademecum-test-config");
    command.env("APPDATA", r"C:\nonexistent-vademecum-test-config");
    command
}

/// Run to completion and return stdout, failing loudly with stderr attached.
pub fn run(args: &[&str]) -> String {
    let output = vademecum().args(args).output().expect("vademecum runs");
    assert!(output.status.success(), "vademecum {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("output is utf-8")
}
