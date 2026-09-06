//! The binary, run in an environment that cannot leak local configuration.

use std::process::Command;

use assert_cmd::prelude::*;

pub fn vademecum() -> Command {
    let mut command = Command::cargo_bin("vademecum").expect("the binary is built by the test harness");
    command.env_remove("NO_COLOR");
    command.env("XDG_CONFIG_HOME", "/nonexistent-vademecum-test-config");
    command.env("APPDATA", r"C:\nonexistent-vademecum-test-config");
    command
}

pub fn run(args: &[&str]) -> String {
    let output = vademecum().args(args).output().expect("vademecum runs");
    assert!(output.status.success(), "vademecum {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("output is utf-8")
}
