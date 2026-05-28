use ratatui::{prelude::*, widgets::*};

use super::Mode;
use crate::{app::viewer::Instance, colors};

pub struct MultiplexerWidget<'a> {
    pub override_mode: Option<Mode>,
    pub mux: &'a mut super::State,
}

impl<'a> MultiplexerWidget<'a> {
    pub fn hydrate(mux: &'a mut super::State) -> Self {
        Self {
            override_mode: None,
            mux,
        }
    }

    pub fn override_mode(mut self, mode: Option<Mode>) -> Self {
        self.override_mode = mode;
        self
    }

    fn split_horizontal(area: Rect, len: usize) -> std::rc::Rc<[Rect]> {
        let constraints = vec![Constraint::Ratio(1, len as u32); len];
        Layout::new(Direction::Horizontal, constraints).split(area)
    }

    pub fn render(
        self,
        area: Rect,
        buf: &'a mut Buffer,
        mut draw: impl FnMut(Rect, &mut Buffer, usize, &'a mut Instance),
    ) {
        if !self.mux.is_empty() {
            let active = self.mux.active_index();
            match self.override_mode.unwrap_or(self.mux.mode()) {
                Mode::SplitView => {
                    let split_chunks = Self::split_horizontal(area, self.mux.len());
                    for (view_index, (&pane_chunk, instance)) in split_chunks
                        .iter()
                        .zip(self.mux.instances_mut())
                        .enumerate()
                    {
                        draw(pane_chunk, buf, view_index, instance);
                    }
                }
                Mode::ActiveOnly => {
                    let instance = self.mux.active_mut().unwrap();
                    let pane_chunk = area;

                    draw(pane_chunk, buf, active, instance);
                }
            };
        } else {
            Block::new().bg(colors::BG).render(area, buf);
        }
    }
}
