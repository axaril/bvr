use super::super::mouse::MouseHandler;
use crate::{
    app::common::{Panel, gutter::GutterType},
    colors,
};
use ratatui::{prelude::*, widgets::*};

pub struct ConfigViewerWidget<'a> {
    pub(super) app: &'a mut super::filters::State,
}

impl<'a> ConfigViewerWidget<'a> {
    pub fn hydrate(state: &'a mut super::filters::State) -> Self {
        Self { app: state }
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        Panel::new("Saved Filter Sets", colors::CONFIG_ACCENT, None).render(
            area,
            buf,
            |area, buf| self.render_inner(area, buf, handle),
        );
    }

    fn render_inner(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        let Some([left_chunk, right_chunk]) = crate::split::split_half(area) else {
            return;
        };
        {
            Block::new().bg(colors::STATUS_BAR).render(left_chunk, buf);

            let cursor_state = self.app.cursor().state();

            self.app.update_view_bounds(left_chunk.height as usize);
            let view = self.app.view();

            (left_chunk.y..left_chunk.bottom())
                .zip(view)
                .for_each(|(y, (index, filter))| {
                    ConfigLineWidget {
                        name: filter.name(),
                        ty: GutterType::map_cursor_state(cursor_state, index),
                    }
                    .render(
                        Rect::new(left_chunk.x, y, left_chunk.width, 1),
                        buf,
                        handle,
                    );
                });
        }
        if let Some(filter) = self.app.selected_filter() {
            Block::new().bg(colors::BLACK).render(right_chunk, buf);

            (right_chunk.y..right_chunk.bottom())
                .zip(filter.filters())
                .for_each(|(y, filter)| {
                    FilterLineWidget {
                        color: filter.color(),
                        name: filter.name(),
                        enabled: filter.is_enabled(),
                    }
                    .render(
                        Rect::new(right_chunk.x, y, right_chunk.width, 1),
                        buf,
                        handle,
                    );
                });
        }
    }
}

struct ConfigLineWidget<'a> {
    name: Option<&'a str>,
    ty: GutterType,
}

impl ConfigLineWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer, _: &mut MouseHandler) {
        let mut v = vec![Span::raw(self.ty.to_gutter(" - ")).fg(colors::CONFIG_ACCENT)];

        v.push(Span::raw(self.name.unwrap_or("Untitled Filter Set")).fg(colors::WHITE));

        Line::from(v).render(area, buf);
    }
}

struct FilterLineWidget<'a> {
    color: Color,
    name: &'a str,
    enabled: bool,
}

impl FilterLineWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer, _: &mut MouseHandler) {
        let spans = vec![
            Span::raw(if self.enabled { " ● " } else { " ◯ " }).fg(self.color),
            Span::raw(self.name).fg(self.color),
        ];
        Line::from(spans).render(area, buf);
    }
}
