//! Link classification.
//!
//! Every link destination is sorted into one of three kinds. Turning a Local
//! or Wiki kind into a file on disk is resolution, which lands with the
//! navigation milestone; this module only decides what a destination *is*.

use std::path::PathBuf;

use pulldown_cmark::LinkType;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
