//! The loaded Markdown document, and the frontmatter split off its front.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub base_dir: PathBuf,
    pub source: String,
    pub title: Option<String>,
}

impl Document {
    pub fn load(path: &Path) -> Result<Self> {
        let source = std::fs::read_to_string(path).with_context(|| format!("{}: cannot read", path.display()))?;
        let base_dir = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        Ok(Self::new(Some(path.to_path_buf()), base_dir, source))
    }

    pub fn from_stdin() -> Result<Self> {
        let mut source = String::new();
        std::io::stdin().read_to_string(&mut source).context("stdin: cannot read")?;
        let base_dir = std::env::current_dir().context("cannot determine the current directory")?;
        Ok(Self::new(None, base_dir, source))
    }

    pub fn new(path: Option<PathBuf>, base_dir: PathBuf, source: String) -> Self {
        let (source, title) = match split_frontmatter(&source) {
            Some(frontmatter) => (blank_out(&source, frontmatter.end), title_of(&frontmatter.block)),
            None => (source, None),
        };
        Self { path, base_dir, source, title }
    }
}

struct Frontmatter {
    block: String,
    end: usize,
}

fn split_frontmatter(source: &str) -> Option<Frontmatter> {
    let mut lines = source.split_inclusive('\n');
    let open = lines.next()?;
    let closers: &[&str] = match open.trim_end() {
        "---" => &["---", "..."],
        "+++" => &["+++"],
        _ => return None,
    };

    let block_start = open.len();
    let mut end = open.len();
    for line in lines {
        let block_end = end;
        end += line.len();
        if closers.contains(&line.trim_end()) {
            return Some(Frontmatter { block: source[block_start..block_end].to_string(), end });
        }
    }
    None
}

fn blank_out(source: &str, end: usize) -> String {
    let mut blanked = "\n".repeat(source[..end].matches('\n').count());
    blanked.push_str(&source[end..]);
    blanked
}

fn title_of(block: &str) -> Option<String> {
    block.lines().find_map(|line| {
        let value = line.trim().strip_prefix("title")?.trim_start();
        let value = value.strip_prefix(':').or_else(|| value.strip_prefix('='))?.trim();
        let value = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).unwrap_or(value);
        let value = value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')).unwrap_or(value);
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(source: &str) -> Document {
        Document::new(None, PathBuf::from("."), source.to_string())
    }

    #[test]
    fn yaml_frontmatter_is_hidden_and_gives_the_title() {
        let doc = document("---\ntitle: Notes\ntags: [a]\n---\n# Heading\n");
        assert_eq!(doc.title.as_deref(), Some("Notes"));
        assert_eq!(doc.source, "\n\n\n\n# Heading\n");
    }

    #[test]
    fn yaml_frontmatter_may_close_with_dots() {
        let doc = document("---\ntitle: Notes\n...\nbody\n");
        assert_eq!(doc.title.as_deref(), Some("Notes"));
        assert_eq!(doc.source, "\n\n\nbody\n");
    }

    #[test]
    fn toml_frontmatter_is_hidden_and_gives_the_title() {
        let doc = document("+++\ntitle = \"Notes\"\n+++\nbody\n");
        assert_eq!(doc.title.as_deref(), Some("Notes"));
        assert_eq!(doc.source, "\n\n\nbody\n");
    }

    #[test]
    fn blanking_keeps_later_lines_on_their_own_line_numbers() {
        let source = "---\ntitle: Notes\n---\n# Heading\n";
        let doc = document(source);
        let line_of = |text: &str, haystack: &str| haystack.lines().position(|line| line == text);
        assert_eq!(line_of("# Heading", &doc.source), line_of("# Heading", source));
    }

    #[test]
    fn a_document_without_frontmatter_is_untouched() {
        let doc = document("# Heading\n\nbody\n");
        assert_eq!(doc.source, "# Heading\n\nbody\n");
        assert_eq!(doc.title, None);
    }

    #[test]
    fn a_rule_further_down_is_not_frontmatter() {
        let doc = document("# Heading\n\n---\n\nbody\n");
        assert_eq!(doc.source, "# Heading\n\n---\n\nbody\n");
    }

    #[test]
    fn an_unclosed_block_is_not_frontmatter() {
        let doc = document("---\ntitle: Notes\nbody\n");
        assert_eq!(doc.source, "---\ntitle: Notes\nbody\n");
        assert_eq!(doc.title, None);
    }

    #[test]
    fn frontmatter_without_a_title_leaves_it_unset() {
        let doc = document("---\ntags: [a]\n---\nbody\n");
        assert_eq!(doc.title, None);
    }

    #[test]
    fn quoted_and_unquoted_titles_read_the_same() {
        assert_eq!(title_of("title: Notes\n").as_deref(), Some("Notes"));
        assert_eq!(title_of("title: \"Notes\"\n").as_deref(), Some("Notes"));
        assert_eq!(title_of("title: 'Notes'\n").as_deref(), Some("Notes"));
        assert_eq!(title_of("title:\n").as_deref(), None);
        assert_eq!(title_of("titles: Notes\n").as_deref(), None);
    }

    #[test]
    fn crlf_delimiters_are_recognised() {
        let doc = document("---\r\ntitle: Notes\r\n---\r\nbody\r\n");
        assert_eq!(doc.title.as_deref(), Some("Notes"));
        assert_eq!(doc.source, "\n\n\nbody\r\n");
    }

    #[test]
    fn a_file_path_yields_its_directory_as_the_base() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.md");
        std::fs::write(&path, "# hi\n").expect("write");

        let doc = Document::load(&path).expect("load");
        assert_eq!(doc.base_dir, dir.path());
        assert_eq!(doc.path.as_deref(), Some(path.as_path()));
        assert_eq!(doc.source, "# hi\n");
    }

    #[test]
    fn a_bare_filename_bases_on_the_working_directory() {
        let doc = Document::new(Some(PathBuf::from("note.md")), PathBuf::from("."), String::new());
        assert_eq!(doc.base_dir, PathBuf::from("."));
    }

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(Document::load(Path::new("does-not-exist.md")).is_err());
    }
}
