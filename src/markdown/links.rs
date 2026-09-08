//! Link classification and resolution.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use pulldown_cmark::LinkType;

use crate::document::Document;
use crate::markdown::ast::{Block, Inline, SourceBlock};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    External(Url),
    Local { path: PathBuf, fragment: Option<String> },
    Wiki { target: String, fragment: Option<String> },
}

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

fn split_fragment(destination: &str) -> (&str, Option<String>) {
    match destination.split_once('#') {
        Some((before, after)) if !after.is_empty() => (before, Some(after.to_string())),
        Some((before, _)) => (before, None),
        None => (destination, None),
    }
}

fn has_scheme(destination: &str) -> bool {
    let Some((scheme, _)) = destination.split_once(':') else { return false };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

impl LinkKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::External(_) => "external",
            Self::Local { .. } => "local",
            Self::Wiki { .. } => "wiki",
        }
    }

    pub fn fragment(&self) -> Option<&str> {
        match self {
            Self::External(_) => None,
            Self::Local { fragment, .. } | Self::Wiki { fragment, .. } => fragment.as_deref(),
        }
    }

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

    pub fn open(&mut self, document: Document) {
        self.document = document;
        self.vault.invalidate();
    }

    pub fn resolve(&self, kind: &LinkKind) -> Result<Target, ResolveError> {
        self.vault.resolve(kind, &self.document)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    External,
    SameDocument,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("{0}: not found")]
    NotFound(String),
    #[error("{target}: ambiguous ({candidates})")]
    Ambiguous { target: String, candidates: String },
}

type Index = HashMap<String, Vec<PathBuf>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Tree,
    Directory,
}

#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    scope: Scope,
    index: RwLock<Option<Arc<Index>>>,
    stale: AtomicBool,
}

impl Vault {
    pub fn discover(explicit: Option<&Path>, base_dir: &Path) -> Self {
        let (root, scope) = match explicit {
            Some(root) => (root.to_path_buf(), Scope::Tree),
            None => marked_root(base_dir),
        };
        Self { root, scope, index: RwLock::new(None), stale: AtomicBool::new(false) }
    }

    pub fn invalidate(&self) {
        self.stale.store(true, Ordering::Relaxed);
    }

    pub fn resolve(&self, kind: &LinkKind, document: &Document) -> Result<Target, ResolveError> {
        match kind {
            LinkKind::External(_) => Ok(Target::External),
            LinkKind::Local { path, .. } if path.as_os_str().is_empty() => Ok(Target::SameDocument),
            LinkKind::Local { path, .. } => {
                for candidate in local_candidates(path) {
                    let candidate = normalize(&document.base_dir.join(candidate));
                    if candidate.is_file() {
                        return Ok(Target::File(candidate));
                    }
                }
                Err(ResolveError::NotFound(path.display().to_string()))
            }
            LinkKind::Wiki { target, .. } if target.is_empty() => Ok(Target::SameDocument),
            LinkKind::Wiki { target, .. } => self.resolve_wiki(target, &document.base_dir),
        }
    }

    fn resolve_wiki(&self, target: &str, base_dir: &Path) -> Result<Target, ResolveError> {
        for relative in [target.to_string(), format!("{target}.md")] {
            let candidate = normalize(&base_dir.join(relative));
            if candidate.is_file() {
                return Ok(Target::File(candidate));
            }
        }

        match self.look_up(target, &self.index()) {
            Err(_) if self.stale.swap(false, Ordering::Relaxed) => self.look_up(target, &self.rebuild()),
            answer => answer,
        }
    }

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

    fn index(&self) -> Arc<Index> {
        if let Some(index) = self.index.read().unwrap_or_else(PoisonError::into_inner).as_ref() {
            return Arc::clone(index);
        }
        self.rebuild()
    }

    fn rebuild(&self) -> Arc<Index> {
        let mut index = Index::new();
        walk(&self.root, self.scope, &mut index, &mut HashSet::new());
        for paths in index.values_mut() {
            paths.sort();
            let mut seen = HashSet::new();
            paths.retain(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.clone())));
        }

        let index = Arc::new(index);
        *self.index.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&index));
        self.stale.store(false, Ordering::Relaxed);
        index
    }
}

fn marked_root(base_dir: &Path) -> (PathBuf, Scope) {
    let absolute = match base_dir.is_absolute() {
        true => normalize(base_dir),
        false => match std::env::current_dir() {
            Ok(working) => normalize(&working.join(base_dir)),
            Err(_) => return (base_dir.to_path_buf(), Scope::Directory),
        },
    };

    let Some(depth) = absolute.ancestors().position(|dir| dir.join(".obsidian").is_dir()) else {
        return (base_dir.to_path_buf(), Scope::Directory);
    };
    let mut root = base_dir.to_path_buf();
    for _ in 0..depth {
        root.push("..");
    }
    (normalize(&root), Scope::Tree)
}

