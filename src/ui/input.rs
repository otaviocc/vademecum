//! Terminal events → actions.

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Size;

use crate::ui::app::Mode;

const WHEEL_LINES: isize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Line(isize),
    HalfPage(isize),
    Page(isize),
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Move(Motion),
    Scroll(isize),
    SelectStart { column: u16, row: u16 },
    SelectExtend { column: u16, row: u16 },
    SelectEnd { column: u16, row: u16 },
    Resize(Size),
    ToggleHelp,
    Dismiss,
    SearchStart,
    SearchType(char),
    SearchErase,
    SearchConfirm,
    SearchCancel,
    SearchStep { forward: bool },
    Focus { forward: bool },
    Follow,
    OpenExternal,
    History { forward: bool },
    Yank,
    YankLink,
    Reload,
}

pub fn action(event: &Event, mode: Mode) -> Option<Action> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => key_action(*key, mode),
        Event::Mouse(mouse) if mode != Mode::Help => mouse_action(*mouse, mode),
        Event::Resize(columns, rows) => Some(Action::Resize(Size::new(*columns, *rows))),
        _ => None,
    }
}

fn key_action(key: KeyEvent, mode: Mode) -> Option<Action> {
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
        KeyCode::Tab => Some(Action::Focus { forward: true }),
        KeyCode::BackTab => Some(Action::Focus { forward: false }),
        KeyCode::Enter => Some(Action::Follow),
        KeyCode::Char('o') => Some(Action::OpenExternal),
        KeyCode::Char('y') => Some(Action::Yank),
        KeyCode::Char('Y') => Some(Action::YankLink),
        KeyCode::Char('h') | KeyCode::Backspace => Some(Action::History { forward: false }),
        KeyCode::Char('l') => Some(Action::History { forward: true }),
        KeyCode::Char('/') => Some(Action::SearchStart),
        KeyCode::Char('n') => Some(Action::SearchStep { forward: true }),
        KeyCode::Char('N') => Some(Action::SearchStep { forward: false }),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Esc => Some(Action::Dismiss),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

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

fn overlay(key: KeyEvent) -> Option<Action> {
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
        return None;
    }
    match key.code {
        KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn mouse_action(mouse: MouseEvent, mode: Mode) -> Option<Action> {
    match mouse.kind {
        MouseEventKind::ScrollDown => Some(Action::Scroll(WHEEL_LINES)),
        MouseEventKind::ScrollUp => Some(Action::Scroll(-WHEEL_LINES)),
        MouseEventKind::Down(MouseButton::Left) if mode == Mode::Browse => {
            Some(Action::SelectStart { column: mouse.column, row: mouse.row })
        }
        MouseEventKind::Drag(MouseButton::Left) if mode == Mode::Browse => {
            Some(Action::SelectExtend { column: mouse.column, row: mouse.row })
        }
        MouseEventKind::Up(MouseButton::Left) if mode == Mode::Browse => {
            Some(Action::SelectEnd { column: mouse.column, row: mouse.row })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn control(code: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL))
    }

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE })
    }

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
            (press(KeyCode::Tab), Action::Focus { forward: true }),
            (press(KeyCode::BackTab), Action::Focus { forward: false }),
            (press(KeyCode::Enter), Action::Follow),
            (press(KeyCode::Char('o')), Action::OpenExternal),
            (press(KeyCode::Char('y')), Action::Yank),
            (press(KeyCode::Char('Y')), Action::YankLink),
            (press(KeyCode::Char('h')), Action::History { forward: false }),
            (press(KeyCode::Backspace), Action::History { forward: false }),
            (press(KeyCode::Char('l')), Action::History { forward: true }),
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
        assert_eq!(browsing(&control('b')), None);
        assert_eq!(browsing(&control('q')), None);
    }

    #[test]
    fn alt_disqualifies_a_key() {
        let alt = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
        assert_eq!(browsing(&alt), None);
    }

    #[test]
    fn the_wheel_scrolls_and_the_other_buttons_do_nothing() {
        assert_eq!(browsing(&wheel(MouseEventKind::ScrollDown)), Some(Action::Scroll(WHEEL_LINES)));
        assert_eq!(browsing(&wheel(MouseEventKind::ScrollUp)), Some(Action::Scroll(-WHEEL_LINES)));
        for kind in [MouseEventKind::Down(MouseButton::Right), MouseEventKind::Down(MouseButton::Middle)] {
            assert_eq!(browsing(&wheel(kind)), None, "{kind:?}");
        }
    }

    #[test]
    fn the_left_button_carries_the_cell_it_landed_on() {
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 17,
            row: 6,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(browsing(&click), Some(Action::SelectStart { column: 17, row: 6 }));
    }

    #[test]
    fn a_click_is_a_reading_gesture_and_reaches_no_other_mode() {
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        for mode in [Mode::Search, Mode::Help] {
            assert_eq!(action(&click, mode), None, "{mode:?}");
        }
    }

    #[test]
    fn a_drag_and_a_release_carry_the_cell_too() {
        let at = |kind| Event::Mouse(MouseEvent { kind, column: 4, row: 9, modifiers: KeyModifiers::NONE });
        let drag = at(MouseEventKind::Drag(MouseButton::Left));
        let release = at(MouseEventKind::Up(MouseButton::Left));

        assert_eq!(browsing(&drag), Some(Action::SelectExtend { column: 4, row: 9 }));
        assert_eq!(browsing(&release), Some(Action::SelectEnd { column: 4, row: 9 }));
    }

    #[test]
    fn the_other_buttons_select_nothing() {
        for button in [MouseButton::Right, MouseButton::Middle] {
            for kind in [MouseEventKind::Down(button), MouseEventKind::Drag(button), MouseEventKind::Up(button)] {
                assert_eq!(browsing(&wheel(kind)), None, "{kind:?}");
            }
        }
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
        for code in [KeyCode::Char('j'), KeyCode::Char('/'), KeyCode::Down, KeyCode::Tab, KeyCode::Enter] {
            assert_eq!(action(&press(code), Mode::Help), None, "{code:?}");
        }
    }
}
