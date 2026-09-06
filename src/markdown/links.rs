//! Link classification and resolution.
//!
//! Every link destination is sorted into one of three kinds, and a Local or
//! Wiki kind is then turned into a file on disk. Resolution is what decides
//! whether a link is followable, so it also decides whether the link renders
//! as a link or as a broken one.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use pulldown_cmark::LinkType;

use crate::document::Document;
use crate::markdown::ast::{Block, Inline, SourceBlock};

/// A link destination that leaves the machine. Kept verbatim — vademecum never
/// re-serializes a URL, so there is nothing to gain from parsing it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url(String);

impl Url {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for Url {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    /// `https://…`, `<https://…>`, `mailto:…`.
    External(Url),
    /// A path relative to the document's `base_dir`. An empty path means the
    /// current file, as in `[text](#section)`.
    Local { path: PathBuf, fragment: Option<String> },
    /// `[[target]]`, `[[target|alias]]`, `[[target#Heading]]`.
    Wiki { target: String, fragment: Option<String> },
}

/// Sort a destination into a [`LinkKind`].
pub fn classify(destination: &str, link_type: LinkType) -> LinkKind {
    if matches!(link_type, LinkType::WikiLink { .. }) {
        let (target, fragment) = split_fragment(destination);
        return LinkKind::Wiki { target: target.to_string(), fragment };
    }

    if matches!(link_type, LinkType::Autolink | LinkType::Email) || has_scheme(destination) {
        return LinkKind::External(Url::from(destination));
    }

    let (path, fragment) = split_fragment(destination);
    LinkKind::Local { path: PathBuf::from(path), fragment }
}

/// Split a destination on its first `#`.
fn split_fragment(destination: &str) -> (&str, Option<String>) {
    match destination.split_once('#') {
        Some((before, after)) if !after.is_empty() => (before, Some(after.to_string())),
        Some((before, _)) => (before, None),
        None => (destination, None),
    }
}

/// Whether the destination starts with a URL scheme (`https:`, `mailto:`).
///
/// A scheme needs at least two characters, so a Windows path like `C:\notes`
/// stays a local path.
fn has_scheme(destination: &str) -> bool {
    let Some((scheme, _)) = destination.split_once(':') else { return false };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

impl LinkKind {
    /// `external`, `local`, `wiki` — the first column of `--resolve-links`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::External(_) => "external",
            Self::Local { .. } => "local",
            Self::Wiki { .. } => "wiki",
        }
    }

    /// The `#heading` a link carries, if it carries one.
    pub fn fragment(&self) -> Option<&str> {
        match self {
            Self::External(_) => None,
            Self::Local { fragment, .. } | Self::Wiki { fragment, .. } => fragment.as_deref(),
        }
    }

    /// The link as written, fragment included.
    pub fn destination(&self) -> String {
        let with_fragment = |body: String, fragment: &Option<String>| match fragment {
            Some(fragment) => format!("{body}#{fragment}"),
            None => body,
        };
        match self {
            Self::External(url) => url.to_string(),
            Self::Local { path, fragment } => with_fragment(path.display().to_string(), fragment),
            Self::Wiki { target, fragment } => with_fragment(target.clone(), fragment),
        }
    }
}

/// Every link in a document, in source order. `--resolve-links` reports from
/// the tree rather than from the rendered lines: a narrow width truncates a
/// line and takes its link with it, and a debug flag should not depend on how
/// wide the terminal happened to be.
pub fn collect(blocks: &[SourceBlock]) -> Vec<LinkKind> {
    let mut links = Vec::new();
    for block in blocks {
        collect_block(&block.block, &mut links);
    }
    links
}

fn collect_block(block: &Block, links: &mut Vec<LinkKind>) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => collect_inlines(inlines, links),
        Block::Quote(blocks) | Block::FootnoteDef { blocks, .. } => {
            blocks.iter().for_each(|block| collect_block(&block.block, links))
        }
        Block::List { items, .. } => {
            items.iter().flat_map(|item| &item.blocks).for_each(|block| collect_block(&block.block, links))
        }
        Block::Table { header, rows, .. } => {
            header.iter().chain(rows.iter().flatten()).for_each(|cell| collect_inlines(cell, links))
        }
        Block::CodeBlock { .. } | Block::Rule | Block::Html(_) => {}
    }
}

fn collect_inlines(inlines: &[Inline], links: &mut Vec<LinkKind>) {
    for inline in inlines {
        match inline {
            Inline::Link { kind, inlines } => {
                links.push(kind.clone());
                collect_inlines(inlines, links);
            }
            Inline::Emphasis(children) | Inline::Strong(children) | Inline::Strike(children) => collect_inlines(children, links),
            _ => {}
        }
    }
}

/// Where a document's links point: the document they were written in, and the
/// vault its wikilinks are looked up in.
///
/// Navigation replaces the document and keeps the vault: following a link
/// stays inside the collection the reader started in, wherever it takes them.
#[derive(Debug)]
pub struct Links {
    pub document: Document,
    pub vault: Vault,
}

impl Links {
    pub fn new(document: Document, root: Option<&Path>) -> Self {
        let vault = Vault::discover(root, &document.base_dir);
        Self { document, vault }
    }

