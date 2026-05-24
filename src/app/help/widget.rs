use std::sync::OnceLock;

use ratatui::{
    buffer::Buffer,
    prelude::{Rect, *},
    widgets::*,
};

use crate::colors;

pub struct HelpWidget<'a> {
    state: &'a mut super::State,
    commands: &'a [crate::app::Command],
}

impl<'a> HelpWidget<'a> {
    pub fn hydrate(state: &'a mut super::State) -> Self {
        Self {
            state,
            commands: &[],
        }
    }

    pub fn commands(mut self, commands: &'a [crate::app::Command]) -> Self {
        self.commands = commands;
        self
    }

    fn walk_commands(
        command_lines: &mut Vec<Line<'_>>,
        description_lines: &mut Vec<Line<'_>>,
        commands: &[crate::app::Command],
        level: usize,
        max_command_width: &mut usize,
    ) {
        for cmd in commands {
            let mut spans = Vec::new();

            spans.push(Span::raw("   ".repeat(level)));
            spans.push(Span::raw(cmd.name).fg(crate::colors::COMMAND_ACCENT));

            if !cmd.arguments.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::raw(cmd.arguments).fg(crate::colors::TEXT_INACTIVE));
            }

            let line = Line::from(spans);
            *max_command_width = (*max_command_width).max(line.width());
            command_lines.push(line);

            description_lines.push(Line::from(Span::raw(cmd.description)));

            Self::walk_commands(
                command_lines,
                description_lines,
                &cmd.subcommands,
                level + 1,
                max_command_width,
            );
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        static WIDGET_BLOCK: OnceLock<Block> = OnceLock::new();
        WIDGET_BLOCK
            .get_or_init(|| Block::new().bg(colors::BLACK))
            .render(area, buf);

        let mut command_lines = Vec::new();
        let mut description_lines = Vec::new();
        let mut max_command_width = 0;
        Self::walk_commands(
            &mut command_lines,
            &mut description_lines,
            self.commands,
            0,
            &mut max_command_width,
        );

        let command_column = Paragraph::new(command_lines);
        let description_column = Paragraph::new(description_lines);

        let Some([command_area, description_area]) =
            crate::split::split_left(area, max_command_width as u16 + 4)
        else {
            return;
        };
        command_column
            .scroll((self.state.view_bounds().top() as u16, 0))
            .render(command_area, buf);
        description_column
            .scroll((self.state.view_bounds().top() as u16, 0))
            .render(description_area, buf);
    }
}
