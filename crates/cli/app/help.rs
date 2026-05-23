use ratatui::{
    text::{Line, Span},
    widgets::{Paragraph, Widget as _},
};

use crate::{app::control::ViewDelta, direction::Direction};

pub struct HelpManual {
    top: usize,
    height: usize,
    content: Paragraph<'static>,
}

impl HelpManual {
    fn walk_commands(lines: &mut Vec<Line<'static>>, commands: &[crate::app::Command], level: usize) {
        for cmd in commands {
            lines.push(Line::from(Span::raw(format!(
                "{}{}",
                "   ".repeat(level),
                cmd.name
            ))));
            Self::walk_commands(lines, &cmd.subcommands, level + 1);
        }
    }

    pub fn new() -> Self {
        Self {
            top: 0,
            height: 0,
            content: Paragraph::new(vec![]),
        }
    }

    pub fn generate(commands: &[crate::app::Command]) -> Self {
        let mut lines = Vec::new();
        Self::walk_commands(&mut lines, commands, 0);

        let content = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false });

        Self {
            top: 0,
            height: 0,
            content,
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

    pub fn render(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::buffer::Buffer) {
        self.content
            .clone()
            .scroll((self.top as u16, 0))
            .render(area, buf);
    }
}