    /// Move to another document without rediscovering the vault: following a
    /// link stays inside the collection the reader started in.
    ///
    /// The vault's *index*, though, is owed a second look: between one document
    /// and the next — a save, above all — a note may have been created that the
    /// walk never saw.
    pub fn open(&mut self, document: Document) {
        self.document = document;
        self.vault.invalidate();
    }

    pub fn resolve(&self, kind: &LinkKind) -> Result<Target, ResolveError> {
        self.vault.resolve(kind, &self.document)
    }
}

/// What a link points at, once resolution has run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Not a file: an external URL.
    External,
    /// The document already open — `[text](#section)`.
    SameDocument,
    File(PathBuf),
}

/// Why a Local or Wiki link cannot be followed. Both render `link_broken` and
/// both are shown to the reader verbatim in the statusbar, which is why the
/// candidates are formatted here rather than at the call site.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("{0}: not found")]
    NotFound(String),
    #[error("{target}: ambiguous ({candidates})")]
    Ambiguous { target: String, candidates: String },
}

/// Every `.md` under the vault root, by lowercase file stem.
type Index = HashMap<String, Vec<PathBuf>>;

/// The collection a document's wikilinks are looked up in.
///
/// The index is built on the first wikilink that actually needs a vault search —
/// a document whose links all resolve relatively never walks the tree at all —
/// and thereafter it is doubted once per document. See `resolve_wiki`.
#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    /// `None` until something needs it. Behind an `Arc` so a lookup can read it
    /// without holding the lock, and behind a lock because `resolve` takes
    /// `&self`: layout resolves links through a shared `Ctx`.
    index: RwLock<Option<Arc<Index>>>,
    /// Whether the index is owed a second look. Set when a document opens; spent
    /// by the next wikilink that fails to resolve.
    stale: AtomicBool,
}

impl Vault {
    /// `--root` if the reader gave one, else the nearest ancestor of `base_dir`
    /// holding `.obsidian/`, else `base_dir` itself.
    pub fn discover(explicit: Option<&Path>, base_dir: &Path) -> Self {
        let root = explicit.map(Path::to_path_buf).unwrap_or_else(|| marked_root(base_dir));
        // Not stale: nothing has been built, so there is nothing to doubt.
        Self { root, index: RwLock::new(None), stale: AtomicBool::new(false) }
    }

    /// Another document is on screen, so the tree it came from may have moved
    /// too. Costs nothing on its own: the walk happens only if a wikilink then
    /// misses, and a document whose links all resolve never pays for it.
    pub fn invalidate(&self) {
        self.stale.store(true, Ordering::Relaxed);
    }

    /// The file a link points at, resolved against the document it was written
    /// in.
    pub fn resolve(&self, kind: &LinkKind, document: &Document) -> Result<Target, ResolveError> {
        match kind {
            LinkKind::External(_) => Ok(Target::External),
            // An empty path is `[text](#section)`: the document already open.
            LinkKind::Local { path, .. } if path.as_os_str().is_empty() => Ok(Target::SameDocument),
            LinkKind::Local { path, .. } => {
                // As written first, decoded second. An editor spells a space
                // `%20`, but a file may genuinely be called `50%25.md`, and
                // only trying the raw path first keeps both readable.
                for candidate in local_candidates(path) {
                    let candidate = normalize(&document.base_dir.join(candidate));
                    if candidate.is_file() {
                        return Ok(Target::File(candidate));
                    }
                }
                Err(ResolveError::NotFound(path.display().to_string()))
            }
            // `[[#Heading]]`: a fragment and nothing else, which is the
            // wikilink spelling of `[text](#heading)` and means the same.
            LinkKind::Wiki { target, .. } if target.is_empty() => Ok(Target::SameDocument),
            LinkKind::Wiki { target, .. } => self.resolve_wiki(target, &document.base_dir),
        }
    }

    /// The README's order, first hit wins: `base_dir/target`,
    /// `base_dir/target.md`, then a unique `target.md` anywhere under the root.
    fn resolve_wiki(&self, target: &str, base_dir: &Path) -> Result<Target, ResolveError> {
        for relative in [target.to_string(), format!("{target}.md")] {
            let candidate = normalize(&base_dir.join(relative));
            if candidate.is_file() {
                return Ok(Target::File(candidate));
            }
        }

        // A name that resolves is not in doubt. A name that does not — missing,
        // or ambiguous — is exactly what an edit in another window fixes, by
        // writing the note or by removing one of the two that clashed. Either
        // is worth one more walk, once, and then the answer stands.
        match self.look_up(target, &self.index()) {
            Err(_) if self.stale.swap(false, Ordering::Relaxed) => self.look_up(target, &self.rebuild()),
            answer => answer,
        }
    }

