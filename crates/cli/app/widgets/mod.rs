mod viewer;

use super::{
    InputMode,
    actions::{Action, NormalAction},
    mouse::MouseHandler,
};
use crate::{
    app::{
        config, status, filters,
        widgets::viewer::LineViewerWidget,
    },
    colors,
    components::{
        instance::Instance,
        mux::{MultiplexerApp, MultiplexerMode},
    },
};
use crossterm::event::MouseEventKind;
use ratatui::{prelude::*, widgets::*};
use regex::bytes::Regex;
use std::sync::OnceLock;

pub struct TabWidget<'a> {
    view_index: usize,
    name: &'a str,
    active: bool,
}

impl TabWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer, handle: &mut MouseHandler) {
        Line::from(vec![
            if self.active {
                Span::raw("▍ ").fg(colors::TAB_SIDE_ACTIVE)
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

pub struct MultiplexerPane<'a> {
    view_index: usize,
    instance: &'a mut Instance,
    show_filter_on_pane: bool,
    show_selection: bool,
    gutter: bool,
    regex: Option<&'a Regex>,
}

impl MultiplexerPane<'_> {
    const FILTER_MAX_HEIGHT: u16 = 10;

    fn filter_area(area: &mut Rect, f: impl FnOnce(Rect)) {
        let [view_chunk, filter_chunk] =
            MultiplexerWidget::split_bottom(*area, Self::FILTER_MAX_HEIGHT);
        f(filter_chunk);
        *area = view_chunk;
    }

    fn render_filter_pane(
        area: &mut Rect,
        buf: &mut Buffer,
        view_index: usize,
        compositor: &mut filters::State,
        handler: &mut MouseHandler,
    ) {
        Self::filter_area(area, |area| {
            filters::Widget {
                view_index,
                compositor,
            }
            .render(area, buf, handler);
        });
    }

    pub fn render(self, mut area: Rect, buf: &mut Buffer, handler: &mut MouseHandler) {
        if self.show_filter_on_pane {
            Self::render_filter_pane(
                &mut area,
                buf,
                self.view_index,
                self.instance.compositor_mut(),
                handler,
            );
        }

        LineViewerWidget {
            view_index: self.view_index,
            show_selection: self.show_selection,
            instance: self.instance,
            gutter: self.gutter,
            regex: self.regex,
        }
        .render(area, buf, handler);
    }
}

pub struct MultiplexerWidget<'a> {
    pub mux: &'a mut MultiplexerApp,
    pub status: &'a mut status::State,
    pub config: &'a mut config::State,
    pub help: &'a mut super::help::HelpManual,
    pub mode: InputMode,
    pub gutter: bool,
    pub regex: Option<&'a Regex>,
    pub linked_filters: bool,
}

impl MultiplexerWidget<'_> {
    fn split_horizontal(area: Rect, len: usize) -> std::rc::Rc<[Rect]> {
        let constraints = vec![Constraint::Ratio(1, len as u32); len];
        Layout::new(Direction::Horizontal, constraints).split(area)
    }

    fn split_top(area: Rect, top_height: u16) -> [Rect; 2] {
        let mut tab_chunk = area;
        tab_chunk.height = top_height;

        let mut data_chunk = area;
        data_chunk.y += top_height;
        data_chunk.height = data_chunk.height.saturating_sub(top_height);

        [tab_chunk, data_chunk]
    }

    pub fn split_bottom(area: Rect, bottom_height: u16) -> [Rect; 2] {
        let mut view_chunk = area;
        view_chunk.height = view_chunk.height.saturating_sub(bottom_height);

        let mut filter_chunk = area;
        filter_chunk.y = area.y + view_chunk.height;
        filter_chunk.height = bottom_height.min(area.height);

        [view_chunk, filter_chunk]
    }

    fn render_mux(&mut self, mut area: Rect, buf: &mut Buffer, handler: &mut MouseHandler) {
        let active = self.mux.active_index();

        let show_filter_on_pane = self.mode == InputMode::Filter && !self.linked_filters;
        let show_filter_on_mux = self.mode == InputMode::Filter && self.linked_filters;

        if show_filter_on_mux {
            MultiplexerPane::render_filter_pane(
                &mut area,
                buf,
                active,
                self.mux.active_mut().unwrap().compositor_mut(),
                handler,
            );
        }

        if self.mode == InputMode::Config {
            MultiplexerPane::filter_area(&mut area, |area| {
                config::hydrate(self.config).render(area, buf, handler);
            });
        } else if self.mode == InputMode::Help {
            MultiplexerPane::filter_area(&mut area, |area| {
                self.help.set_height(usize::from(area.height));
                self.help.render(area, buf);
            });
        }

        let [tab_chunk, view_chunk] = Self::split_top(area, 1);
        let split_chunks = Self::split_horizontal(area, self.mux.len());

        for (view_index, (chunk, instance)) in split_chunks
            .iter()
            .map(|&chunk| tab_chunk.intersection(chunk))
            .zip(self.mux.instances_mut())
            .enumerate()
        {
            TabWidget {
                view_index,
                name: instance.name(),
                active: active == view_index,
            }
            .render(chunk, buf, handler);
        }

        match self.mux.mode() {
            MultiplexerMode::Panes => {
                for (view_index, (pane_chunk, instance)) in split_chunks
                    .iter()
                    .map(|&chunk| view_chunk.intersection(chunk))
                    .zip(self.mux.instances_mut())
                    .enumerate()
                {
                    MultiplexerPane {
                        view_index,
                        instance,
                        show_filter_on_pane,
                        show_selection: self.mode == InputMode::Visual,
                        gutter: self.gutter,
                        regex: self.regex,
                    }
                    .render(pane_chunk, buf, handler);
                }
            }
            MultiplexerMode::Tabs => {
                let instance = self.mux.active_mut().unwrap();
                let pane_chunk = view_chunk;

                MultiplexerPane {
                    view_index: active,
                    instance,
                    show_filter_on_pane,
                    show_selection: self.mode == InputMode::Visual,
                    gutter: self.gutter,
                    regex: self.regex,
                }
                .render(pane_chunk, buf, handler);
            }
        }
    }

    pub fn render(mut self, area: Rect, buf: &mut Buffer, handler: &mut MouseHandler) {
        let [mux_chunk, status_chunk] = Self::split_bottom(area, 1);

        if !self.mux.is_empty() {
            self.render_mux(mux_chunk, buf, handler);
        } else {
            static BG_BLOCK: OnceLock<Block> = OnceLock::new();
            BG_BLOCK
                .get_or_init(|| Block::new().bg(colors::BG))
                .render(mux_chunk, buf);
        }

        status::Widget::new(self.mode)
            .with_instance(self.mux.active_mut().map(|v| &*v))
            .with_message(self.status.get_message_update().as_deref())
            .render(status_chunk, buf);
    }
}
