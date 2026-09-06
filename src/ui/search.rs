//! Finding a query in the rendered text.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

fn is_case_sensitive(query: &str) -> bool {
    query.chars().any(char::is_uppercase)
}

pub fn find(haystack: &[String], query: &str) -> Vec<Match> {
    if query.is_empty() {
        return Vec::new();
    }

    let sensitive = is_case_sensitive(query);
    let needle = if sensitive { query.to_string() } else { query.to_lowercase() };

    let mut matches = Vec::new();
    for (line, text) in haystack.iter().enumerate() {
        let mut from = 0;
        while let Some((start, end)) = locate(&text[from..], &needle, sensitive) {
            matches.push(Match { line, start: from + start, end: from + end });
            from += end;
        }
    }
    matches
}

fn locate(text: &str, needle: &str, sensitive: bool) -> Option<(usize, usize)> {
    if sensitive {
        return text.find(needle).map(|start| (start, start + needle.len()));
    }
    text.char_indices().find_map(|(start, _)| starts_with(&text[start..], needle).map(|len| (start, start + len)))
}

fn starts_with(text: &str, needle: &str) -> Option<usize> {
    let mut wanted = needle.chars();
    let mut consumed = 0;
    for character in text.chars() {
        for lowered in character.to_lowercase() {
            if wanted.next()? != lowered {
                return None;
            }
        }
        consumed += character.len_utf8();
        if wanted.clone().next().is_none() {
            return Some(consumed);
        }
    }
    wanted.next().is_none().then_some(consumed)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Search {
    pub query: String,
    pub input: String,
    pub matches: Vec<Match>,
    pub current: Option<usize>,
}

impl Search {
    pub fn on_line(&self, line: usize) -> &[Match] {
        let start = self.matches.partition_point(|found| found.line < line);
        let end = self.matches.partition_point(|found| found.line <= line);
        &self.matches[start..end]
    }

    pub fn step(&mut self, forward: bool) -> Option<Match> {
        if self.matches.is_empty() {
            return None;
        }
        let last = self.matches.len() - 1;
        self.current = Some(match (self.current, forward) {
            (Some(index), true) if index == last => 0,
            (Some(index), true) => index + 1,
            (Some(0), false) => last,
            (Some(index), false) => index - 1,
            (None, true) => 0,
            (None, false) => last,
        });
        self.current.map(|index| self.matches[index])
    }

    pub fn seek_from(&mut self, line: usize) -> Option<Match> {
        let index =
            self.matches.iter().position(|found| found.line >= line).or_else(|| (!self.matches.is_empty()).then_some(0))?;
        self.current = Some(index);
        Some(self.matches[index])
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.input.clear();
        self.matches.clear();
        self.current = None;
    }

    pub fn progress(&self) -> Option<(usize, usize)> {
        let current = self.current?;
        (!self.query.is_empty()).then_some((current + 1, self.matches.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn haystack(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_string()).collect()
    }

    #[test]
    fn a_lowercase_query_ignores_case_and_a_capital_insists_on_it() {
        let lines = haystack(&["a TODO here", "a todo there"]);
        assert_eq!(find(&lines, "todo").len(), 2);

        let found = find(&lines, "TODO");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 0);
    }

    #[test]
    fn a_match_is_the_byte_range_of_the_text_as_written() {
        let lines = haystack(&["a TODO here"]);
        let found = find(&lines, "todo");
        assert_eq!((found[0].start, found[0].end), (2, 6));
        assert_eq!(&lines[0][found[0].start..found[0].end], "TODO");
    }

    #[test]
    fn every_occurrence_on_a_line_is_found() {
        let found = find(&haystack(&["ab ab ab"]), "ab");
        assert_eq!(found.len(), 3);
        assert_eq!(found.iter().map(|m| m.start).collect::<Vec<_>>(), vec![0, 3, 6]);
    }

    #[test]
    fn matches_do_not_overlap_themselves() {
        assert_eq!(find(&haystack(&["aaaa"]), "aa").len(), 2);
    }

    #[test]
    fn an_empty_query_matches_nothing_rather_than_everything() {
        assert!(find(&haystack(&["anything"]), "").is_empty());
    }

    #[test]
    fn a_query_wider_than_one_column_is_still_a_byte_range() {
        let lines = haystack(&["日本語のテキスト"]);
        let found = find(&lines, "テキスト");
        assert_eq!(found.len(), 1);
        assert_eq!(&lines[0][found[0].start..found[0].end], "テキスト");
    }

    #[test]
    fn matches_arrive_in_document_order() {
        let found = find(&haystack(&["x", "hit", "x", "hit"]), "hit");
        assert_eq!(found.iter().map(|m| m.line).collect::<Vec<_>>(), vec![1, 3]);
    }

    fn search(lines: &[&str], query: &str) -> Search {
        let matches = find(&haystack(lines), query);
        Search { query: query.to_string(), input: String::new(), matches, current: None }
    }

    #[test]
    fn a_line_reports_exactly_its_own_matches() {
        let search = search(&["hit hit", "none", "hit"], "hit");
        assert_eq!(search.on_line(0).len(), 2);
        assert!(search.on_line(1).is_empty());
        assert_eq!(search.on_line(2).len(), 1);
        assert!(search.on_line(99).is_empty());
    }

    #[test]
    fn stepping_forward_wraps_past_the_last_match() {
        let mut search = search(&["hit", "hit"], "hit");
        assert_eq!(search.step(true).map(|m| m.line), Some(0));
        assert_eq!(search.step(true).map(|m| m.line), Some(1));
        assert_eq!(search.step(true).map(|m| m.line), Some(0));
    }

    #[test]
    fn stepping_back_wraps_past_the_first() {
        let mut search = search(&["hit", "hit"], "hit");
        assert_eq!(search.step(false).map(|m| m.line), Some(1));
        assert_eq!(search.step(false).map(|m| m.line), Some(0));
        assert_eq!(search.step(false).map(|m| m.line), Some(1));
    }

    #[test]
    fn stepping_through_nothing_finds_nothing() {
        let mut search = search(&["none"], "hit");
        assert_eq!(search.step(true), None);
        assert_eq!(search.current, None);
    }

    #[test]
    fn seeking_takes_the_first_match_at_or_after_a_line_and_wraps() {
        let mut search = search(&["hit", "x", "hit", "x"], "hit");
        assert_eq!(search.seek_from(1).map(|m| m.line), Some(2));
        assert_eq!(search.seek_from(2).map(|m| m.line), Some(2));
        assert_eq!(search.seek_from(3).map(|m| m.line), Some(0), "nothing below, so back to the top");
    }

    #[test]
    fn progress_counts_from_one_and_is_silent_until_there_is_a_match_to_be_on() {
        let mut search = search(&["hit", "hit"], "hit");
        assert_eq!(search.progress(), None, "nothing to count until a match is current");

        search.step(true);
        assert_eq!(search.progress(), Some((1, 2)));
        search.step(true);
        assert_eq!(search.progress(), Some((2, 2)));

        search.clear();
        assert_eq!(search.progress(), None);
    }

    #[test]
    fn clearing_forgets_the_query_and_the_highlight() {
        let mut search = search(&["hit"], "hit");
        search.step(true);
        search.clear();
        assert!(search.query.is_empty());
        assert!(search.matches.is_empty());
        assert_eq!(search.current, None);
    }
}
