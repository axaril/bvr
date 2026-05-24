use crate::app::{
    actions::{Action, NormalAction},
    control::ViewDelta,
    mouse::MouseHandler,
    viewer::{instance::Instance, virtual_view::CachedLine},
};
use crate::{app::actions::VisualAction, colors, cursor::Cursor, direction::Direction};
use bitflags::bitflags;
use crossterm::event::MouseEventKind;
use ratatui::prelude::*;
use regex::bytes::Regex;
use unicode_segmentation::UnicodeSegmentation;

pub struct ViewWidget<'a> {
    pub view_index: usize,
    pub instance: &'a mut Instance,
    pub show_selection: bool,
    pub gutter: bool,
    pub regex: Option<(Color, &'a Regex)>,
}

bitflags! {
    struct LineType: u8 {
        const None = 0;
        const Origin = 1 << 0;
        const OriginStart = 1 << 1;
        const OriginEnd = 1 << 2;
        const Within = 1 << 3;
        const Bookmarked = 1 << 4;
    }
}

impl ViewWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        let left = self.instance.viewport().left();
        let gutter_size = self
            .gutter
            .then(|| (self.instance.total_line_count().max(1).ilog10() as u16 + 1).max(4));

        let mut itoa_buf = itoa::Buffer::new();

        let cursor_state = self.instance.cursor().state();

        self.instance
            .update_viewport(area.height as usize, area.width as usize);

        self.instance.populate_view();
        let view = self.instance.view_cache();

        (area.y..area.bottom())
            .zip(view.map(Some).chain(std::iter::repeat(None)))
            .for_each(|(y, line_data)| {
                ViewerLineWidget {
                    parent: &self,
                    start: left,
                    line_data,
                    ty: if let Some(line) = line_data {
                        match cursor_state {
                            Cursor::Singleton(i) => {
                                if line.index == i {
                                    LineType::Origin
                                } else {
                                    LineType::None
                                }
                            }
                            Cursor::Selection(start, end, _) => {
                                if !(start..=end).contains(&line.index) {
                                    LineType::None
                                } else if line.index == start {
                                    LineType::Origin | LineType::OriginStart
                                } else if line.index == end {
                                    LineType::Origin | LineType::OriginEnd
                                } else {
                                    LineType::Within
                                }
                            }
                        }
                        .union(if line.bookmarked {
                            LineType::Bookmarked
                        } else {
                            LineType::None
                        })
                    } else {
                        LineType::None
                    },
                    itoa_buf: &mut itoa_buf,
                    gutter_size,
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

    itoa_buf: &'a mut itoa::Buffer,
    gutter_size: Option<u16>,
    start: usize,
    ty: LineType,
}

impl ViewerLineWidget<'_> {
    fn gutter_selection(&self) -> &'static str {
        if self.ty.contains(LineType::Origin) {
            if self.ty.contains(LineType::OriginStart) {
                "┌ "
            } else if self.ty.contains(LineType::OriginEnd) {
                "└"
            } else {
                "▶"
            }
        } else if self.ty.contains(LineType::Within) {
            "│"
        } else {
            ""
        }
    }

    fn split_line(&self, area: Rect) -> (Option<Rect>, Option<Rect>, Rect) {
        const SPECIAL_SIZE: u16 = 3;

        if self.gutter_size.is_none() && !self.parent.show_selection {
            return (None, None, area);
        }

        let gutter_size = self.gutter_size.unwrap_or(0);
        let mut gutter_chunk = area;
        gutter_chunk.width = gutter_size;

        let mut cursor_chunk = area;
        cursor_chunk.x += gutter_size + 1;
        cursor_chunk.width = 1;

        let mut data_chunk = area;
        data_chunk.x += gutter_size + SPECIAL_SIZE;
        data_chunk.width = data_chunk.width.saturating_sub(gutter_size + SPECIAL_SIZE);

        (Some(gutter_chunk), Some(cursor_chunk), data_chunk)
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
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
            let ln_str = self.itoa_buf.format(line.line_number + 1);
            let ln = Line::raw(ln_str).alignment(Alignment::Right).fg(
                if self.ty.contains(LineType::Bookmarked) {
                    colors::SELECT_ACCENT
                } else {
                    colors::GUTTER_TEXT
                },
            );

            ln.render(gutter_chunk, buf);
        }

        if let Some(type_chunk) = cursor_chunk {
            Span::raw(self.gutter_selection())
                .fg(colors::SELECT_ACCENT)
                .render(type_chunk, buf);
        }

        let data = {
            let data = &line.data;
            let mut chars = data.grapheme_indices(true);
            let start = chars
                .nth(self.start)
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
        if self.ty.contains(LineType::Bookmarked) {
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
