//! Link classification and resolution.
//!
//! Every link destination is sorted into one of three kinds, and a Local or
//! Wiki kind is then turned into a file on disk. Resolution is what decides
//! whether a link is followable, so it also decides whether the link renders
//! as a link or as a broken one.

use std::collections::HashMap;
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
                let candidate = normalize(&document.base_dir.join(path));
                if candidate.is_file() {
                    Ok(Target::File(candidate))
                } else {
                    Err(ResolveError::NotFound(path.display().to_string()))
                }
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

        // A hit is always right, so only a miss can be stale — and a miss is
        // exactly the note created in another window since the walk. It is
        // worth one more walk, once, and then the answer stands.
        match self.look_up(target, &self.index()) {
            Err(ResolveError::NotFound(_)) if self.stale.swap(false, Ordering::Relaxed) => self.look_up(target, &self.rebuild()),
            answer => answer,
        }
    }

    /// One lookup against one index. Ambiguity is a hit: two candidates are an
    /// answer, and they do not send vademecum looking for a third.
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
    fn rebuild(&self) -> Arc<Index> {
        let mut index = Index::new();
        walk(&self.root, &mut index);
        // The candidate list a reader is shown, and which of two files a
        // unique match is, must not depend on the order the filesystem
        // handed the directory back.
        for paths in index.values_mut() {
            paths.sort();
        }

        let index = Arc::new(index);
        *self.index.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&index));
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

/// Every `.md` under `dir`, by lowercase file stem.
///
/// Hidden directories are skipped, as the README says, and so are symlinks:
/// a vault with a link back to its own root would otherwise walk forever.
fn walk(dir: &Path, index: &mut HashMap<String, Vec<PathBuf>>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, index);
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

    /// Ambiguity is an answer. Two candidates are not a reason to go looking
    /// for a third, and they must not spend the walk either.
    #[test]
    fn ambiguity_is_an_answer_rather_than_a_reason_to_walk() {
        let root = vault(&["index.md", "a/dup.md", "b/dup.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        vault.invalidate();
        std::fs::write(path.join("a/new.md"), "# new\n").expect("a file");
        assert!(matches!(vault.resolve(&classify("dup", WIKI), &from), Err(ResolveError::Ambiguous { .. })));

        assert_eq!(
            vault.resolve(&classify("new", WIKI), &from),
            Ok(Target::File(path.join("a/new.md"))),
            "the ambiguous answer left the walk unspent"
        );
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
