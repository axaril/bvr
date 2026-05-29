use crate::app::common::gutter::GutterType;
use crate::app::{
    actions::{Action, NormalAction},
    control::ViewDelta,
    mouse::EventHandler,
    viewer::{instance::Instance, virtual_view::CachedLine},
};
use crate::{app::actions::VisualAction, colors, direction::Direction};
use crossterm::event::MouseEventKind;
use ratatui::prelude::*;
use regex::bytes::Regex;
use unicode_segmentation::UnicodeSegmentation;

pub struct ViewWidget<'a> {
    view_index: usize,
    instance: &'a mut Instance,
    show_selection: bool,
    gutter: Option<u16>,
    regex: Option<(Color, &'a Regex)>,
    left: usize,
}

impl<'a> ViewWidget<'a> {
    pub fn new(
        view_index: usize,
        instance: &'a mut Instance,
        show_selection: bool,
        show_gutter: bool,
        regex: Option<(Color, &'a Regex)>,
    ) -> Self {
        Self {
            view_index,
            show_selection,
            gutter: show_gutter
                .then(|| (instance.total_line_count().max(1).ilog10() as u16 + 1).max(4)),
            left: instance.view_bounds().left(),
            regex,
            instance,
        }
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut EventHandler) {
        let cursor_state = self.instance.cursor().state();

        self.instance
            .update_view_bounds(area.height as usize, area.width as usize);

        self.instance.populate_view();
        let view = self.instance.view_cache();

        (area.y..area.bottom())
            .zip(view.map(Some).chain(std::iter::repeat(None)))
            .for_each(|(y, line_data)| {
                ViewerLineWidget {
                    parent: &self,
                    line_data,
                    ty: line_data
                        .map(|line| GutterType::map_cursor_state(cursor_state, line.index))
                        .unwrap_or_default(),
                }
                .render(Rect::new(area.x, y, area.width, 1), buf, handle);
            });

        handle.on_mouse(area, |event| match event.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                Some(Action::Normal(NormalAction::PanVertical {
                    direction: Direction::back_if(event.kind == MouseEventKind::ScrollUp),
                    delta: ViewDelta::Number { value: 5 },
                    target_view: Some(self.view_index),
                }))
            }
            _ => None,
        });
    }
}

struct ViewerLineWidget<'a> {
    parent: &'a ViewWidget<'a>,

    line_data: Option<&'a CachedLine>,
    ty: GutterType,
}

impl ViewerLineWidget<'_> {
    fn split_line(&self, area: Rect) -> (Option<Rect>, Option<Rect>, Rect) {
        const SPECIAL_SIZE: u16 = 3;

        if self.parent.gutter.is_none() && !self.parent.show_selection {
            return (None, None, area);
        }

        let gutter_size = self.parent.gutter.unwrap_or(0);
        let mut gutter_chunk = area;
        gutter_chunk.width = gutter_size;

        let mut cursor_chunk = area;
        cursor_chunk.x += gutter_size;
        cursor_chunk.width = 3;

        let mut data_chunk = area;
        data_chunk.x += gutter_size + SPECIAL_SIZE;
        data_chunk.width = data_chunk.width.saturating_sub(gutter_size + SPECIAL_SIZE);

        (Some(gutter_chunk), Some(cursor_chunk), data_chunk)
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut EventHandler) {
        let (gutter_chunk, cursor_chunk, data_chunk) = self.split_line(area);

        let Some(line) = &self.line_data else {
            let ln = Line::raw("~")
                .alignment(Alignment::Right)
                .fg(colors::GUTTER_TEXT)
                .bg(colors::GUTTER_BG);

            if let Some(gutter_chunk) = gutter_chunk {
                ln.render(gutter_chunk, buf);
            }
            return;
        };

        if let Some(gutter_chunk) = gutter_chunk {
            let mut itoa_buf = itoa::Buffer::new();
            let ln_str = itoa_buf.format(line.line_number + 1);
            let ln = Line::raw(ln_str).alignment(Alignment::Right).fg(
                if self.line_data.map(|l| l.bookmarked).unwrap_or(false) {
                    colors::SELECT_ACCENT
                } else {
                    colors::GUTTER_TEXT
                },
            );

            ln.render(gutter_chunk, buf);
        }

        if let Some(type_chunk) = cursor_chunk
            && self.parent.show_selection
        {
            Span::raw(self.ty.to_gutter())
                .fg(colors::SELECT_ACCENT)
                .render(type_chunk, buf);
        }

        let data = {
            let data = &line.data;
            let mut chars = data.grapheme_indices(true);
            let start = chars
                .nth(self.parent.left)
                .map(|(idx, _)| idx)
                .unwrap_or(data.len());
            let end = chars
                .nth(data_chunk.width as usize)
                .map(|(idx, _)| idx)
                .unwrap_or(data.len());

            data.get(start..end).unwrap_or("Bad char boundary handling")
        };

        let mut line_widget = if let Some((color, regex)) = self.parent.regex
            && let Some(m) = regex.find(data.as_bytes())
        {
            let start = m.start();
            let end = m.end();
            let spans = vec![
                Span::raw(&data[..start]),
                Span::raw(&data[start..end]).bg(color),
                Span::raw(&data[end..]),
            ];
            Line::from(spans)
        } else {
            Line::raw(data)
        };

        line_widget.style.fg = Some(line.color);
        if self.line_data.map(|l| l.bookmarked).unwrap_or(false) {
            line_widget.style.bg = Some(colors::SELECT_ACCENT);
        }

        line_widget.render(data_chunk, buf);

        if let Some(line) = self.line_data {
            handle.on_mouse(area, |event| match event.kind {
                MouseEventKind::Down(_) => Some(Action::Visual(VisualAction::ToggleLine {
                    line_number: line.line_number,
                    target_view: self.parent.view_index,
                })),
                _ => None,
            });
        }
    }
}
