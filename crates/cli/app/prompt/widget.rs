use std::sync::OnceLock;

use ratatui::{prelude::*, widgets::*};

use crate::{
    app::control::{InputMode, PromptMode},
    colors,
    cursor::{Cursor, SelectionOrigin},
};

pub struct PromptWidget<'a> {
    pub prompt: &'a mut super::State,
    pub mode: InputMode,
    pub cursor: &'a mut Option<(u16, u16)>,
}

impl PromptWidget<'_> {
    pub fn split_prompt(area: Rect) -> [Rect; 2] {
        let mut indicator_chunk = area;
        indicator_chunk.width = 1;

        let mut data_chunk = area;
        data_chunk.width -= 1;
        data_chunk.x += 1;

        [indicator_chunk, data_chunk]
    }
}

impl<'a> Widget for PromptWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let InputMode::Prompt(mode) = self.mode else {
            static WIDGET_BLOCK: OnceLock<Block> = OnceLock::new();
            WIDGET_BLOCK
                .get_or_init(|| Block::new().bg(colors::BG))
                .render(area, buf);
            return;
        };

        let [indicator_area, data_area] = Self::split_prompt(area);

        let cursor = self.prompt.cursor();
        let left = self.prompt.viewport().left();
        self.prompt.update_viewport(usize::from(area.width));
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

        // Syntax-highlight the input when editing a regex (not escaped/literal mode).
        let prompt_line = match mode {
            PromptMode::Search { escaped: false, .. } => {
                super::regex_highlight::RegexHighlighter::new(cmd_buf).highlight()
            }
            _ => Line::raw(cmd_buf),
        };
        Paragraph::new(prompt_line)
            .bg(colors::BG)
            .scroll((0, left as u16))
            .render(data_area, buf);

        match cursor {
            Cursor::Selection(start, end, _) => {
                let start = start.saturating_sub(left);
                let end = end.saturating_sub(left);
                let mut span_area = data_area;
                span_area.x += start as u16;
                span_area.width = (end - start) as u16;

                static HIGHLIGHT_BLOCK: OnceLock<Block> = OnceLock::new();
                HIGHLIGHT_BLOCK
                    .get_or_init(|| Block::new().bg(colors::COMMAND_BAR_SELECT))
                    .render(span_area, buf);
            }
            _ => {}
        }

        let i = match cursor {
            Cursor::Singleton(i)
            | Cursor::Selection(_, i, SelectionOrigin::Right)
            | Cursor::Selection(i, _, SelectionOrigin::Left) => {
                cmd_buf[..i.saturating_sub(left)].chars().count()
            }
        };
        *self.cursor = Some((data_area.x + i as u16, data_area.y));
    }
}
