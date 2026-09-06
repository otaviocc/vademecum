//! Terminal events → actions.
//!
//! Pure and stateless, in the reducer style Holodeck uses: this module knows
//! the keybinding table and nothing else. Every state change in the pager goes
//! through an `Action`, which is what makes the bindings testable without a
//! terminal.

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Size;

use crate::ui::app::Mode;

/// Lines a wheel notch scrolls.
const WHEEL_LINES: isize = 3;

/// How far a movement key travels. The amounts that depend on the viewport are
/// resolved by the app, which is the only thing that knows how tall it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Signed, so `j` and `k` are one variant.
    Line(isize),
    HalfPage(isize),
    Page(isize),
    Top,
    Bottom,
}

/// What an event asks the pager to do. `None` from `action` means the pager
/// does not care about the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Move(Motion),
    /// The wheel: move the viewport, the cursor following it into view.
    Scroll(isize),
    Resize(Size),
    /// `?`: open the help overlay, or close it.
    ToggleHelp,
    /// `Esc` in Browse: drop the search highlight.
    Dismiss,
    SearchStart,
    SearchType(char),
    SearchErase,
    SearchConfirm,
    SearchCancel,
    /// `n` / `N`.
    SearchStep {
        forward: bool,
    },
}

/// The one mapping from a terminal event to an action, given what keys mean
/// right now.
pub fn action(event: &Event, mode: Mode) -> Option<Action> {
    match event {
        // Windows reports a Release for every Press; without the filter every
        // key would fire twice.
        Event::Key(key) if key.kind == KeyEventKind::Press => key_action(*key, mode),
        // The overlay is modal: the document behind it must not move, or
        // closing it would put the reader somewhere they never navigated to.
        Event::Mouse(mouse) if mode != Mode::Help => mouse_action(*mouse),
        Event::Resize(columns, rows) => Some(Action::Resize(Size::new(*columns, *rows))),
        _ => None,
    }
}

fn key_action(key: KeyEvent, mode: Mode) -> Option<Action> {
    // The one binding that means the same thing everywhere, including with a
    // half-typed query on screen.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }
    match mode {
        Mode::Browse => browse(key),
        Mode::Search => typing(key),
        Mode::Help => overlay(key),
    }
}

fn browse(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('d') => Some(Action::Move(Motion::HalfPage(1))),
            KeyCode::Char('u') => Some(Action::Move(Motion::HalfPage(-1))),
            _ => None,
        };
    }
    // Shift is how `G` and `N` arrive on some terminals, so it is not
    // disqualifying; Alt and the platform key are.
    if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Move(Motion::Line(1))),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Move(Motion::Line(-1))),
        KeyCode::Char('d') => Some(Action::Move(Motion::HalfPage(1))),
        KeyCode::Char('u') => Some(Action::Move(Motion::HalfPage(-1))),
        KeyCode::Char(' ') | KeyCode::PageDown => Some(Action::Move(Motion::Page(1))),
        KeyCode::Char('b') | KeyCode::PageUp => Some(Action::Move(Motion::Page(-1))),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::Move(Motion::Top)),
        KeyCode::Char('G') | KeyCode::End => Some(Action::Move(Motion::Bottom)),
        KeyCode::Char('/') => Some(Action::SearchStart),
        KeyCode::Char('n') => Some(Action::SearchStep { forward: true }),
        KeyCode::Char('N') => Some(Action::SearchStep { forward: false }),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

/// While a query is being typed every printable key is text, so `q` does not
/// quit and `?` does not open the help.
fn typing(key: KeyEvent) -> Option<Action> {
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }
    match key.code {
        KeyCode::Char(character) => Some(Action::SearchType(character)),
        KeyCode::Backspace => Some(Action::SearchErase),
        KeyCode::Enter => Some(Action::SearchConfirm),
        KeyCode::Esc => Some(Action::SearchCancel),
        _ => None,
    }
}

