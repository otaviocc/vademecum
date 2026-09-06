//! The file watcher behind `--watch`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot watch for file changes")]
    Start(#[source] notify_debouncer_mini::notify::Error),
    #[error("{path}: cannot watch the directory it lives in")]
    Arm {
        path: PathBuf,
        #[source]
        source: notify_debouncer_mini::notify::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Wrote(Vec<PathBuf>),
    Blind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Armed {
    dir: PathBuf,
    name: OsString,
}

pub struct Watcher {
    debouncer: Debouncer<RecommendedWatcher>,
    armed: Option<Armed>,
    attempted: Option<PathBuf>,
}

impl Watcher {
    pub fn new(mut on_change: impl FnMut(Change) + Send + 'static) -> Result<Self, Error> {
        let debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            on_change(match result {
                Ok(events) => Change::Wrote(events.into_iter().map(|event| event.path).collect()),
                Err(_) => Change::Blind,
            });
        })
        .map_err(Error::Start)?;
        Ok(Self { debouncer, armed: None, attempted: None })
    }

    pub fn arm(&mut self, path: &Path) -> Result<(), Error> {
        if self.attempted.as_deref() == Some(path) {
            return Ok(());
        }
        self.attempted = Some(path.to_path_buf());
        let Some(armed) = armed(path) else { return Ok(()) };

        if self.armed.as_ref().is_some_and(|current| current.dir == armed.dir) {
            self.armed = Some(armed);
            return Ok(());
        }

        if let Some(current) = self.armed.take() {
            let _ = self.debouncer.watcher().unwatch(&current.dir);
        }
        self.debouncer
            .watcher()
            .watch(&armed.dir, RecursiveMode::NonRecursive)
            .map_err(|source| Error::Arm { path: path.to_path_buf(), source })?;
        self.armed = Some(armed);
        Ok(())
    }

    pub fn wrote(&self, change: &Change) -> bool {
        let Some(armed) = &self.armed else { return false };
        match change {
            Change::Blind => true,
            Change::Wrote(paths) => paths.iter().any(|path| names_our_file(armed, path)),
        }
    }
}

fn armed(path: &Path) -> Option<Armed> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let name = resolved.file_name()?.to_os_string();
    let dir = match resolved.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some(Armed { dir, name })
}