    /// One lookup against one index.
    fn look_up(&self, target: &str, index: &Index) -> Result<Target, ResolveError> {
        match index.get(&target.to_lowercase()).map(Vec::as_slice) {
            Some([only]) => Ok(Target::File(only.clone())),
            Some(candidates) if candidates.len() > 1 => Err(ResolveError::Ambiguous {
                target: target.to_string(),
                candidates: candidates.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", "),
            }),
            _ => Err(ResolveError::NotFound(target.to_string())),
        }
    }

    /// The index as it stands, walking the tree if this is the first ask.
    fn index(&self) -> Arc<Index> {
        if let Some(index) = self.index.read().unwrap_or_else(PoisonError::into_inner).as_ref() {
            return Arc::clone(index);
        }
        self.rebuild()
    }

    /// Walk the tree and take the result as the index from now on.
    ///
    /// A fresh walk owes nothing, so it settles the doubt too. Without that, a
    /// document opened before anything had needed the index would leave the
    /// doubt standing over an index built from scratch a moment later, and the
    /// first name to miss would walk the same unchanged tree twice over.
    fn rebuild(&self) -> Arc<Index> {
        let mut index = Index::new();
        walk(&self.root, &mut index, &mut HashSet::new());
        // The candidate list a reader is shown, and which of two files a
        // unique match is, must not depend on the order the filesystem
        // handed the directory back.
        for paths in index.values_mut() {
            paths.sort();
            // One file reachable under two names — through a symlinked
            // directory, or as a symlinked file — is one file, not two
            // candidates to be ambiguous between. Sorting first is what makes
            // the surviving name the same from one run to the next.
            let mut seen = HashSet::new();
            paths.retain(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.clone())));
        }

        let index = Arc::new(index);
        *self.index.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&index));
        self.stale.store(false, Ordering::Relaxed);
        index
    }
}

/// The nearest ancestor of `base_dir` holding `.obsidian/`, else `base_dir`.
///
/// The ancestors are walked on an **absolute** path. A relative `base_dir` runs
/// out at the working directory, and its last ancestor is the empty path, which
/// `join` then resolves against the working directory — so a document opened as
/// `nested/note.md` from inside its own vault matched the marker at `""`, took
/// the empty path as the root, and found nothing under it at all.
///
/// The answer is expressed back in `base_dir`'s own terms, by climbing the same
/// number of levels, so a document addressed relatively keeps reporting
/// relative paths.
fn marked_root(base_dir: &Path) -> PathBuf {
    let absolute = match base_dir.is_absolute() {
        true => normalize(base_dir),
        // No working directory to resolve against is not worth failing over:
        // the document's own directory is always a usable vault.
        false => match std::env::current_dir() {
            Ok(working) => normalize(&working.join(base_dir)),
            Err(_) => return base_dir.to_path_buf(),
        },
    };

    let Some(depth) = absolute.ancestors().position(|dir| dir.join(".obsidian").is_dir()) else {
        return base_dir.to_path_buf();
    };
    let mut root = base_dir.to_path_buf();
    for _ in 0..depth {
        root.push("..");
    }
    normalize(&root)
}

/// The paths a Local destination may name, in the order they are tried: the
/// destination as written, then its percent-decoded form when that differs.
///
/// Decoding is a fallback rather than a reinterpretation, which is what keeps a
/// file named `50%25.md` resolving while `my%20note.md` finds `my note.md`.
fn local_candidates(path: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![path.to_path_buf()];
    if let Some(decoded) = percent_decode(&path.to_string_lossy())
        && decoded != path.to_string_lossy()
    {
        candidates.push(PathBuf::from(decoded));
    }
    candidates
}

/// A `#fragment` as the heading it names, with any escapes resolved.
///
/// The editors that write `my%20note.md` write `#A%20Heading` too, and the
/// fragment is looked up by slug rather than on disk — so a raw-then-decoded
/// fallback has nothing to test the first spelling against. Decoding is
/// therefore unconditional here, and harmless: slugging drops `%` either way,
/// so a heading really containing one is unreachable by fragment regardless.
pub fn decode_fragment(fragment: &str) -> String {
    percent_decode(fragment).unwrap_or_else(|| fragment.to_string())
}

