//! The marks a reload leaves on the lines it changed, and the clock that cuts them.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use similar::{ChangeTag, TextDiff};

use crate::render::line::RenderedLine;

pub const PULSE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct Pulse {
    pub changed: BTreeSet<usize>,
    pub until: Instant,
}

pub fn changed(old: &str, new: &str) -> BTreeSet<usize> {
    TextDiff::from_lines(old, new)
        .iter_all_changes()
        .filter(|change| change.tag() == ChangeTag::Insert)
        .filter_map(|change| change.new_index())
        .map(|index| index + 1)
        .collect()
}

pub fn marks(changed: &BTreeSet<usize>, lines: &[RenderedLine]) -> Vec<bool> {
    let ladder: BTreeSet<usize> = lines.iter().filter(|line| !line.is_blank()).map(|line| line.source_line).collect();
    let rungs: BTreeSet<usize> = changed.iter().filter_map(|line| ladder.range(..=line).next_back().copied()).collect();
    lines.iter().map(|line| !line.is_blank() && rungs.contains(&line.source_line)).collect()
}

pub fn patience(pulse: Option<&Pulse>, now: Instant) -> Option<Duration> {
    pulse.map(|pulse| pulse.until.saturating_duration_since(now))
}

pub fn settled(pulse: Option<&Pulse>, now: Instant) -> bool {
    pulse.is_some_and(|pulse| now >= pulse.until)
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use super::*;
    use crate::render::line::StyledSpan;

    fn line(source_line: usize) -> RenderedLine {
        RenderedLine { spans: vec![StyledSpan::new("text", Style::default())], source_line, ..RenderedLine::default() }
    }

    fn armed(until: Instant) -> Pulse {
        Pulse { changed: BTreeSet::from([1]), until }
    }

    #[test]
    fn an_edit_in_place_marks_the_line_it_touched() {
        assert_eq!(changed("one\ntwo\nthree\n", "one\nTWO\nthree\n"), BTreeSet::from([2]));
    }

    #[test]
    fn an_insertion_marks_only_what_it_inserted() {
        let old = "one\ntwo\nthree\nfour\nfive\n";
        let new = "one\nINSERTED\ntwo\nthree\nfour\nfive\n";
        assert_eq!(changed(old, new), BTreeSet::from([2]));
    }

    #[test]
    fn an_unchanged_document_marks_nothing() {
        assert!(changed("one\ntwo\n", "one\ntwo\n").is_empty());
    }

    #[test]
    fn a_deletion_marks_nothing_because_there_is_nothing_left_to_mark() {
        assert!(changed("one\ntwo\nthree\n", "one\nthree\n").is_empty());
    }

    #[test]
    fn two_distant_edits_do_not_mark_the_span_between_them() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nH\n";
        assert_eq!(changed(old, new), BTreeSet::from([1, 8]));
    }

    #[test]
    fn a_changed_line_marks_every_row_of_the_block_that_contains_it() {
        let lines = [line(10), line(10), line(10), RenderedLine::blank(), line(20)];
        assert_eq!(marks(&BTreeSet::from([12]), &lines), vec![true, true, true, false, false]);
    }

    #[test]
    fn a_row_of_its_own_is_marked_alone() {
        let lines = [line(10), line(11), line(12)];
        assert_eq!(marks(&BTreeSet::from([11]), &lines), vec![false, true, false]);
    }

    #[test]
    fn a_blank_row_never_carries_a_mark() {
        let lines = [line(10), RenderedLine::blank(), line(10)];
        assert_eq!(marks(&BTreeSet::from([10]), &lines), vec![true, false, true]);
    }

    #[test]
    fn a_change_above_everything_rendered_marks_nothing() {
        let lines = [line(7), line(9)];
        assert_eq!(marks(&BTreeSet::from([3]), &lines), vec![false, false]);
    }

    #[test]
    fn nothing_changed_marks_nothing() {
        let lines = [line(1), line(2)];
        assert_eq!(marks(&BTreeSet::new(), &lines), vec![false, false]);
    }

    #[test]
    fn nothing_armed_means_no_timer_at_all() {
        assert_eq!(patience(None, Instant::now()), None);
        assert!(!settled(None, Instant::now()));
    }

    #[test]
    fn an_armed_pulse_asks_for_no_longer_than_it_has_left() {
        let now = Instant::now();
        let patience = patience(Some(&armed(now + PULSE)), now).expect("an armed pulse owns a clock");
        assert!(patience <= PULSE);
        assert!(!settled(Some(&armed(now + PULSE)), now));
    }

    #[test]
    fn a_deadline_exactly_now_is_spent() {
        let now = Instant::now();
        assert!(settled(Some(&armed(now)), now));
        assert_eq!(patience(Some(&armed(now)), now), Some(Duration::ZERO));
    }

    #[test]
    fn a_deadline_already_past_asks_for_zero_rather_than_panicking() {
        let now = Instant::now();
        assert_eq!(patience(Some(&armed(now - PULSE)), now), Some(Duration::ZERO));
        assert!(settled(Some(&armed(now - PULSE)), now));
    }
}