fn local_candidates(path: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![path.to_path_buf()];
    if let Some(decoded) = percent_decode(&path.to_string_lossy())
        && decoded != path.to_string_lossy()
    {
        candidates.push(PathBuf::from(decoded));
    }
    candidates
}

pub fn decode_fragment(fragment: &str) -> String {
    percent_decode(fragment).unwrap_or_else(|| fragment.to_string())
}

fn percent_decode(destination: &str) -> Option<String> {
    let bytes = destination.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = destination.get(index + 1..index + 3)?;
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

fn walk(dir: &Path, scope: Scope, index: &mut HashMap<String, Vec<PathBuf>>, visited: &mut HashSet<PathBuf>) {
    let Ok(real) = dir.canonicalize() else { return };
    if !visited.insert(real) {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(kind) = std::fs::metadata(&path) else { continue };
        if kind.is_dir() {
            if scope == Scope::Tree {
                walk(&path, scope, index, visited);
            }
        } else if kind.is_file()
            && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            && let Some(stem) = path.file_stem()
        {
            index.entry(stem.to_string_lossy().to_lowercase()).or_default().push(normalize(&path));
        }
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
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
        let kind = classify("note", LinkType::WikiLink { has_pothole: true });
        assert_eq!(kind, LinkKind::Wiki { target: "note".into(), fragment: None });
    }

    #[test]
    fn a_wikilink_that_looks_like_a_url_is_still_a_wikilink() {
        assert_eq!(wiki("https://example.com"), LinkKind::Wiki { target: "https://example.com".into(), fragment: None });
    }

    fn vault(paths: &[&str]) -> tempfile::TempDir {
        let root = folder(paths);
        std::fs::create_dir_all(root.path().join(".obsidian")).expect("the vault marker");
        root
    }

    fn folder(paths: &[&str]) -> tempfile::TempDir {
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

    #[cfg(unix)]
    #[test]
    fn a_vault_linking_back_to_its_own_root_stops_walking() {
        let root = vault(&["index.md", "deep/a.md"]);
        std::os::unix::fs::symlink(root.path(), root.path().join("deep/loop")).expect("a symlink");

        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("a", WIKI), &from);
        assert_eq!(found, Ok(Target::File(root.path().join("deep/a.md"))));
    }

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

    #[cfg(unix)]
    #[test]
    fn two_different_files_of_one_name_are_still_ambiguous() {
        let root = vault(&["index.md", "a/dup.md", "b/dup.md"]);
        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("dup", WIKI), &from);
        assert!(matches!(found, Err(ResolveError::Ambiguous { .. })), "{found:?}");
    }

    #[cfg(unix)]
    #[test]
    fn the_name_a_shared_directory_is_shown_under_is_the_same_every_time() {
        let root = vault(&["index.md", "shared/note.md"]);
        let path = root.path();
        std::os::unix::fs::symlink(path.join("shared"), path.join("alias")).expect("a symlink");

        let from = document(path, "index.md");
        let expected = Ok(Target::File(path.join("alias/note.md")));
        for _ in 0..3 {
            assert_eq!(Vault::discover(None, &from.base_dir).resolve(&classify("note", WIKI), &from), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_symlink_is_not_indexed() {
        let root = vault(&["index.md"]);
        std::os::unix::fs::symlink(root.path().join("nowhere.md"), root.path().join("ghost.md")).expect("a symlink");

        let from = document(root.path(), "index.md");
        let found = Vault::discover(None, &from.base_dir).resolve(&classify("ghost", WIKI), &from);
        assert_eq!(found, Err(ResolveError::NotFound("ghost".into())));
    }

    #[test]
    fn a_percent_encoded_destination_finds_the_file_it_names() {
        let root = vault(&["index.md", "my note.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "my%20note.md"), Ok(Target::File(path.join("my note.md"))));
    }

    #[test]
    fn a_destination_that_is_already_a_filename_wins_over_its_decoding() {
        let root = vault(&["index.md", "50%25.md", "50%.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "50%25.md"), Ok(Target::File(path.join("50%25.md"))));
    }

    #[test]
    fn a_decoded_percent_is_the_fallback_when_the_raw_name_is_not_there() {
        let root = vault(&["index.md", "50%.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "50%25.md"), Ok(Target::File(path.join("50%.md"))));
    }

    #[test]
    fn a_malformed_escape_is_used_as_written() {
        assert_eq!(percent_decode("bad%zz.md"), None);
        assert_eq!(percent_decode("truncated%2"), None);

        let root = vault(&["index.md", "bad%zz.md"]);
        let path = root.path();
        assert_eq!(local_link(path, "index.md", "bad%zz.md"), Ok(Target::File(path.join("bad%zz.md"))));
    }

    #[test]
    fn an_escape_that_is_not_two_hex_digits_is_refused() {
        assert_eq!(percent_decode("a%+41.md"), None);
        assert_eq!(percent_decode("a%-1.md"), None);
        assert_eq!(percent_decode("a% 1.md"), None);
        assert_eq!(percent_decode("a%2f%2Fb").as_deref(), Some("a//b"));
    }

    #[test]
    fn a_fragment_is_decoded_like_the_path_beside_it() {
        assert_eq!(decode_fragment("A%20Heading"), "A Heading");
        assert_eq!(decode_fragment("a-heading"), "a-heading");
        assert_eq!(decode_fragment("bad%zz"), "bad%zz");
    }

    #[test]
    fn decoding_leaves_an_unescaped_destination_exactly_as_it_was() {
        assert_eq!(percent_decode("nested/note.md").as_deref(), Some("nested/note.md"));
        assert_eq!(percent_decode("my%20note.md").as_deref(), Some("my note.md"));
        assert_eq!(percent_decode("%E6%97%A5.md").as_deref(), Some("日.md"));
        assert_eq!(percent_decode("%FF.md"), None);
    }

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
        assert_eq!(wiki_link(path, "deep/down/here.md", "index"), Ok(Target::File(path.join("index.md"))));
    }

    #[test]
    fn outside_a_vault_a_name_beside_the_document_resolves() {
        let root = folder(&["index.md", "note.md"]);
        let path = root.path();
        assert_eq!(wiki_link(path, "index.md", "note"), Ok(Target::File(path.join("note.md"))));
    }

    #[test]
    fn outside_a_vault_a_name_is_still_matched_without_regard_to_case() {
        let root = folder(&["index.md", "Note.md"]);
        let path = root.path();
        let Ok(Target::File(found)) = wiki_link(path, "index.md", "note") else {
            panic!("[[note]] did not find Note.md beside the document");
        };
        assert_eq!(
            found.canonicalize().expect("the target exists"),
            path.join("Note.md").canonicalize().expect("the fixture exists"),
            "a case-insensitive filesystem reports the spelling that was asked for, so compare the files"
        );
    }

    #[test]
    fn outside_a_vault_a_relative_target_resolves_against_the_document() {
        let root = folder(&["index.md", "deep/down/target.md"]);
        let path = root.path();
        assert_eq!(wiki_link(path, "index.md", "deep/down/target"), Ok(Target::File(path.join("deep/down/target.md"))));
    }

    #[test]
    fn outside_a_vault_a_name_in_a_subdirectory_is_not_searched_for() {
        let root = folder(&["index.md", "deep/target.md"]);
        assert_eq!(wiki_link(root.path(), "index.md", "target"), Err(ResolveError::NotFound("target".into())));
    }

    #[test]
    fn outside_a_vault_a_subdirectory_is_never_read() {
        let root = folder(&["index.md"]);
        let unreadable = root.path().join("locked");
        std::fs::create_dir(&unreadable).expect("a directory");
        std::fs::write(unreadable.join("target.md"), "# heading\n").expect("a file");
        deny(&unreadable);

        let answer = wiki_link(root.path(), "index.md", "target");

        allow(&unreadable);
        assert_eq!(answer, Err(ResolveError::NotFound("target".into())), "the walk descended into a directory it must not read");
    }

    #[cfg(unix)]
    fn deny(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000)).expect("permissions");
    }

    #[cfg(unix)]
    fn allow(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).expect("permissions");
    }

    #[cfg(not(unix))]
    fn deny(_dir: &Path) {}

    #[cfg(not(unix))]
    fn allow(_dir: &Path) {}

    #[test]
    fn an_explicit_root_is_taken_over_the_marker() {
        let root = vault(&[".obsidian/", "index.md", "deep/here.md"]);
        let path = root.path();
        let from = document(path, "deep/here.md");

        let discovered = Vault::discover(None, &from.base_dir);
        assert!(discovered.resolve(&classify("index", WIKI), &from).is_ok(), "the marker finds it");

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
        std::fs::write(path.join("deep/c.md"), "# c\n").expect("a file");
        assert_eq!(vault.resolve(&classify("c", WIKI), &from), Err(ResolveError::NotFound("c".into())));
    }

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

    #[test]
    fn one_walk_is_owed_per_document_however_many_names_miss() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        vault.invalidate();
        assert_eq!(vault.resolve(&classify("gone", WIKI), &from), Err(ResolveError::NotFound("gone".into())));

        std::fs::write(path.join("deep/later.md"), "# later\n").expect("a file");
        assert_eq!(vault.resolve(&classify("later", WIKI), &from), Err(ResolveError::NotFound("later".into())));
    }

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

    #[test]
    fn a_walk_that_builds_the_index_settles_the_doubt_with_it() {
        let root = vault(&["index.md", "deep/a.md"]);
        let path = root.path();
        let from = document(path, "index.md");
        let vault = Vault::discover(None, &from.base_dir);

        vault.invalidate();
        assert_eq!(vault.resolve(&classify("gone", WIKI), &from), Err(ResolveError::NotFound("gone".into())));

        std::fs::write(path.join("deep/later.md"), "# later\n").expect("a file");
        assert_eq!(vault.resolve(&classify("later", WIKI), &from), Err(ResolveError::NotFound("later".into())));
    }

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
