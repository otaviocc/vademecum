//! Writing to the clipboard, natively where possible and through the terminal otherwise.

use std::env;
use std::io::{self, Write};
use std::process::{Command, Stdio};

use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::execute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    MacOs,
    Unix,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Helper {
    pub program: &'static str,
    pub args: &'static [&'static str],
}

const PBCOPY: Helper = Helper { program: "pbcopy", args: &[] };
const CLIP: Helper = Helper { program: "clip.exe", args: &[] };
const WL_COPY: Helper = Helper { program: "wl-copy", args: &[] };
const XCLIP: Helper = Helper { program: "xclip", args: &["-selection", "clipboard"] };
const XSEL: Helper = Helper { program: "xsel", args: &["--clipboard", "--input"] };

pub fn copy(text: &str) -> io::Result<()> {
    let terminal = execute!(io::stdout(), CopyToClipboard::to_clipboard_from(text));
    native(text).unwrap_or(terminal)
}

fn native(text: &str) -> Option<io::Result<()>> {
    for helper in helpers(target(), set("WAYLAND_DISPLAY"), set("DISPLAY")) {
        match run(helper, text) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            outcome => return Some(outcome),
        }
    }
    None
}

fn helpers(target: Target, wayland: bool, x11: bool) -> &'static [Helper] {
    match (target, wayland, x11) {
        (Target::MacOs, _, _) => &[PBCOPY],
        (Target::Windows, _, _) => &[CLIP],
        (Target::Unix, true, true) => &[WL_COPY, XCLIP, XSEL],
        (Target::Unix, true, false) => &[WL_COPY],
        (Target::Unix, false, true) => &[XCLIP, XSEL],
        (Target::Unix, false, false) => &[],
    }
}

fn run(helper: &Helper, text: &str) -> io::Result<()> {
    let mut child = Command::new(helper.program)
        .args(helper.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let mut stdin = child.stdin.take().ok_or_else(|| io::Error::other("stdin was not piped"))?;
    let written = stdin.write_all(text.as_bytes());
    drop(stdin);
    written?;

    match child.wait()? {
        status if status.success() => Ok(()),
        status => Err(io::Error::other(format!("{} {status}", helper.program))),
    }
}

fn target() -> Target {
    if cfg!(target_os = "macos") {
        Target::MacOs
    } else if cfg!(target_os = "windows") {
        Target::Windows
    } else {
        Target::Unix
    }
}

fn set(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs(helpers: &[Helper]) -> Vec<&'static str> {
        helpers.iter().map(|helper| helper.program).collect()
    }

    #[test]
    fn macos_copies_with_pbcopy_whatever_the_display_variables_say() {
        assert_eq!(programs(helpers(Target::MacOs, false, false)), ["pbcopy"]);
        assert_eq!(programs(helpers(Target::MacOs, true, true)), ["pbcopy"]);
    }

    #[test]
    fn windows_copies_with_clip() {
        assert_eq!(programs(helpers(Target::Windows, false, false)), ["clip.exe"]);
    }

    #[test]
    fn wayland_prefers_wl_copy_and_still_falls_back_to_the_x11_tools() {
        assert_eq!(programs(helpers(Target::Unix, true, true)), ["wl-copy", "xclip", "xsel"]);
    }

    #[test]
    fn wayland_without_an_x_display_offers_only_wl_copy() {
        assert_eq!(programs(helpers(Target::Unix, true, false)), ["wl-copy"]);
    }

    #[test]
    fn x11_tries_xclip_then_xsel() {
        assert_eq!(programs(helpers(Target::Unix, false, true)), ["xclip", "xsel"]);
    }

    #[test]
    fn a_unix_host_with_no_display_has_no_helper_and_is_left_to_osc_52() {
        assert_eq!(helpers(Target::Unix, false, false), []);
    }

    #[test]
    fn the_x11_helpers_name_the_clipboard_rather_than_the_primary_selection() {
        assert_eq!(XCLIP.args, ["-selection", "clipboard"]);
        assert_eq!(XSEL.args, ["--clipboard", "--input"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_helper_that_exits_non_zero_is_a_failure() {
        let helper = Helper { program: "false", args: &[] };

        assert!(run(&helper, "text").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_helper_that_takes_the_text_and_exits_cleanly_is_a_copy() {
        let helper = Helper { program: "cat", args: &[] };

        assert!(run(&helper, "text").is_ok());
    }

    #[test]
    fn a_missing_helper_is_reported_as_not_found_so_the_next_one_is_tried() {
        let helper = Helper { program: "vademecum-no-such-clipboard-tool", args: &[] };

        assert_eq!(run(&helper, "text").unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
