use crate::colors;
use crate::cursor::Cursor;

use ratatui::prelude::*;

pub struct SelectionHighlighter<'a> {
    base: super::Highlighter<'a>,
}

impl<'a> SelectionHighlighter<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            base: super::Highlighter::new(input),
        }
    }

    pub fn highlight(mut self, cursor: &Cursor) -> Vec<Span<'a>> {
        match cursor {
            Cursor::Selection(start, end, _) => {
                self.base.eat_and_color(*start);
                self.base
                    .eat_and_color(end - start)
                    .bg(colors::COMMAND_BAR_SELECT);
            }
            _ => {}
        }

        self.base.eat_remaining_and_color();

        self.base.extract()
    }
}
