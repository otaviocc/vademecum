//! Writing to the terminal's clipboard.

use std::io;

use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::execute;

pub fn copy(text: &str) -> io::Result<()> {
    execute!(io::stdout(), CopyToClipboard::to_clipboard_from(text))
}
