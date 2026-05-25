use std::sync::OnceLock;

use ratatui::{prelude::*, widgets::*};

use crate::{
    app::{
        command::CommandSystem,
        control::{InputMode, PromptMode},
        highlight,
    },
    colors,
    cursor::{Cursor, SelectionOrigin},
};

pub struct PromptWidget<'a> {
    pub prompt: &'a mut super::State,
    pub commands: &'a CommandSystem,
    pub mode: InputMode,
}

impl<'a> PromptWidget<'a> {
    pub fn render(self, area: Rect, f: &mut ratatui::Frame) {
        let buf = f.buffer_mut();
        let InputMode::Prompt(mode) = self.mode else {
            static WIDGET_BLOCK: OnceLock<Block> = OnceLock::new();
            WIDGET_BLOCK
                .get_or_init(|| Block::new().bg(colors::BG))
                .render(area, buf);
            return;
        };

        let Some([indicator_area, data_area]) = crate::split::split_left(area, 1) else {
            return;
        };

        let cursor = self.prompt.cursor();
        let left = self.prompt.view_bounds().left();
        self.prompt.update_view_bounds(usize::from(area.width));
        let cmd_buf = self.prompt.buf();

        match mode {
            PromptMode::Command => Span::raw(":").fg(colors::COMMAND_ACCENT),
            PromptMode::Search { .. } => Span::raw("/").fg(colors::FILTER_ACCENT),
            PromptMode::Shell { pipe: true } => Span::raw("|").fg(colors::SHELL_ACCENT),
            PromptMode::Shell { pipe: false } => Span::raw("!").fg(colors::SHELL_ACCENT),
            PromptMode::FilterColor => {
                let span = Span::raw("#");

                use std::str::FromStr;
                match ratatui::style::Color::from_str(cmd_buf) {
                    Ok(color) => span.fg(color),
                    Err(_) => span.bg(colors::ERROR),
                }
            }
        }
        .render(indicator_area, buf);

        let text_spans = match mode {
            PromptMode::Search { escaped: false, .. } => {
                highlight::RegexHighlighter::new(cmd_buf).highlight()
            }
            PromptMode::Command => {
                highlight::CommandHighlighter::new(cmd_buf).highlight(self.commands)
            }
            _ => vec![Span::raw(cmd_buf)],
        };

        let sel_spans = highlight::SelectionHighlighter::new(cmd_buf).highlight(&cursor);

        let prompt_spans = highlight::merge_spans(&text_spans, &sel_spans, Style::patch);

        Paragraph::new(Line::from(prompt_spans))
            .bg(colors::BG)
            .scroll((0, left as u16))
            .render(data_area, buf);

        let i = match cursor {
            Cursor::Singleton(i)
            | Cursor::Selection(_, i, SelectionOrigin::Right)
            | Cursor::Selection(i, _, SelectionOrigin::Left) => cmd_buf
                .chars()
                .take(i)
                .map(|c| c.len_utf8())
                .sum::<usize>(),
        };
        f.set_cursor_position((data_area.x + i as u16, data_area.y));
    }
}
