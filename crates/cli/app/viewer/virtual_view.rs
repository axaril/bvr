use crate::{app::filters, direction::Direction, viewport::Viewport};

use bvr_core::{LineSet, SegBuffer, SegStr};
use ratatui::style::Color;
use std::collections::VecDeque;

#[derive(Clone)]
pub struct CachedLine {
    pub index: usize,
    pub line_number: usize,
    pub data: SegStr,
    pub color: Color,
    pub bookmarked: bool,
}

pub struct VirtualView {
    composite: LineSet,
    cache: VecDeque<CachedLine>,

    viewport: Viewport,

    follow_output: bool,
    end_index: usize,

    need_recoloring: bool,
}

impl VirtualView {
    pub fn new(composite: LineSet) -> Self {
        Self {
            composite,
            cache: VecDeque::new(),
            viewport: Viewport::new(),
            follow_output: false,
            need_recoloring: false,
            end_index: 0,
        }
    }

    pub fn set_end_index(&mut self, end_index: usize) {
        self.end_index = end_index;
    }

    pub fn composite(&self) -> &LineSet {
        &self.composite
    }

    pub fn set_follow_output(&mut self, follow_output: bool) {
        self.follow_output = follow_output;
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn line_at_view_index(&self, index: usize) -> Option<usize> {
        self.composite.get(index)
    }

    fn get_cached_line(&self, index: usize, buf: &SegBuffer) -> Option<CachedLine> {
        let line_number = self.line_at_view_index(index)?;
        let data = buf.get_line(line_number)?;

        Some(CachedLine {
            index,
            line_number,
            data,
            color: Color::Reset,
            bookmarked: false,
        })
    }

    fn push_front(&mut self, index: usize, buf: &SegBuffer) {
        if let Some(line) = self.get_cached_line(index, buf) {
            self.cache.push_front(line);
        }
    }

    fn push_back(&mut self, index: usize, buf: &SegBuffer) -> bool {
        if let Some(line) = self.get_cached_line(index, buf) {
            self.cache.push_back(line);
            true
        } else {
            false
        }
    }

    fn fill_cache(&mut self, buf: &SegBuffer) {
        while self.cache.len() < self.viewport.height() {
            if !self.push_back(self.cache.len() + self.viewport.top(), buf) {
                break;
            }
        }
        self.cache.truncate(self.viewport.height());
    }

    pub fn jump_vertically_to(&mut self, buf: &SegBuffer, index: usize) {
        self.viewport.jump_vertically_to(index);
        self.cache.clear();
        self.fill_cache(buf);
    }

    pub fn pan_vertically(&mut self, buf: &SegBuffer, direction: Direction) {
        match direction {
            Direction::Back => {
                if self.viewport.pan_vertical(direction) {
                    self.push_front(self.viewport.top(), buf);
                    self.cache.truncate(self.viewport.height());
                }
            }
            Direction::Next => {
                if self.viewport.pan_vertical(direction) {
                    self.cache.pop_front();
                    self.fill_cache(buf);
                }
            }
        }
    }

    pub fn cache_view(
        &mut self,
        buf: &SegBuffer,
        compositor: Option<&filters::State>,
    ) -> impl Iterator<Item = &CachedLine> {
        if self.follow_output {
            self.jump_vertically_to(buf, self.end_index.saturating_sub(1));
        }

        self.viewport.clamp(self.end_index);

        self.fill_cache(buf);

        if let Some(compositor) = compositor {
            self.color_cache(compositor);
        }

        self.cache.iter()
    }

    pub fn color_cache(&mut self, compositor: &filters::State) {
        if self.need_recoloring {
            self.reset_color_cache();
            self.need_recoloring = compositor
                .filters()
                .iter_active()
                .any(|filter| !filter.is_complete());
        }

        let filters = compositor.filters().iter_active().collect::<Vec<_>>();

        self.cache
            .iter_mut()
            .filter(|line| line.color == Color::Reset)
            .for_each(|line| {
                line.color = filters
                    .iter()
                    .rev()
                    .filter(|filter| !filter.is_bookmark())
                    .find(|filter| filter.has_line(line.line_number))
                    .map(|filter| filter.color())
                    .unwrap_or(Color::White);

                line.bookmarked = compositor.filters().bookmarks().has_line(line.line_number);
            });
    }

    pub fn reset_color_cache(&mut self) {
        self.need_recoloring = true;
        self.cache
            .iter_mut()
            .for_each(|line| line.color = Color::Reset);
    }

    pub fn insert_new_line_set(&mut self, line_set: LineSet) {
        self.cache.clear();
        let old_line_number = self.line_at_view_index(self.viewport.top());
        self.composite = line_set;
        if let Some(old_line_number) = old_line_number {
            if let Some(index) = self.composite.find(old_line_number) {
                self.viewport.top_to(index);
            }
        }
    }

    pub fn is_following_output(&self) -> bool {
        self.follow_output
    }

    pub fn fit(&mut self, height: usize, width: usize) {
        self.viewport.fit(height, width);
    }

    pub fn pan_horizontal(&mut self, dir: Direction) {
        self.viewport.pan_horizontal(dir);
    }
}
