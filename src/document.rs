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
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: String,
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
        let Some(frontmatter) = split_frontmatter(&source) else {
            return Self { path, base_dir, source, title: None, properties: Vec::new() };
        };

        let properties = properties_of(&frontmatter.block);
        let title = title_of(&frontmatter.block);

        Self { path, base_dir, source: blank_out(&source, frontmatter.end), title, properties }
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

fn properties_of(block: &str) -> Vec<Property> {
    let mut rows: Vec<Property> = Vec::new();
    let mut open: Option<(usize, usize)> = None;

    for line in block.lines() {
        let indent = line.len() - line.trim_start().len();
        let text = line.trim();
        if text.is_empty() {
            continue;
        }

        if let Some((row, parent)) = open.filter(|(_, parent)| indent > *parent) {
            let child = text.strip_prefix("- ").unwrap_or(text).trim();
            let value = &mut rows[row].value;
            if !value.is_empty() {
                value.push_str(", ");
            }
            value.push_str(child);
            open = Some((row, parent));
            continue;
        }

        open = None;
        match split_pair(text) {
            Some((key, value)) => {
                if value.is_empty() {
                    open = Some((rows.len(), indent));
                }
                rows.push(Property { key, value });
            }
            None => rows.push(Property { key: String::new(), value: text.to_string() }),
        }
    }

    rows
}

fn split_pair(text: &str) -> Option<(String, String)> {
    let cut = text.find([':', '='])?;
    let key = text[..cut].trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key.to_string(), unwrap_value(text[cut + 1..].trim())))
}

fn unwrap_value(value: &str) -> String {
    let value = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).unwrap_or(value);
    let value = value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')).unwrap_or(value);
    let value = value.strip_prefix('[').and_then(|value| value.strip_suffix(']')).unwrap_or(value);
    value.trim().to_string()
}

fn title_of(block: &str) -> Option<String> {
    properties_of(block).into_iter().find(|row| row.key == "title").map(|row| row.value).filter(|title| !title.is_empty())
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

    fn property(key: &str, value: &str) -> Property {
        Property { key: key.to_string(), value: value.to_string() }
    }

    #[test]
    fn every_property_is_kept_in_the_order_it_was_written() {
        let doc = document("---\ntitle: Notes\ncreated: 2026-09-08\n---\nbody\n");
        assert_eq!(doc.properties, [property("title", "Notes"), property("created", "2026-09-08")]);
    }

    #[test]
    fn an_inline_list_loses_its_brackets() {
        let doc = document("---\ntags: [markdown, fixtures]\n---\nbody\n");
        assert_eq!(doc.properties[0].value, "markdown, fixtures");
    }

    #[test]
    fn a_list_written_over_several_lines_folds_into_its_key() {
        let doc = document("---\ntags:\n  - inbox\n  - ideas\ncreated: today\n---\nbody\n");
        assert_eq!(doc.properties, [property("tags", "inbox, ideas"), property("created", "today")]);
    }

    #[test]
    fn a_nested_map_folds_into_its_key_rather_than_pretending_to_be_top_level() {
        let doc = document("---\nnested:\n  one: 1\n  two: 2\n---\nbody\n");
        assert_eq!(doc.properties, [property("nested", "one: 1, two: 2")]);
    }

    #[test]
    fn a_line_the_parser_does_not_understand_is_kept_verbatim() {
        let doc = document("---\nthis is not a pair\ntitle: Notes\n---\nbody\n");
        assert_eq!(doc.properties, [property("", "this is not a pair"), property("title", "Notes")]);
    }

    #[test]
    fn toml_properties_read_like_yaml_ones() {
        let doc = document("+++\ntitle = \"Notes\"\ndraft = true\n+++\nbody\n");
        assert_eq!(doc.properties, [property("title", "Notes"), property("draft", "true")]);
    }

    #[test]
    fn a_document_without_frontmatter_has_no_properties() {
        assert!(document("# Heading\n").properties.is_empty());
    }

    #[test]
    fn an_empty_block_is_no_properties_rather_than_an_empty_window() {
        let doc = document("---\n---\nbody\n");
        assert!(doc.properties.is_empty());
        assert_eq!(doc.source, "\n\nbody\n");
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
