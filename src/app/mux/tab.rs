use crossterm::event::MouseEventKind;
use ratatui::prelude::*;

use crate::{
    app::{
        actions::{Action, NormalAction},
        mouse::EventHandler,
    },
    colors,
};

pub struct TabWidget<'a> {
    pub view_index: usize,
    pub name: &'a str,
    pub active: bool,
}

impl TabWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut EventHandler) {
        Line::from(vec![
            if self.active {
                Span::raw("▌ ").fg(colors::TAB_SIDE_ACTIVE)
            } else {
                Span::raw("▏ ").fg(colors::TAB_SIDE_INACTIVE)
            },
            Span::raw(self.name),
        ])
        .bg(if self.active {
            colors::TAB_ACTIVE
        } else {
            colors::TAB_INACTIVE
        })
        .fg(if self.active {
            colors::TEXT_ACTIVE
        } else {
            colors::TEXT_INACTIVE
        })
        .render(area, buf);

        handle.on_mouse(area, |event| match event.kind {
            MouseEventKind::Down(_) => Some(Action::Normal(NormalAction::SwitchActiveIndex {
                target_view: self.view_index,
            })),
            _ => None,
        });
    }
}
