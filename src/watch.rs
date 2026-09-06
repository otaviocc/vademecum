//! The file watcher behind `--watch`.
//!
//! What is watched is the open document's **directory**, never the document.
//! Editors save by renaming a new file into place, so a watch on the file
//! itself is a watch on an inode that the first save throws away; watching the
//! directory survives that, and survives a document being deleted and restored
//! too.
//!
//! Because exactly one directory is watched, and non-recursively, "is this
//! event ours?" is a comparison of file names. A watcher over a *tree* would
//! have to strip a prefix from the event path and cope with the three ways a
//! path can be spelled — as given, absolutised, canonicalised — which on macOS
//! is the difference between `/var` and `/private/var`. Comparing names alone
//! sidesteps all of it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

/// Long enough to fold an editor's write-rename-chmod burst into one event, and
/// to land past the window a non-atomic save leaves the file absent in; short
/// enough that a save still feels immediate. The crate's own default is 500ms,
/// which a reader would notice.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Why the pager is not watching.
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

/// What the watcher hands the pager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The paths one debounced burst touched.
    Wrote(Vec<PathBuf>),
    /// The watch lost its footing — an inotify queue overflow, say. What was
    /// missed is exactly what cannot be known, so this counts as a change:
    /// re-reading the file is both the cheapest answer and the correct one.
    Blind,
}

/// The directory being watched, and the file inside it that matters.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Armed {
    /// Passed to `watch`, and the same value has to be passed back to
    /// `unwatch`: macOS compares the literal string it was given.
    dir: PathBuf,
    /// Canonical, so a symlinked document is recognised under the name its
    /// target carries rather than the one the reader typed.
    name: OsString,
}

/// A debounced watch over one directory.
pub struct Watcher {
    /// Dropping it stops the watch, so it is held for the pager's lifetime.
    debouncer: Debouncer<RecommendedWatcher>,
    armed: Option<Armed>,
    /// The document `arm` was last asked for, whether or not it succeeded. It
    /// is what makes a second ask about the same document free — and, when the
    /// first one failed, what stops the pager retrying a directory it cannot
    /// watch on every single keypress and overwriting whatever the reader was
    /// being told at the time.
    attempted: Option<PathBuf>,
}

impl Watcher {
    /// Start watching nothing. `on_change` is called from the debouncer's own
    /// thread, so it does the least it can: it posts the change onto the
    /// pager's channel and lets the pager decide whether the change is its.
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

    /// Watch the directory `path` lives in. Idempotent: re-arming on the
    /// document already armed does nothing at all, and following a link within
    /// one directory only changes the name being listened for.
    pub fn arm(&mut self, path: &Path) -> Result<(), Error> {
        if self.attempted.as_deref() == Some(path) {
            return Ok(());
        }
        self.attempted = Some(path.to_path_buf());
        let Some(armed) = armed(path) else { return Ok(()) };

        // The directory is already watched, so only the name changes. Doing
        // this the other way round would drop and retake the OS watch on every
        // step through a vault, losing whatever landed in between.
        if self.armed.as_ref().is_some_and(|current| current.dir == armed.dir) {
            self.armed = Some(armed);
            return Ok(());
        }

        // Errors here are the old directory's business: it may be gone, which
        // is one of the reasons the reader navigated away from it.
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

    /// Whether this change is the open document's.
    pub fn wrote(&self, change: &Change) -> bool {
        let Some(armed) = &self.armed else { return false };
        match change {
            Change::Blind => true,
            Change::Wrote(paths) => paths.iter().any(|path| names_our_file(armed, path)),
        }
    }
}

/// The directory to watch for `path`, and the name to listen for in it.
///
/// Canonical, because a document opened through a symlink lives somewhere else:
/// watching the link's own directory would watch a directory nothing ever
/// writes to. `Document::load` keeps using the path as the reader wrote it;
/// only the watch follows the link.
fn armed(path: &Path) -> Option<Armed> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let name = resolved.file_name()?.to_os_string();
    let dir = match resolved.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some(Armed { dir, name })
}

/// Whether an event path names the armed document. One directory is watched and
/// nothing below it, so the name is the whole question — and answering it by
/// name is what makes the answer independent of how the path is spelled.
fn names_our_file(armed: &Armed, path: &Path) -> bool {
    path.file_name() == Some(armed.name.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    /// A path spelled the way the host spells one, so the Windows run tests its
    /// own separator rather than a literal.
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
        // Nothing on disk, so `canonicalize` fails and the fallback is what is
        // under test: a relative path still has to name a directory to watch.
        let armed = armed(Path::new("note.md")).expect("a bare name still names a directory");
        assert_eq!(armed.dir, PathBuf::from("."));
        assert_eq!(armed.name, OsString::from("note.md"));
    }

    /// The empty path is the one a `Document` can carry that names no file at
    /// all, and `arm` has to answer it without reaching for a `file_name` that
    /// is not there.
    #[test]
    fn a_path_that_names_no_file_arms_on_nothing() {
        assert!(armed(Path::new("")).is_none());
    }

    /// The reason `armed` canonicalises. Watching the link's own directory
    /// would watch a directory the editor never writes to, and the reader would
    /// see nothing reload, ever.
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
        // What macOS actually delivers for a watch armed on `/var/...`.
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

    /// A directory that cannot be watched is reported once, not on every wake.
    /// The pager calls `arm` after each burst of events, so an `arm` that
    /// failed and then retried would replace whatever the reader was being
    /// told, on every keypress, for as long as they stayed in that document.
    #[test]
    fn a_directory_that_cannot_be_watched_is_answered_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        let gone = dir.path().join("no-such-directory").join("note.md");

        let mut watcher = Watcher::new(|_| {}).expect("a watcher");
        assert!(watcher.arm(&gone).is_err(), "a directory that is not there cannot be watched");
        assert!(watcher.arm(&gone).is_ok(), "the same document was asked about twice and answered twice");
    }

    /// The one test that waits on a real filesystem. It writes in a loop rather
    /// than once because a backend is not always armed the instant `watch`
    /// returns — FSEvents especially — and a single write landing in that
    /// window is simply lost. The deadline is generous and a passing run never
    /// pays it.
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