/// Percent-decoding, as CommonMark says a destination is written.
///
/// `None` when an escape is malformed — `bad%zz.md` names a file called exactly
/// that, and guessing at half of it would be worse than leaving it alone. The
/// decoded bytes must also still be UTF-8, since a path that is not cannot have
/// come from a destination that was.
fn percent_decode(destination: &str) -> Option<String> {
    let bytes = destination.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = destination.get(index + 1..index + 3)?;
            // Checked before parsing: `from_str_radix` accepts a leading sign,
            // so `%+f` would otherwise decode to a byte rather than being the
            // malformed escape it is.
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// Every `.md` under `dir`, by lowercase file stem.
///
/// Hidden directories are skipped, as the README says. Symlinked ones are
/// followed: a shared folder linked into a vault is an ordinary way to build
/// one, and the relative rule follows links already — `base_dir/target.md` goes
/// through `is_file()` — so skipping them here made the same note resolvable
/// beside a document and unresolvable through the vault.
///
/// `visited` is what makes that safe. It holds the canonical path of every
/// directory already walked, so a vault linking back to its own root stops
/// instead of walking forever, which is what skipping symlinks was really for.
fn walk(dir: &Path, index: &mut HashMap<String, Vec<PathBuf>>, visited: &mut HashSet<PathBuf>) {
    // Canonical, and recorded before descending: two names for one directory
    // are one directory, and only the resolved path can say so.
    let Ok(real) = dir.canonicalize() else { return };
    if !visited.insert(real) {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else { return };
    // Sorted, because two names for one directory are walked only once and the
    // one that wins would otherwise be whichever the filesystem happened to
    // hand back first — a different answer on a different machine.
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        // `metadata` follows the link where `file_type` does not, which is the
        // whole of the change. A broken link fails here and is skipped, which
        // is what should happen to it anyway.
        let Ok(kind) = std::fs::metadata(&path) else { continue };
        if kind.is_dir() {
            walk(&path, index, visited);
        } else if kind.is_file()
            && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            && let Some(stem) = path.file_stem()
        {
            // Normalized, so a root of `.` does not put `./` in front of every
            // path the reader is shown.
            index.entry(stem.to_string_lossy().to_lowercase()).or_default().push(normalize(&path));
        }
    }
}

/// Resolve `.` and `..` lexically, without asking the filesystem. Joining a
/// relative link onto a base directory otherwise leaves `notes/../index.md`
/// behind, which opens correctly but reads badly everywhere it is shown.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            // Only pop a name: `../..` from the root of a relative path has
            // nothing above it to discard.
            Component::ParentDir
                if normalized.components().next_back().is_some_and(|last| matches!(last, Component::Normal(_))) =>
            {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    if normalized.as_os_str().is_empty() { PathBuf::from(".") } else { normalized }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The link type `[[…]]` arrives as.
    const WIKI: LinkType = LinkType::WikiLink { has_pothole: false };

    fn inline(destination: &str) -> LinkKind {
        classify(destination, LinkType::Inline)
    }

    fn wiki(destination: &str) -> LinkKind {
        classify(destination, LinkType::WikiLink { has_pothole: false })
    }

    fn local(path: &str, fragment: Option<&str>) -> LinkKind {
        LinkKind::Local { path: PathBuf::from(path), fragment: fragment.map(str::to_string) }
    }

    #[test]
    fn schemes_are_external() {
        assert_eq!(inline("https://example.com/x"), LinkKind::External(Url::from("https://example.com/x")));
        assert_eq!(inline("mailto:otavio@example.com"), LinkKind::External(Url::from("mailto:otavio@example.com")));
        assert_eq!(inline("ftp://example.com"), LinkKind::External(Url::from("ftp://example.com")));
    }

    #[test]
    fn autolinks_and_emails_are_external_whatever_they_look_like() {
        assert!(matches!(classify("https://example.com", LinkType::Autolink), LinkKind::External(_)));
        assert!(matches!(classify("otavio@example.com", LinkType::Email), LinkKind::External(_)));
    }

    #[test]
    fn relative_paths_are_local() {
        assert_eq!(inline("other.md"), local("other.md", None));
        assert_eq!(inline("../x/y.md#sec"), local("../x/y.md", Some("sec")));
        assert_eq!(inline("nested/note.md"), local("nested/note.md", None));
    }

    #[test]
    fn a_bare_fragment_is_the_current_file() {
        assert_eq!(inline("#sec"), local("", Some("sec")));
    }

    #[test]
    fn an_empty_fragment_is_no_fragment() {
        assert_eq!(inline("other.md#"), local("other.md", None));
    }

    #[test]
    fn a_windows_drive_letter_is_not_a_scheme() {
        assert_eq!(inline(r"C:\notes\x.md"), local(r"C:\notes\x.md", None));
    }

    #[test]
    fn wikilinks_carry_target_and_fragment() {
        assert_eq!(wiki("note"), LinkKind::Wiki { target: "note".into(), fragment: None });
        assert_eq!(wiki("note#Heading"), LinkKind::Wiki { target: "note".into(), fragment: Some("Heading".into()) });
    }

    #[test]
    fn a_piped_wikilink_classifies_on_its_target() {
        // pulldown-cmark hands us the target; the alias is the link's text.
        let kind = classify("note", LinkType::WikiLink { has_pothole: true });
        assert_eq!(kind, LinkKind::Wiki { target: "note".into(), fragment: None });
    }

    #[test]
    fn a_wikilink_that_looks_like_a_url_is_still_a_wikilink() {
        assert_eq!(wiki("https://example.com"), LinkKind::Wiki { target: "https://example.com".into(), fragment: None });
    }

    /// A vault on disk: every path is created relative to a fresh temporary
    /// directory, and a path ending in `/` is a directory.
    fn vault(paths: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temporary directory");
        for path in paths {
            let full = root.path().join(path);
            match path.ends_with('/') {
                true => std::fs::create_dir_all(&full).expect("a directory"),
                false => {
                    if let Some(parent) = full.parent() {
                        std::fs::create_dir_all(parent).expect("a parent directory");
                    }
                    std::fs::write(&full, "# heading\n").expect("a file");
                }
            }
        }
        root
    }

    /// A document sitting at `path` inside the vault, with no content of its
    /// own: resolution only reads its `base_dir`.
    fn document(root: &Path, path: &str) -> Document {
        let full = root.join(path);
        let base_dir = full.parent().expect("a parent").to_path_buf();
        Document::new(Some(full), base_dir, String::new())
    }

    fn resolved(root: &Path, from: &str, destination: &str, link_type: LinkType) -> Result<Target, ResolveError> {
        let vault = Vault::discover(None, &document(root, from).base_dir);
        vault.resolve(&classify(destination, link_type), &document(root, from))
    }

    fn local_link(root: &Path, from: &str, destination: &str) -> Result<Target, ResolveError> {
        resolved(root, from, destination, LinkType::Inline)
    }

    fn wiki_link(root: &Path, from: &str, destination: &str) -> Result<Target, ResolveError> {
        resolved(root, from, destination, LinkType::WikiLink { has_pothole: false })
    }

    #[test]
    fn a_local_path_resolves_against_the_document_it_was_written_in() {
        let root = vault(&["index.md", "nested/note.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "nested/note.md"), Ok(Target::File(path.join("nested/note.md"))));
        // The fragment is not part of the file, and does not stop it resolving.
        assert_eq!(local_link(path, "index.md", "nested/note.md#a-heading"), Ok(Target::File(path.join("nested/note.md"))));
    }

    #[test]
    fn a_climbing_path_is_normalized_rather_than_left_to_read_badly() {
        let root = vault(&["index.md", "nested/note.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "nested/note.md", "../index.md"), Ok(Target::File(path.join("index.md"))));
    }

    #[test]
    fn a_missing_local_path_is_not_found() {
        let root = vault(&["index.md"]);
        assert_eq!(local_link(root.path(), "index.md", "nope.md"), Err(ResolveError::NotFound("nope.md".into())));
    }

    #[test]
    fn a_bare_fragment_is_the_document_already_open() {
        let root = vault(&["index.md"]);
        assert_eq!(local_link(root.path(), "index.md", "#section"), Ok(Target::SameDocument));
    }

    #[test]
    fn an_external_link_names_no_file() {
        let root = vault(&["index.md"]);
        assert_eq!(local_link(root.path(), "index.md", "https://example.com"), Ok(Target::External));
    }

    #[test]
    fn a_wikilink_takes_the_file_beside_the_document_first() {
        // Two `note.md`, one beside the document and one not. The relative rule
        // runs before the vault search, so there is no ambiguity to report.
        let root = vault(&["index.md", "note.md", "nested/note.md"]);
        let path = root.path();
        assert_eq!(wiki_link(path, "index.md", "note"), Ok(Target::File(path.join("note.md"))));
    }

    #[test]
    fn a_wikilink_appends_md_when_the_bare_name_is_not_a_file() {
        let root = vault(&["index.md", "note.md"]);
        let path = root.path();
        assert_eq!(wiki_link(path, "index.md", "note"), Ok(Target::File(path.join("note.md"))));
    }

    #[test]
    fn a_wikilink_searches_the_vault_when_nothing_is_beside_it() {
        let root = vault(&["index.md", "deep/down/target.md"]);
        let path = root.path();
        assert_eq!(wiki_link(path, "index.md", "target"), Ok(Target::File(path.join("deep/down/target.md"))));
    }

    #[test]
    fn the_vault_search_ignores_case() {
        // The file has to be out of the document's own directory, or a
        // case-insensitive filesystem would answer the relative rule first and
        // the index's own folding would never be exercised.
        let root = vault(&["index.md", "deep/Target.md"]);
        let path = root.path();
        assert_eq!(wiki_link(path, "index.md", "target"), Ok(Target::File(path.join("deep/Target.md"))));
    }

    #[test]
    fn two_candidates_are_ambiguous_and_both_are_named() {
        let root = vault(&["index.md", "a/dup.md", "b/dup.md"]);
        let path = root.path();
        let Err(ResolveError::Ambiguous { target, candidates }) = wiki_link(path, "index.md", "dup") else {
            panic!("two files of the same name should be ambiguous");
        };
        assert_eq!(target, "dup");
        // Sorted, so the reader is shown the same list whatever order the
        // filesystem handed the directories back in. The expectation is joined
        // a component at a time: `join("a/dup.md")` keeps the forward slash on
        // Windows, and this is a comparison of strings, not of paths.
        let expected = |dir: &str| path.join(dir).join("dup.md").display().to_string();
        assert_eq!(candidates, format!("{}, {}", expected("a"), expected("b")));
    }

    #[test]
    fn a_missing_wikilink_is_not_found() {
        let root = vault(&["index.md"]);
        assert_eq!(wiki_link(root.path(), "index.md", "nope"), Err(ResolveError::NotFound("nope".into())));
    }

    #[test]
    fn hidden_directories_are_not_searched() {
        let root = vault(&["index.md", ".trash/note.md"]);
        assert_eq!(wiki_link(root.path(), "index.md", "note"), Err(ResolveError::NotFound("note".into())));
    }

    /// A shared folder linked into a vault is an ordinary layout, and every
    /// note under it used to be invisible to wikilink resolution while the
    /// same note resolved fine when it sat beside the document.
    #[cfg(unix)]
    #[test]
    fn notes_under_a_symlinked_directory_are_found() {
        let root = vault(&["index.md"]);
        let elsewhere = vault(&["shared/note.md"]);
        std::os::unix::fs::symlink(elsewhere.path().join("shared"), root.path().join("shared")).expect("a symlink");

        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("note", WIKI), &from);
        assert_eq!(found, Ok(Target::File(root.path().join("shared/note.md"))));
    }

    /// The reason symlinks were skipped wholesale, now handled by the guard
    /// rather than by refusing to look. Without it this test does not fail —
    /// it never returns.
    #[cfg(unix)]
    #[test]
    fn a_vault_linking_back_to_its_own_root_stops_walking() {
        let root = vault(&["index.md", "deep/a.md"]);
        std::os::unix::fs::symlink(root.path(), root.path().join("deep/loop")).expect("a symlink");

        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("a", WIKI), &from);
        assert_eq!(found, Ok(Target::File(root.path().join("deep/a.md"))));
    }

    /// Following links means a note can be reached by more than one name. One
    /// file is one file: reporting it as a clash with itself would make the
    /// link unfollowable, which is worse than not following symlinks at all.
    #[cfg(unix)]
    #[test]
    fn one_file_under_two_names_is_one_candidate() {
        let root = vault(&["index.md", "shared/note.md", "other/"]);
        let path = root.path();
        std::os::unix::fs::symlink(path.join("shared/note.md"), path.join("other/note.md")).expect("a symlink");

        let from = document(path, "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("note", WIKI), &from);
        assert!(matches!(found, Ok(Target::File(_))), "one file was reported as a clash with itself: {found:?}");
    }

    /// And two genuinely different files still clash, which is the thing the
    /// deduplication must not swallow.
    #[cfg(unix)]
    #[test]
    fn two_different_files_of_one_name_are_still_ambiguous() {
        let root = vault(&["index.md", "a/dup.md", "b/dup.md"]);
        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("dup", WIKI), &from);
        assert!(matches!(found, Err(ResolveError::Ambiguous { .. })), "{found:?}");
    }

    /// A directory reachable under two names is walked once, and which name the
    /// reader is shown must not depend on the order the filesystem handed the
    /// entries back — that would be a different answer on a different machine.
    #[cfg(unix)]
    #[test]
    fn the_name_a_shared_directory_is_shown_under_is_the_same_every_time() {
        let root = vault(&["index.md", "shared/note.md"]);
        let path = root.path();
        std::os::unix::fs::symlink(path.join("shared"), path.join("alias")).expect("a symlink");

        let from = document(path, "index.md");
        // `alias` sorts before `shared`, and the walk is sorted, so this is the
        // answer on every run and every filesystem.
        let expected = Ok(Target::File(path.join("alias/note.md")));
        for _ in 0..3 {
            assert_eq!(Vault::discover(None, &from.base_dir).resolve(&classify("note", WIKI), &from), expected);
        }
    }

    /// A link pointing nowhere is not a note. `metadata` fails on it, which is
    /// exactly the behaviour wanted — the alternative is indexing a path that
    /// cannot be opened.
    #[cfg(unix)]
    #[test]
    fn a_broken_symlink_is_not_indexed() {
        let root = vault(&["index.md"]);
        std::os::unix::fs::symlink(root.path().join("nowhere.md"), root.path().join("ghost.md")).expect("a symlink");

        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("ghost", WIKI), &from);
        assert_eq!(found, Err(ResolveError::NotFound("ghost".into())));
    }

    /// What an editor writes for a filename with a space in it. Both forms a
    /// human writes already worked, which made this the wrong way round.
    #[test]
    fn a_percent_encoded_destination_finds_the_file_it_names() {
        let root = vault(&["index.md", "my note.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "my%20note.md"), Ok(Target::File(path.join("my note.md"))));
    }

    /// Decoding is a fallback, not a reinterpretation: a file really called
    /// `50%25.md` is found under the name it has.
    #[test]
    fn a_destination_that_is_already_a_filename_wins_over_its_decoding() {
        let root = vault(&["index.md", "50%25.md", "50%.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "50%25.md"), Ok(Target::File(path.join("50%25.md"))));
    }

    /// And when only the decoded spelling exists, the fallback earns its keep.
    #[test]
    fn a_decoded_percent_is_the_fallback_when_the_raw_name_is_not_there() {
        let root = vault(&["index.md", "50%.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "50%25.md"), Ok(Target::File(path.join("50%.md"))));
    }

    /// Half an escape is not an escape. Guessing at it would be worse than
    /// leaving the destination alone.
    #[test]
    fn a_malformed_escape_is_used_as_written() {
        assert_eq!(percent_decode("bad%zz.md"), None);
        assert_eq!(percent_decode("truncated%2"), None);

        let root = vault(&["index.md", "bad%zz.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "bad%zz.md"), Ok(Target::File(path.join("bad%zz.md"))));
    }

    /// `from_str_radix` accepts a leading sign, so this was decoding to a byte
    /// rather than refusing. Small, but it let a link to a file that does not
    /// exist fall back onto a bogus candidate instead of being reported broken.
    #[test]
    fn an_escape_that_is_not_two_hex_digits_is_refused() {
        assert_eq!(percent_decode("a%+41.md"), None);
        assert_eq!(percent_decode("a%-1.md"), None);
        assert_eq!(percent_decode("a% 1.md"), None);
        // And the ones that are stay accepted, in either case.
        assert_eq!(percent_decode("a%2f%2Fb").as_deref(), Some("a//b"));
    }

    /// The editors that write `my%20note.md` write `#A%20Heading` beside it.
    /// Decoding the path and not the fragment opened the right file at the
    /// wrong place, silently.
    #[test]
    fn a_fragment_is_decoded_like_the_path_beside_it() {
        assert_eq!(decode_fragment("A%20Heading"), "A Heading");
        assert_eq!(decode_fragment("a-heading"), "a-heading");
        // Malformed escapes are left alone here too.
        assert_eq!(decode_fragment("bad%zz"), "bad%zz");
    }

    #[test]
    fn decoding_leaves_an_unescaped_destination_exactly_as_it_was() {
        assert_eq!(percent_decode("nested/note.md").as_deref(), Some("nested/note.md"));
        assert_eq!(percent_decode("my%20note.md").as_deref(), Some("my note.md"));
        // Multi-byte UTF-8, one escape per byte, which is how a browser writes it.
        assert_eq!(percent_decode("%E6%97%A5.md").as_deref(), Some("日.md"));
        // Bytes that are not UTF-8 cannot have come from a destination that was.
        assert_eq!(percent_decode("%FF.md"), None);
    }

    /// A fragment is split before any decoding, so an encoded `#` stays part of
    /// the filename rather than becoming a heading reference.
    #[test]
    fn an_encoded_hash_is_part_of_the_name_rather_than_a_fragment() {
        let LinkKind::Local { path, fragment } = classify("a%23b.md", LinkType::Inline) else {
            panic!("a relative destination is Local");
        };
        assert_eq!(fragment, None, "the encoded hash did not split");
        assert_eq!(path, PathBuf::from("a%23b.md"));
        assert_eq!(local_candidates(&path).last().map(PathBuf::as_path), Some(Path::new("a#b.md")));
    }

    #[test]
    fn the_vault_root_is_the_nearest_obsidian_ancestor() {
        let root = vault(&[".obsidian/", "index.md", "deep/down/note.md", "deep/down/here.md"]);
        let path = root.path();
        // `note` is neither beside `here.md` nor below it: only a root at the
        // top of the vault finds it, which is what `.obsidian/` marks.
        assert_eq!(wiki_link(path, "deep/down/here.md", "index"), Ok(Target::File(path.join("index.md"))));
    }

    #[test]
    fn without_a_marker_the_vault_is_the_document_s_own_directory() {
        let root = vault(&["index.md", "deep/here.md"]);
        // No `.obsidian/`, so the search from `deep/` never climbs to `index.md`.
        assert_eq!(wiki_link(root.path(), "deep/here.md", "index"), Err(ResolveError::NotFound("index".into())));
    }

    #[test]
    fn an_explicit_root_is_taken_over_the_marker() {
        let root = vault(&[".obsidian/", "index.md", "deep/here.md"]);
        let path = root.path();
        let from = document(path, "deep/here.md");

        let discovered = Vault::discover(None, &from.base_dir);
        assert!(discovered.resolve(&classify("index", WIKI), &from).is_ok(), "the marker finds it");

        // Pointed at a subdirectory, the same link has nowhere to look.
        let pinned = Vault::discover(Some(&path.join("deep")), &from.base_dir);
        assert_eq!(pinned.resolve(&classify("index", WIKI), &from), Err(ResolveError::NotFound("index".into())));
    }

    #[test]
    fn the_vault_is_walked_once_however_many_links_ask_for_it() {
        let root = vault(&["index.md", "deep/a.md", "deep/b.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        assert!(vault.resolve(&classify("a", WIKI), &from).is_ok());
        // Adding a file after the index is built is not seen: the walk happened
        // once, which is the point of the cache and worth pinning down. Only
        // opening a document buys another one.
        std::fs::write(path.join("deep/c.md"), "# c\n").expect("a file");
        assert_eq!(vault.resolve(&classify("c", WIKI), &from), Err(ResolveError::NotFound("c".into())));
    }

    /// The case `--watch` exists to serve: a note written in another window
    /// while this one is being read.
    #[test]
    fn opening_a_document_lets_the_next_miss_walk_again() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        assert!(vault.resolve(&classify("a", WIKI), &from).is_ok(), "the index is built");
        std::fs::write(path.join("deep/new.md"), "# new\n").expect("a file");
        assert_eq!(vault.resolve(&classify("new", WIKI), &from), Err(ResolveError::NotFound("new".into())));

        vault.invalidate();
        assert_eq!(
            vault.resolve(&classify("new", WIKI), &from),
            Ok(Target::File(path.join("deep/new.md"))),
            "a note created since the walk is found once the document has been reopened"
        );
    }

    /// The doubt is spent by one miss, not by every lookup after it — otherwise
    /// a document with a permanently broken wikilink would walk the tree on
    /// every link it renders.
    #[test]
    fn one_walk_is_owed_per_document_however_many_names_miss() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        vault.invalidate();
        assert_eq!(vault.resolve(&classify("gone", WIKI), &from), Err(ResolveError::NotFound("gone".into())));

        // The walk that miss bought has already happened, so this one is not
        // seen until another document opens.
        std::fs::write(path.join("deep/later.md"), "# later\n").expect("a file");
        assert_eq!(vault.resolve(&classify("later", WIKI), &from), Err(ResolveError::NotFound("later".into())));
    }

    /// A name that resolves is not in doubt, so it must not spend the one walk
    /// the document is owed. If it did, the reader would lose the rebuild to
    /// whichever link happened to render first.
    #[test]
    fn a_name_that_resolves_does_not_spend_the_doubt() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        vault.invalidate();
        std::fs::write(path.join("deep/new.md"), "# new\n").expect("a file");
        assert!(vault.resolve(&classify("a", WIKI), &from).is_ok(), "a hit, which asks no questions");

        assert_eq!(
            vault.resolve(&classify("new", WIKI), &from),
            Ok(Target::File(path.join("deep/new.md"))),
            "the walk was still owed after the hit"
        );
    }

    /// An ambiguity is a broken link, and removing one of the two files is
    /// exactly how a reader fixes it. If only a missing name could spend the
    /// doubt, the clash would go on being reported — naming a file that is no
    /// longer there — until vademecum was restarted.
    #[test]
    fn resolving_an_ambiguity_elsewhere_is_seen_like_any_other_edit() {
        let root = vault(&["index.md", "a/dup.md", "b/dup.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        assert!(matches!(vault.resolve(&classify("dup", WIKI), &from), Err(ResolveError::Ambiguous { .. })));

        std::fs::remove_file(path.join("b/dup.md")).expect("one of the two");
        vault.invalidate();
        assert_eq!(vault.resolve(&classify("dup", WIKI), &from), Ok(Target::File(path.join("a/dup.md"))));
    }

    /// The doubt is one walk, however it is spent. An ambiguity that buys the
    /// walk must not leave a second one owed.
    #[test]
    fn an_ambiguity_spends_the_walk_it_bought() {
        let root = vault(&["index.md", "a/dup.md", "b/dup.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        vault.invalidate();
        assert!(matches!(vault.resolve(&classify("dup", WIKI), &from), Err(ResolveError::Ambiguous { .. })));

        std::fs::write(path.join("a/new.md"), "# new\n").expect("a file");
        assert_eq!(
            vault.resolve(&classify("new", WIKI), &from),
            Err(ResolveError::NotFound("new".into())),
            "the walk was already spent on the ambiguity"
        );
    }

    /// A walk that has just happened owes nothing. Without that, a document
    /// opened before anything needed the index left the doubt standing over an
    /// index built from scratch, and the first miss walked the tree twice.
    #[test]
    fn a_walk_that_builds_the_index_settles_the_doubt_with_it() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        // Nothing has needed the index yet, and the reader opens a document.
        vault.invalidate();
        assert_eq!(vault.resolve(&classify("gone", WIKI), &from), Err(ResolveError::NotFound("gone".into())));

        // That miss built the index, which is a walk of its own; the doubt went
        // with it rather than buying a second walk of the same tree.
        std::fs::write(path.join("deep/later.md"), "# later\n").expect("a file");
        assert_eq!(vault.resolve(&classify("later", WIKI), &from), Err(ResolveError::NotFound("later".into())));
    }

    /// The seam the pager actually goes through: `App::show` and `App::reload`
    /// both call `Links::open`, and a save is a reload.
    #[test]
    fn opening_a_document_through_links_is_what_marks_the_index_stale() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let mut links = Links::new(document(path, "index.md"), None);

        assert!(links.resolve(&classify("a", WIKI)).is_ok(), "the index is built");
        std::fs::write(path.join("deep/new.md"), "# new\n").expect("a file");
        assert_eq!(links.resolve(&classify("new", WIKI)), Err(ResolveError::NotFound("new".into())));

        links.open(document(path, "index.md"));
        assert_eq!(links.resolve(&classify("new", WIKI)), Ok(Target::File(path.join("deep/new.md"))));
    }

    #[test]
    fn collecting_finds_every_link_however_deeply_it_is_nested() {
        let source = "\
[top](a.md)

> quoted [link](b.md)

- item [link](c.md)
  - nested [link](d.md)

| head [link](e.md) |
| --- |
| cell [link](f.md) |

**[bold](g.md)** and [[wiki]]
";
        let found: Vec<String> = collect(&crate::markdown::ast::parse(source)).iter().map(LinkKind::destination).collect();
        assert_eq!(found, ["a.md", "b.md", "c.md", "d.md", "e.md", "f.md", "g.md", "wiki"]);
    }

    #[test]
    fn a_destination_is_echoed_as_it_was_written() {
        assert_eq!(inline("../x/y.md#sec").destination(), "../x/y.md#sec");
        assert_eq!(wiki("note#A Heading").destination(), "note#A Heading");
        assert_eq!(inline("https://example.com").destination(), "https://example.com");
        assert_eq!(inline("other.md").name(), "local");
        assert_eq!(wiki("note").name(), "wiki");
        assert_eq!(inline("https://example.com").name(), "external");
    }
}
