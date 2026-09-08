//! Selection and scrolling shared by the pager's list overlays.

use crate::ui::input::Motion;

pub fn target(motion: Motion, selected: usize, last: usize, height: usize) -> usize {
    let height = height.max(1) as isize;
    match motion {
        Motion::Line(delta) => step(selected, delta, last),
        Motion::HalfPage(delta) => step(selected, delta * (height / 2).max(1), last),
        Motion::Page(delta) => step(selected, delta * height, last),
        Motion::Top => 0,
        Motion::Bottom => last,
    }
}

pub fn step(selected: usize, delta: isize, last: usize) -> usize {
    selected.saturating_add_signed(delta).min(last)
}

pub fn scrolled(top: usize, delta: isize, last: usize, height: usize) -> usize {
    let ceiling = last.saturating_sub(height.max(1) - 1);
    top.saturating_add_signed(delta).min(ceiling)
}

pub fn revealed(top: usize, selected: usize, height: usize) -> usize {
    let height = height.max(1);
    top.min(selected).max(selected.saturating_sub(height - 1))
}

pub fn snapped(selected: usize, top: usize, last: usize, height: usize) -> usize {
    let height = height.max(1);
    selected.clamp(top, (top + height - 1).min(last))
}

pub fn picked(top: usize, row: usize, last: usize) -> Option<usize> {
    let index = top.checked_add(row)?;
    (index <= last).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_motion_cannot_walk_past_either_end() {
        assert_eq!(target(Motion::Line(-1), 0, 9, 5), 0);
        assert_eq!(target(Motion::Line(100), 0, 9, 5), 9);
        assert_eq!(target(Motion::Bottom, 0, 9, 5), 9);
        assert_eq!(target(Motion::Top, 9, 9, 5), 0);
    }

    #[test]
    fn a_half_page_is_at_least_one_row_however_short_the_box() {
        assert_eq!(target(Motion::HalfPage(1), 0, 9, 1), 1);
        assert_eq!(target(Motion::HalfPage(1), 0, 9, 0), 1);
    }

    #[test]
    fn scrolling_stops_where_the_last_row_reaches_the_bottom() {
        assert_eq!(scrolled(0, 100, 9, 5), 5, "9 - (5 - 1)");
        assert_eq!(scrolled(0, -1, 9, 5), 0);
    }

    #[test]
    fn a_list_shorter_than_the_box_does_not_scroll_at_all() {
        assert_eq!(scrolled(0, 3, 2, 10), 0);
    }

    #[test]
    fn revealing_moves_the_box_the_least_it_can() {
        assert_eq!(revealed(0, 2, 5), 0, "already visible");
        assert_eq!(revealed(0, 7, 5), 3, "scrolled down just enough");
        assert_eq!(revealed(6, 2, 5), 2, "scrolled up onto the selection");
    }

    #[test]
    fn snapping_pulls_the_selection_onto_the_visible_rows() {
        assert_eq!(snapped(0, 4, 9, 5), 4, "down to the first visible row");
        assert_eq!(snapped(9, 0, 9, 5), 4, "up to the last visible row");
        assert_eq!(snapped(9, 0, 2, 5), 2, "never past the last row");
    }

    #[test]
    fn a_row_past_the_end_of_the_list_picks_nothing() {
        assert_eq!(picked(0, 3, 9), Some(3));
        assert_eq!(picked(5, 3, 9), Some(8));
        assert_eq!(picked(5, 30, 9), None);
    }
}
