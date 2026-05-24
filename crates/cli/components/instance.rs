use std::path::{Path, PathBuf};

use super::{
    cursor::{Cursor, CursorState, SelectionOrigin},
    virtual_view::{CachedLine, VirtualView},
    viewport::Viewport,
};
use crate::{
    app::{
        control::ViewDelta,
        filters::{self, Filter, FilterExportSet},
    },
    colors::ColorSelector,
    direction::Direction,
};
use bvr_core::SegBuffer;
use bvr_core::{Result, matches::CompositeStrategy};
use ratatui::prelude::Color;

pub struct Instance {
    name: String,
    link: Option<PathBuf>,
    buf: SegBuffer,
    cursor: CursorState,
    compositor: filters::State,
    view: VirtualView,
}

impl Instance {
    pub fn new(name: String, link: Option<PathBuf>, buf: SegBuffer) -> Self {
        let mut compositor = filters::State::new(&buf);
        let composite = compositor.create_composite();
        Self {
            link,
            view: VirtualView::new(composite),
            compositor: filters::State::new(&buf),
            name,
            buf,
            cursor: CursorState::new(),
        }
    }

    pub fn file(&self) -> &SegBuffer {
        &self.buf
    }

    pub fn viewport(&self) -> &Viewport {
        self.view.viewport()
    }

    pub fn set_follow_output(&mut self, follow_output: bool) {
        self.view.set_follow_output(follow_output);
    }

    pub fn is_following_output(&self) -> bool {
        self.view.is_following_output()
    }

    pub fn visible_line_count(&self) -> usize {
        self.view.composite().len()
    }

    pub fn total_line_count(&self) -> usize {
        self.buf.line_count()
    }

    pub fn compositor_mut(&mut self) -> &mut filters::State {
        &mut self.compositor
    }

    pub fn color_selector(&self) -> &ColorSelector {
        self.compositor.color_selector()
    }

    pub fn cursor(&self) -> &CursorState {
        &self.cursor
    }

    pub fn nearest_index(&self, line_number: usize) -> Option<usize> {
        self.view
            .composite()
            .nearest_backward(line_number)
            .and_then(|ln| self.view.composite().find(ln))
    }

    pub fn update_viewport(&mut self, height: usize, width: usize) {
        self.view.fit(height, width);
        self.view.set_end_index(self.visible_line_count());
    }

    pub fn view(&mut self) -> impl Iterator<Item = &CachedLine> {
        self.view.cache_view(&self.buf, Some(&self.compositor))
    }

    pub fn jump_vertically_to(&mut self, index: usize) {
        self.view.jump_vertically_to(&self.buf, index);
    }

    pub fn add_search_filter(&mut self, pattern: &str, literal: bool) -> Result<(), regex::Error> {
        self.compositor
            .add_search_filter(&self.buf, pattern, literal)?;
        self.invalidate_cache();
        Ok(())
    }

    pub fn edit_search_filter(&mut self, pattern: &str, literal: bool) -> Result<(), regex::Error> {
        self.compositor
            .edit_selected_filter(&self.buf, pattern, literal)?;
        self.invalidate_cache();
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn move_selected_into_view(&mut self) {
        let current = match self.cursor.state() {
            Cursor::Singleton(i)
            | Cursor::Selection(i, _, SelectionOrigin::Left)
            | Cursor::Selection(_, i, SelectionOrigin::Right) => i,
        };
        if current < self.view.viewport().top() {
            self.cursor.place(self.view.viewport().top());
        } else if current >= self.view.viewport().bottom() {
            self.cursor
                .place(self.view.viewport().bottom().saturating_sub(1));
        }
    }

    pub fn move_viewport_vertical(&mut self, dir: Direction, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.view.viewport().height(),
            ViewDelta::HalfPage => self.view.viewport().height().div_ceil(2),
            ViewDelta::Boundary => {
                let top = match dir {
                    Direction::Back => 0,
                    Direction::Next => self.view.viewport().bottom().saturating_sub(1),
                };
                self.jump_vertically_to(top);
                self.view.set_follow_output(false);
                return;
            }
            ViewDelta::Match => {
                let current = self.view.viewport().top();
                if let Some(next) =
                    self.compositor
                        .compute_jump(current, dir, self.view.composite())
                {
                    self.jump_vertically_to(next);
                }
                return;
            }
        };
        for _ in 0..delta {
            self.view.pan_vertically(&self.buf, dir);
        }
        self.view.set_follow_output(false);
    }

