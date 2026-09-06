//! Terminal events → actions.
//!
//! Pure and stateless, in the reducer style Holodeck uses: this module knows
//! the keybinding table and nothing else. Every state change in the pager goes
//! through an `Action`, which is what makes the bindings testable without a
//! terminal.

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Size;

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
}

/// The one mapping from a terminal event to an action.
pub fn action(event: &Event) -> Option<Action> {
    match event {
        // Windows reports a Release for every Press; without the filter every
        // key would fire twice.
        Event::Key(key) if key.kind == KeyEventKind::Press => key_action(*key),
        Event::Mouse(mouse) => mouse_action(*mouse),
        Event::Resize(columns, rows) => Some(Action::Resize(Size::new(*columns, *rows))),
        _ => None,
    }
}

fn key_action(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Action::Quit),
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
        KeyCode::Char('q') => Some(Action::Quit),
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
            (press(KeyCode::Char('q')), Action::Quit),
            (control('c'), Action::Quit),
        ];
        for (event, expected) in table {
            assert_eq!(action(&event), Some(expected), "{event:?}");
        }
    }

    #[test]
    fn a_shifted_capital_still_reaches_its_binding() {
        let shifted = Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(action(&shifted), Some(Action::Move(Motion::Bottom)));
    }

    #[test]
    fn a_release_is_not_a_second_press() {
        let mut key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(action(&Event::Key(key)), None);
    }

    #[test]
    fn a_control_binding_does_not_answer_to_its_bare_letter_twice_over() {
        // Ctrl-b is not Page up: only the letters the table names are bound.
        assert_eq!(action(&control('b')), None);
        assert_eq!(action(&control('q')), None);
    }

    #[test]
    fn alt_disqualifies_a_key() {
        let alt = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
        assert_eq!(action(&alt), None);
    }

    #[test]
    fn the_wheel_scrolls_and_the_buttons_do_nothing() {
        assert_eq!(action(&wheel(MouseEventKind::ScrollDown)), Some(Action::Scroll(WHEEL_LINES)));
        assert_eq!(action(&wheel(MouseEventKind::ScrollUp)), Some(Action::Scroll(-WHEEL_LINES)));
        assert_eq!(action(&wheel(MouseEventKind::Down(MouseButton::Left))), None);
    }

    #[test]
    fn a_resize_carries_the_new_size() {
        assert_eq!(action(&Event::Resize(80, 24)), Some(Action::Resize(Size::new(80, 24))));
    }

    #[test]
    fn an_unbound_key_is_ignored_rather_than_guessed_at() {
        assert_eq!(action(&press(KeyCode::Char('z'))), None);
        assert_eq!(action(&press(KeyCode::Insert)), None);
    }
}
