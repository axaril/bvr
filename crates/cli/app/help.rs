use std::sync::OnceLock;

use ratatui::{prelude::*, widgets::{Block, Paragraph}};

use crate::{app::control::ViewDelta, colors, direction::Direction};

pub struct HelpManual {
    top: usize,
    height: usize,
    command_column: Paragraph<'static>,
    description_column: Paragraph<'static>,
    max_command_width: usize,
}

impl HelpManual {
    fn walk_commands(
        command_lines: &mut Vec<Line<'static>>,
        description_lines: &mut Vec<Line<'static>>,
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

    pub fn new() -> Self {
        Self {
            top: 0,
            height: 0,
            command_column: Paragraph::new(vec![]),
            description_column: Paragraph::new(vec![]),
            max_command_width: 0,
        }
    }

    pub fn generate(commands: &[crate::app::Command]) -> Self {
        let mut command_lines = Vec::new();
        let mut description_lines = Vec::new();
        let mut max_command_width = 0;
        Self::walk_commands(&mut command_lines, &mut description_lines, commands, 0, &mut max_command_width);

        let command_column =
            Paragraph::new(command_lines).wrap(ratatui::widgets::Wrap { trim: false });
        let description_column =
            Paragraph::new(description_lines).wrap(ratatui::widgets::Wrap { trim: false });

        Self {
            top: 0,
            height: 0,
            command_column,
            description_column,
            max_command_width,
        }
    }

    pub fn set_height(&mut self, height: usize) {
        self.height = height;
    }

    pub fn pan_vertically(&mut self, dir: Direction, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.height,
            ViewDelta::HalfPage => self.height.div_ceil(2),
            ViewDelta::Boundary if let Direction::Back = dir => usize::MAX,
            ViewDelta::Boundary => 0,
            ViewDelta::Match => unimplemented!("there is no result jumping for help"),
        };
        match dir {
            Direction::Back => self.top = self.top.saturating_sub(delta),
            Direction::Next => self.top = self.top.saturating_add(delta),
        }
    }

    fn split_left(area: Rect, left_width: u16) -> [Rect; 2] {
        let mut left_chunk = area;
        left_chunk.width = left_width;

        let mut right_chunk = area;
        right_chunk.x += left_width;
        right_chunk.width = right_chunk.width.saturating_sub(left_width);

        [left_chunk, right_chunk]
    }

    pub fn render(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::buffer::Buffer) {
        static WIDGET_BLOCK: OnceLock<Block> = OnceLock::new();
        WIDGET_BLOCK
            .get_or_init(|| Block::new().bg(colors::BLACK))
            .render(area, buf);

        let [command_area, description_area] = Self::split_left(area, self.max_command_width as u16 + 4);
        self.command_column
            .clone()
            .scroll((self.top as u16, 0))
            .render(command_area, buf);
        self.description_column
            .clone()
            .scroll((self.top as u16, 0))
            .render(description_area, buf);
    }
}
