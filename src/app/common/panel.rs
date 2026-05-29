use ratatui::{prelude::*, widgets::*};

use crate::colors;

pub struct Panel<'a> {
    name: &'a str,
    title_color: Color,
    bg_color: Option<Color>,
}

impl<'a> Panel<'a> {
    pub fn new(name: &'a str, title_color: Color, bg_color: Option<Color>) -> Self {
        Self {
            name,
            title_color,
            bg_color,
        }
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, inner: impl FnOnce(Rect, &mut Buffer)) {
        let Some([title_area, area]) = crate::split::split_top(area, 1) else {
            return;
        };

        Line::from(vec![Span::raw(" ▒ "), Span::raw(self.name)])
            .fg(colors::BLACK)
            .bg(self.title_color)
            .render(title_area, buf);

        if let Some(bg_color) = self.bg_color {
            Block::new().bg(bg_color).render(area, buf);
        }

        inner(area, buf);
    }
}