fn names_our_file(armed: &Armed, path: &Path) -> bool {
    path.file_name() == Some(armed.name.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    fn native(components: &[&str]) -> PathBuf {
        components.iter().collect()
    }

    fn armed_on(dir: &str, name: &str) -> Armed {
        Armed { dir: PathBuf::from(dir), name: OsString::from(name) }
    }

    #[test]
    fn a_path_arms_on_the_directory_it_lives_in() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.md");
        std::fs::write(&path, "# hi\n").expect("write");

        let armed = armed(&path).expect("a file names a directory");
        assert_eq!(armed.name, OsString::from("note.md"));
        assert_eq!(armed.dir, dir.path().canonicalize().expect("canonical temp dir"));
    }

    #[test]
    fn a_bare_filename_arms_on_the_working_directory() {
        let armed = armed(Path::new("note.md")).expect("a bare name still names a directory");
        assert_eq!(armed.dir, PathBuf::from("."));
        assert_eq!(armed.name, OsString::from("note.md"));
    }

    #[test]
    fn a_path_that_names_no_file_arms_on_nothing() {
        assert!(armed(Path::new("")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_document_arms_on_the_directory_its_target_lives_in() {
        let dir = tempfile::tempdir().expect("temp dir");
        let notes = dir.path().join("notes");
        std::fs::create_dir(&notes).expect("mkdir");
        let target = notes.join("2024-01.md");
        std::fs::write(&target, "# hi\n").expect("write");
        let link = dir.path().join("latest.md");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let armed = armed(&link).expect("a symlink names a file");
        assert_eq!(armed.dir, notes.canonicalize().expect("canonical notes"));
        assert_eq!(armed.name, OsString::from("2024-01.md"), "the name events will carry is the target's");
    }

    #[test]
    fn an_event_naming_the_document_is_ours_however_the_path_is_spelled() {
        let armed = armed_on("/var/folders/x", "note.md");
        assert!(names_our_file(&armed, &native(&["/var", "folders", "x", "note.md"])));
        assert!(names_our_file(&armed, &native(&["/private", "var", "folders", "x", "note.md"])));
    }

    #[test]
    fn an_event_naming_something_else_in_the_directory_is_not_ours() {
        let armed = armed_on("/notes", "note.md");
        assert!(!names_our_file(&armed, &native(&["/notes", "other.md"])));
        assert!(!names_our_file(&armed, &native(&["/notes", "note.md.swp"])), "an editor's scratch file is not the document");
        assert!(!names_our_file(&armed, Path::new("")));
    }

    #[test]
    fn a_watcher_armed_on_nothing_owns_no_change() {
        let watcher = Watcher::new(|_| {}).expect("a watcher");
        assert!(!watcher.wrote(&Change::Blind), "there is no document for a lost watch to be about");
        assert!(!watcher.wrote(&Change::Wrote(vec![PathBuf::from("note.md")])));
    }

    #[test]
    fn a_lost_watch_counts_as_a_change_because_what_was_missed_cannot_be_known() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.md");
        std::fs::write(&path, "# hi\n").expect("write");

        let mut watcher = Watcher::new(|_| {}).expect("a watcher");
        watcher.arm(&path).expect("arm");

        assert!(watcher.wrote(&Change::Blind));
        assert!(watcher.wrote(&Change::Wrote(vec![path])));
        assert!(!watcher.wrote(&Change::Wrote(vec![dir.path().join("other.md")])));
        assert!(!watcher.wrote(&Change::Wrote(Vec::new())), "a burst that named nothing is nobody's");
    }

    #[test]
    fn re_arming_on_the_same_document_is_free_and_on_another_follows_the_reader() {
        let dir = tempfile::tempdir().expect("temp dir");
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).expect("mkdir");
        let here = dir.path().join("note.md");
        let there = nested.join("other.md");
        for path in [&here, &there] {
            std::fs::write(path, "# hi\n").expect("write");
        }

        let mut watcher = Watcher::new(|_| {}).expect("a watcher");
        watcher.arm(&here).expect("arm");
        let armed = watcher.armed.clone();
        watcher.arm(&here).expect("re-arm");
        assert_eq!(watcher.armed, armed, "re-arming on the open document changed nothing");

        watcher.arm(&there).expect("arm elsewhere");
        assert!(watcher.wrote(&Change::Wrote(vec![there])), "the watch followed the reader");
        assert!(!watcher.wrote(&Change::Wrote(vec![here])), "and stopped answering for where they were");
    }

    #[test]
    fn a_directory_that_cannot_be_watched_is_answered_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        let gone = dir.path().join("no-such-directory").join("note.md");

        let mut watcher = Watcher::new(|_| {}).expect("a watcher");
        assert!(watcher.arm(&gone).is_err(), "a directory that is not there cannot be watched");
        assert!(watcher.arm(&gone).is_ok(), "the same document was asked about twice and answered twice");
    }

    #[test]
    fn a_write_to_the_watched_file_reaches_the_pager() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.md");
        std::fs::write(&path, "# one\n").expect("write");

        let (tx, rx) = mpsc::channel();
        let mut watcher = Watcher::new(move |change| {
            let _ = tx.send(change);
        })
        .expect("a watcher");
        watcher.arm(&path).expect("arm");

        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            std::fs::write(&path, "# two\n").expect("write");
            if let Ok(change) = rx.recv_timeout(Duration::from_millis(500))
                && watcher.wrote(&change)
            {
                return;
            }
        }
        panic!("the watcher never saw the file change");
    }
}
