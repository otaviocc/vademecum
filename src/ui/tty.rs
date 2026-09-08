//! Giving the pager a readable terminal when stdin is a pipe.

use anyhow::Result;

pub fn adopt_controlling_terminal() -> Result<()> {
    imp::adopt_controlling_terminal()
}

fn wanted(stdin_is_tty: bool, stdout_is_tty: bool) -> bool {
    !stdin_is_tty && stdout_is_tty
}

#[cfg(unix)]
mod imp {
    use std::ffi::OsString;
    use std::fs::OpenOptions;
    use std::io::{stdin, stdout};
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    use anyhow::{Context, Result};
    use rustix::termios::{isatty, ttyname};

    pub fn adopt_controlling_terminal() -> Result<()> {
        if !super::wanted(isatty(stdin()), isatty(stdout())) {
            return Ok(());
        }

        let name = ttyname(stdout(), Vec::new()).context("cannot name the terminal on stdout")?;
        let path = PathBuf::from(OsString::from_vec(name.into_bytes()));
        let terminal = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("cannot open {} for keyboard input", path.display()))?;

        rustix::stdio::dup2_stdin(&terminal).with_context(|| format!("cannot read keyboard input from {}", path.display()))
    }
}

#[cfg(not(unix))]
mod imp {
    use anyhow::Result;

    pub fn adopt_controlling_terminal() -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_piped_stdin_with_a_terminal_on_stdout_is_redirected() {
        assert!(wanted(false, true));
    }

    #[test]
    fn a_terminal_on_stdin_is_left_alone() {
        assert!(!wanted(true, true));
    }

    #[test]
    fn nothing_is_redirected_when_stdout_is_not_a_terminal() {
        assert!(!wanted(false, false));
        assert!(!wanted(true, false));
    }
}