/// The overlay swallows everything but the keys that close it.
fn overlay(key: KeyEvent) -> Option<Action> {
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }
    match key.code {
        KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn mouse_action(mouse: MouseEvent) -> Option<Action> {
    match mouse.kind {
        MouseEventKind::ScrollDown => Some(Action::Scroll(WHEEL_LINES)),
        MouseEventKind::ScrollUp => Some(Action::Scroll(-WHEEL_LINES)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{MouseButton, MouseEventKind};

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn control(code: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL))
    }

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE })
    }

    /// What the key does while reading, which is most of the table.
    fn browsing(event: &Event) -> Option<Action> {
        action(event, Mode::Browse)
    }

    #[test]
    fn every_documented_key_maps_to_its_action() {
        let table = [
            (press(KeyCode::Char('j')), Action::Move(Motion::Line(1))),
            (press(KeyCode::Down), Action::Move(Motion::Line(1))),
            (press(KeyCode::Char('k')), Action::Move(Motion::Line(-1))),
            (press(KeyCode::Up), Action::Move(Motion::Line(-1))),
            (press(KeyCode::Char('d')), Action::Move(Motion::HalfPage(1))),
            (control('d'), Action::Move(Motion::HalfPage(1))),
            (press(KeyCode::Char('u')), Action::Move(Motion::HalfPage(-1))),
            (control('u'), Action::Move(Motion::HalfPage(-1))),
            (press(KeyCode::Char(' ')), Action::Move(Motion::Page(1))),
            (press(KeyCode::PageDown), Action::Move(Motion::Page(1))),
            (press(KeyCode::Char('b')), Action::Move(Motion::Page(-1))),
            (press(KeyCode::PageUp), Action::Move(Motion::Page(-1))),
            (press(KeyCode::Char('g')), Action::Move(Motion::Top)),
            (press(KeyCode::Home), Action::Move(Motion::Top)),
            (press(KeyCode::Char('G')), Action::Move(Motion::Bottom)),
            (press(KeyCode::End), Action::Move(Motion::Bottom)),
            (press(KeyCode::Char('/')), Action::SearchStart),
            (press(KeyCode::Char('n')), Action::SearchStep { forward: true }),
            (press(KeyCode::Char('N')), Action::SearchStep { forward: false }),
            (press(KeyCode::Char('?')), Action::ToggleHelp),
            (press(KeyCode::Esc), Action::Dismiss),
            (press(KeyCode::Char('q')), Action::Quit),
            (control('c'), Action::Quit),
        ];
        for (event, expected) in table {
            assert_eq!(browsing(&event), Some(expected), "{event:?}");
        }
    }

    #[test]
    fn a_shifted_capital_still_reaches_its_binding() {
        let shifted = Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(browsing(&shifted), Some(Action::Move(Motion::Bottom)));
    }

    #[test]
    fn a_release_is_not_a_second_press() {
        let mut key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(browsing(&Event::Key(key)), None);
    }

    #[test]
    fn a_control_binding_does_not_answer_to_its_bare_letter_twice_over() {
        // Ctrl-b is not Page up: only the letters the table names are bound.
        assert_eq!(browsing(&control('b')), None);
        assert_eq!(browsing(&control('q')), None);
    }

    #[test]
    fn alt_disqualifies_a_key() {
        let alt = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
        assert_eq!(browsing(&alt), None);
    }

    #[test]
    fn the_wheel_scrolls_and_the_buttons_do_nothing() {
        assert_eq!(browsing(&wheel(MouseEventKind::ScrollDown)), Some(Action::Scroll(WHEEL_LINES)));
        assert_eq!(browsing(&wheel(MouseEventKind::ScrollUp)), Some(Action::Scroll(-WHEEL_LINES)));
        assert_eq!(browsing(&wheel(MouseEventKind::Down(MouseButton::Left))), None);
    }

    #[test]
    fn a_resize_reaches_the_pager_in_every_mode() {
        for mode in [Mode::Browse, Mode::Search, Mode::Help] {
            assert_eq!(action(&Event::Resize(80, 24), mode), Some(Action::Resize(Size::new(80, 24))), "{mode:?}");
        }
    }

    #[test]
    fn the_wheel_scrolls_while_reading_and_typing_but_not_behind_the_overlay() {
        for mode in [Mode::Browse, Mode::Search] {
            assert_eq!(action(&wheel(MouseEventKind::ScrollDown), mode), Some(Action::Scroll(WHEEL_LINES)), "{mode:?}");
        }
        assert_eq!(action(&wheel(MouseEventKind::ScrollDown), Mode::Help), None);
    }

    #[test]
    fn ctrl_c_quits_from_anywhere() {
        for mode in [Mode::Browse, Mode::Search, Mode::Help] {
            assert_eq!(action(&control('c'), mode), Some(Action::Quit), "{mode:?}");
        }
    }

    #[test]
    fn an_unbound_key_is_ignored_rather_than_guessed_at() {
        assert_eq!(browsing(&press(KeyCode::Char('z'))), None);
        assert_eq!(browsing(&press(KeyCode::Insert)), None);
    }

    #[test]
    fn a_printable_key_is_text_while_a_query_is_being_typed() {
        for character in ['q', 'j', '?', '/', 'G', ' '] {
            let event = press(KeyCode::Char(character));
            assert_eq!(action(&event, Mode::Search), Some(Action::SearchType(character)), "{character:?}");
        }
    }

    #[test]
    fn the_query_is_edited_confirmed_and_abandoned() {
        assert_eq!(action(&press(KeyCode::Backspace), Mode::Search), Some(Action::SearchErase));
        assert_eq!(action(&press(KeyCode::Enter), Mode::Search), Some(Action::SearchConfirm));
        assert_eq!(action(&press(KeyCode::Esc), Mode::Search), Some(Action::SearchCancel));
    }

    #[test]
    fn a_control_chord_does_not_type_its_letter_into_the_query() {
        assert_eq!(action(&control('d'), Mode::Search), None);
    }

    #[test]
    fn the_overlay_answers_only_to_the_keys_that_close_it() {
        for code in [KeyCode::Char('?'), KeyCode::Char('q'), KeyCode::Esc] {
            assert_eq!(action(&press(code), Mode::Help), Some(Action::ToggleHelp), "{code:?}");
        }
        for code in [KeyCode::Char('j'), KeyCode::Char('/'), KeyCode::Down] {
            assert_eq!(action(&press(code), Mode::Help), None, "{code:?}");
        }
    }
}
