use super::super::{
    actions::{Action, FilterAction},
    mouse::MouseHandler,
};
use crate::{
    app::{
        common::{Panel, gutter::GutterType},
        filters::Filter,
        highlight,
    },
    colors,
};
use crossterm::event::MouseEventKind;
use ratatui::prelude::*;

pub struct FilterViewerWidget<'a> {
    pub(crate) view_index: usize,
    pub(crate) compositor: &'a mut super::State,
}

impl FilterViewerWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        Panel::new("Filters", colors::FILTER_ACCENT, Some(colors::STATUS_BAR)).render(
            area,
            buf,
            |area, buf| self.render_inner(area, buf, handle),
        );
    }

    fn render_inner(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        let cursor_state = self.compositor.cursor().state();

        self.compositor.update_view_bounds(area.height as usize);
        let view = self.compositor.view();

        (area.y..area.bottom())
            .zip(view)
            .for_each(|(y, (index, filter))| {
                FilterLineWidget {
                    view_index: self.view_index,
                    index,
                    filter,
                    ty: GutterType::map_cursor_state(cursor_state, index),
                    enabled: filter.is_enabled(),
                }
                .render(Rect::new(area.x, y, area.width, 1), buf, handle);
            });
    }
}

struct FilterLineWidget<'a> {
    view_index: usize,
    index: usize,
    filter: &'a Filter,
    ty: GutterType,
    enabled: bool,
}

impl FilterLineWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        let color = self.filter.color();

        let mut v = vec![
            Span::raw(self.ty.to_gutter(" - ")).fg(colors::FILTER_ACCENT),
            Span::raw(if self.enabled { "● " } else { "◯ " }).fg(color),
        ];

        if self.filter.mask().regex().is_some() && !self.filter.mask().escaped() {
            v.extend(highlight::RegexHighlighter::new(self.filter.mask().name()).highlight());
        } else {
            v.push(Span::raw(self.filter.mask().name()).fg(color));
        }

        if self.filter.mask().escaped() {
            v.push(Span::raw(" (escaped)").fg(colors::TEXT_INACTIVE));
        }

        if let Some(len) = self.filter.len() {
            v.push(Span::raw(format!(" {}", len)).fg(colors::TEXT_INACTIVE));
        }

        if !self.filter.is_complete() {
            v.push(Span::raw(" (searching)").fg(colors::TEXT_INACTIVE));
        }

        Line::from(v).render(area, buf);

        handle.on_mouse(area, |event| match event.kind {
            MouseEventKind::Down(_) => Some(Action::Filter(FilterAction::ToggleSpecificFilter {
                target_view: self.view_index,
                filter_index: self.index,
            })),
            _ => None,
        });
    }
}
