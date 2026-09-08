#![cfg(unix)]
//! The pager, driven over a pipe on a real pty.

#[allow(dead_code, reason = "each integration binary uses a subset of the helpers")]
mod common;

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};

const ROWS: u16 = 24;
const COLUMNS: u16 = 80;
const LINES: usize = 400;
const DEADLINE: Duration = Duration::from_secs(20);

struct Pty {
    master: OwnedFd,
    slave: OwnedFd,
}

fn pty() -> Pty {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("a pty");
    grantpt(&master).expect("the pty is granted");
    unlockpt(&master).expect("the pty is unlocked");

    let name = ptsname(&master, Vec::new()).expect("the pty has a name");
    let path = String::from_utf8(name.into_bytes()).expect("the name is utf-8");
    let slave = OpenOptions::new().read(true).write(true).open(&path).expect("the pty opens");

    let size = Winsize { ws_row: ROWS, ws_col: COLUMNS, ws_xpixel: 0, ws_ypixel: 0 };
    tcsetwinsize(&slave, size).expect("the pty is sized");

    Pty { master, slave: slave.into() }
}

fn document() -> String {
    (1..=LINES).map(|line| format!("LINE-{line:03}\n")).collect()
}

fn drain(master: OwnedFd) -> Arc<Mutex<Vec<u8>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    std::thread::spawn(move || {
        let mut master = std::fs::File::from(master);
        let mut buffer = [0u8; 4096];
        while let Ok(read) = master.read(&mut buffer) {
            if read == 0 {
                return;
            }
            sink.lock().expect("the capture is not poisoned").extend_from_slice(&buffer[..read]);
        }
    });
    seen
}

fn wait_for(seen: &Arc<Mutex<Vec<u8>>>, fragment: &str, child: &mut Child) {
    let start = Instant::now();
    while start.elapsed() < DEADLINE {
        let capture = seen.lock().expect("the capture is not poisoned").clone();
        if String::from_utf8_lossy(&capture).contains(fragment) {
            return;
        }
        if let Some(status) = child.try_wait().expect("the child can be polled") {
            panic!("the pager exited with {status} before painting {fragment:?}: {}", String::from_utf8_lossy(&capture));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let capture = seen.lock().expect("the capture is not poisoned").clone();
    panic!("{fragment:?} never appeared: {}", String::from_utf8_lossy(&capture));
}

#[test]
fn a_document_piped_in_is_paged_and_answers_the_keyboard() {
    let Pty { master, slave } = pty();
    let keyboard = master.try_clone().expect("the pty is cloned for writing");
    let errors = slave.try_clone().expect("the pty is cloned for stderr");

    let mut child = common::vademecum()
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::from(slave))
        .stderr(Stdio::from(errors))
        .spawn()
        .expect("the pager starts");

    child.stdin.take().expect("stdin is a pipe").write_all(document().as_bytes()).expect("the document is piped in");

    let seen = drain(master);
    let mut keyboard = std::fs::File::from(keyboard);

    wait_for(&seen, "LINE-001", &mut child);

    keyboard.write_all(b"G").expect("the bottom key is sent");
    wait_for(&seen, &format!("LINE-{LINES:03}"), &mut child);

    keyboard.write_all(b"q").expect("the quit key is sent");

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("the child can be polled") {
            assert!(status.success(), "the pager exited with {status}");
            return;
        }
        assert!(start.elapsed() < DEADLINE, "the pager did not quit on `q`");
        std::thread::sleep(Duration::from_millis(25));
    }
}