    pub fn move_viewport_horizontal(&mut self, dir: Direction, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.viewport().width(),
            ViewDelta::HalfPage => self.viewport().width().div_ceil(2),
            _ => 0,
        };
        for _ in 0..delta {
            self.view.pan_horizontal(dir);
        }
        self.set_follow_output(false);
    }

    pub fn move_select(&mut self, dir: Direction, select: bool, delta: ViewDelta) {
        let compute_delta = |i: usize| match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.view.viewport().height(),
            ViewDelta::HalfPage => self.view.viewport().height().div_ceil(2),
            ViewDelta::Boundary => usize::MAX,
            ViewDelta::Match => i.abs_diff(
                self.compositor
                    .compute_jump(i, dir, self.view.composite())
                    .unwrap_or(i),
            ),
        };

        match dir {
            Direction::Back => self
                .cursor
                .back(select, |i| i.saturating_sub(compute_delta(i))),
            Direction::Next => self
                .cursor
                .forward(select, |i| i.saturating_add(compute_delta(i))),
        }
        self.cursor
            .clamp(self.visible_line_count().saturating_sub(1));
        let i = match self.cursor.state() {
            Cursor::Singleton(i)
            | Cursor::Selection(i, _, SelectionOrigin::Left)
            | Cursor::Selection(_, i, SelectionOrigin::Right) => i,
        };
        self.jump_vertically_to(i);
    }

    pub fn toggle_bookmark_line_number(&mut self, line_number: usize) {
        self.compositor
            .filters_mut()
            .bookmarks_mut()
            .toggle(line_number);
        self.cursor
            .clamp(self.visible_line_count().saturating_sub(1));
        self.view.set_end_index(self.visible_line_count());

        if self
            .compositor
            .filters()
            .iter_active()
            .all(|filter| !filter.has_line(line_number))
        {
            self.invalidate_cache();
        } else {
            self.view.reset_color_cache();
        }
    }

    pub fn toggle_select_bookmarks(&mut self) {
        let mut needs_invalidation = true;
        match self.cursor.state() {
            Cursor::Singleton(i) => {
                let line_number = self.view.line_at_view_index(i).unwrap();
                return self.toggle_bookmark_line_number(line_number);
            }
            Cursor::Selection(start, end, _) => {
                let line_numbers = (start..=end)
                    .map(|i| self.view.line_at_view_index(i).unwrap())
                    .collect::<Vec<_>>();
                let present = line_numbers
                    .iter()
                    .all(|&i| self.compositor.filters().bookmarks().has_line(i));

                for line_number in line_numbers {
                    needs_invalidation = self
                        .compositor
                        .filters()
                        .iter_active()
                        .all(|filter| !filter.has_line(line_number));
                    let bookmarks = self.compositor.filters_mut().bookmarks_mut();
                    if present {
                        bookmarks.remove(line_number);
                    } else {
                        bookmarks.add(line_number);
                    }
                }
            }
        }
        self.cursor
            .clamp(self.visible_line_count().saturating_sub(1));
        self.view.set_end_index(self.visible_line_count());
        if needs_invalidation {
            self.invalidate_cache();
        } else {
            self.view.reset_color_cache();
        }
    }

    pub fn clear_filters(&mut self) {
        self.compositor.clear_filters();
        self.invalidate_cache();
    }

    pub fn toggle_selected_filters(&mut self) {
        self.compositor
            .toggle_filters(self.compositor.selected_filter_indices());
        self.invalidate_cache();
    }

    pub fn remove_selected_filters(&mut self) {
        self.compositor
            .remove_filters(self.compositor.selected_filter_indices());
        self.invalidate_cache();
    }

    pub fn displace_selected_filters(&mut self, dir: Direction, delta: ViewDelta) {
        self.compositor
            .displace_filters(self.compositor.selected_filter_indices(), dir, delta);
        self.invalidate_cache();
    }

    pub fn toggle_filter(&mut self, filter_index: usize) {
        self.compositor
            .filters_mut()
            .get_mut(filter_index)
            .map(Filter::toggle);
        self.invalidate_cache();
    }

    pub fn set_composite_strategy(&mut self, strategy: CompositeStrategy) {
        self.compositor.set_strategy(strategy);
        self.invalidate_cache();
    }

    pub fn write_bytes(&self, mut file: &mut impl std::io::Write) -> Result<()> {
        self.buf.write_bytes(&mut file, self.view.composite())
    }

    pub fn export_string(&mut self) -> Result<String> {
        let mut output = String::new();
        self.buf
            .write_to_string(&mut output, self.view.composite())?;
        output.truncate(output.trim_end_matches('\0').len());
        Ok(output)
    }

    pub fn invalidate_cache(&mut self) {
        let prev_all = self.view.composite().is_all();
        let now_all = !self.compositor.needs_composite();

        if prev_all && now_all {
            self.view.reset_color_cache();
        } else {
            self.view
                .insert_new_line_set(self.compositor.create_composite());
        }
    }

    pub fn import_user_filters(&mut self, filters: &FilterExportSet) {
        self.compositor
            .filters_mut()
            .import_user_filters(&self.buf, filters);
        self.invalidate_cache();
    }

    pub fn link(&self) -> Option<&Path> {
        self.link.as_deref()
    }

    pub(crate) fn set_selected_filter_color(&mut self, color: Color) {
        self.compositor
            .selected_filter_mut()
            .map(|filter| filter.set_color(color));
        self.view.reset_color_cache();
    }
}
