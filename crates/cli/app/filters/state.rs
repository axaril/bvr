use super::{Filter, FilterSet, Filters, Mask};
use crate::{
    app::control::ViewDelta,
    colors::ColorSelector,
    cursor::{Cursor, CursorState, SelectionOrigin},
    direction::Direction,
    viewport::Viewport,
};
use bvr_core::{LineSet, SegBuffer, matches::CompositeStrategy};

pub struct Compositor {
    all_composite: LineSet,
    strategy: CompositeStrategy,
    viewport: Viewport,
    cursor: CursorState,
    filters: Filters,
    color_selector: ColorSelector,
}

impl Compositor {
    pub fn new(buf: &SegBuffer) -> Self {
        Self {
            all_composite: buf.all_line_matches(),
            viewport: Viewport::new(),
            cursor: CursorState::new(),
            filters: Filters::new(),
            strategy: CompositeStrategy::Union,
            color_selector: ColorSelector::new(),
        }
    }

    pub fn set_strategy(&mut self, strategy: CompositeStrategy) {
        self.strategy = strategy;
    }

    pub fn needs_composite(&self) -> bool {
        !self.filters.all.is_enabled()
    }

    pub fn filters_mut(&mut self) -> &mut Filters {
        &mut self.filters
    }

    pub fn filters(&self) -> &Filters {
        &self.filters
    }

    pub fn update_viewport(&mut self, viewport_height: usize) {
        self.viewport.fit(viewport_height, 0);
        self.viewport.clamp(self.filters.len());
    }

    pub fn view(&self) -> impl Iterator<Item = (usize, &Filter)> {
        self.filters
            .iter()
            .enumerate()
            .skip(self.viewport.top())
            .take(self.viewport.height())
    }

    pub fn create_composite(&mut self) -> LineSet {
        if self.filters.all.is_enabled() {
            self.all_composite.clone()
        } else {
            let filters = self
                .filters
                .iter_active()
                .map(|filter| filter.as_line_matches())
                .collect();
            LineSet::compose(filters, false, self.strategy).unwrap()
        }
    }

    pub fn move_select(&mut self, dir: Direction, select: bool, delta: ViewDelta) {
        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.viewport.height(),
            ViewDelta::HalfPage => self.viewport.height().div_ceil(2),
            ViewDelta::Boundary => usize::MAX,
            ViewDelta::Match => unimplemented!("there is no result jumping for filters"),
        };
        match dir {
            Direction::Back => self.cursor.back(select, |i| i.saturating_sub(delta)),
            Direction::Next => self.cursor.forward(select, |i| i.saturating_add(delta)),
        }
        self.cursor.clamp(self.filters.len().saturating_sub(1));
        let i = match self.cursor.state() {
            Cursor::Singleton(i)
            | Cursor::Selection(i, _, SelectionOrigin::Left)
            | Cursor::Selection(_, i, SelectionOrigin::Right) => i,
        };
        self.viewport.jump_vertically_to(i);
    }

    pub fn displace_filters(
        &mut self,
        range: std::ops::Range<usize>,
        dir: Direction,
        delta: ViewDelta,
    ) {
        fn displace_range<T>(
            vec: &mut Vec<T>,
            range: std::ops::Range<usize>,
            delta: usize,
            direction: Direction,
        ) {
            let len = vec.len();
            let start = range.start;
            let end = range.end;
            let range_len = end - start;

            assert!(start < end && end <= len);

            // Extract the elements to move
            let moved: Vec<T> = vec.drain(start..end).collect();

            // Compute clamped insertion index
            let insert_at = match direction {
                Direction::Next => (start + delta).min(len - range_len),
                Direction::Back => start.saturating_sub(delta).max(0),
            };

            vec.splice(insert_at..insert_at, moved);
        }

        let delta = match delta {
            ViewDelta::Number { value } => usize::from(value),
            ViewDelta::Page => self.viewport.height(),
            ViewDelta::HalfPage => self.viewport.height().div_ceil(2),
            ViewDelta::Boundary => usize::MAX,
            ViewDelta::Match => unimplemented!("there is no result jumping for filters"),
        };

        let Some(range) = Self::fixup_range(range) else {
            return;
        };
        if range.is_empty() {
            return;
        }

        displace_range(&mut self.filters.user_filters, range, delta, dir);

        match dir {
            Direction::Back => self.cursor.map(|i| i.saturating_sub(delta)),
            Direction::Next => self.cursor.map(|i| i.saturating_add(delta)),
        }
        self.cursor.clamp(self.filters.len().saturating_sub(1));
        self.cursor.map(|i| i.max(2));
        // let i = match self.cursor.state() {
        //     Cursor::Singleton(i)
        //     | Cursor::Selection(i, _, SelectionOrigin::Left)
        //     | Cursor::Selection(_, i, SelectionOrigin::Right) => i,
        // };
        // self.viewport.jump_vertically_to(i);
    }

    pub fn clear_filters(&mut self) {
        self.cursor = CursorState::new();
        self.color_selector.reset();
        self.filters.clear();
    }

    pub fn selected_filter(&self) -> Option<&Filter> {
        match self.cursor.state() {
            Cursor::Singleton(i) => self.filters.get(i),
            _ => None,
        }
    }

    pub fn selected_filter_mut(&mut self) -> Option<&mut Filter> {
        match self.cursor.state() {
            Cursor::Singleton(i) => self.filters.get_mut(i),
            _ => None,
        }
    }

    pub fn selected_filter_indices(&self) -> std::ops::Range<usize> {
        match self.cursor.state() {
            Cursor::Singleton(i) => i..i + 1,
            Cursor::Selection(start, end, _) => start..end + 1,
        }
    }

    pub fn toggle_filters(&mut self, range: std::ops::Range<usize>) {
        for i in range {
            self.filters.get_mut(i).map(Filter::toggle);
        }
    }

    fn fixup_range(mut range: std::ops::Range<usize>) -> Option<std::ops::Range<usize>> {
        if range.start < 2 {
            if range.end < 2 {
                return None;
            }
            range.start = 2;
        }
        // fixup because the first 2 is pseudo
        range.start -= 2;
        range.end -= 2;
        Some(range)
    }

    pub fn remove_filters(&mut self, range: std::ops::Range<usize>) {
        let Some(range) = Self::fixup_range(range) else {
            return;
        };
        self.filters.user_filters.drain(range);
        self.cursor.clamp(self.filters.len().saturating_sub(1));
    }

    pub fn add_search_filter(
        &mut self,
        file: &SegBuffer,
        pattern: &str,
        literal: bool,
    ) -> Result<(), regex::Error> {
        let (mask, regex) = Mask::build(pattern, literal)?;

        self.filters.user_filters.push(Filter::new(
            mask,
            self.color_selector.next_color(),
            FilterSet::Search(LineSet::search(file.segment_iter().unwrap(), regex)),
        ));
        Ok(())
    }

    pub fn edit_selected_filter(
        &mut self,
        file: &SegBuffer,
        pattern: &str,
        literal: bool,
    ) -> Result<(), regex::Error> {
        let (mask, regex) = Mask::build(pattern, literal)?;

        if let Some(filter) = self.selected_filter_mut() {
            *filter = Filter::new(
                mask,
                filter.color,
                FilterSet::Search(LineSet::search(file.segment_iter().unwrap(), regex)),
            )
        }
        Ok(())
    }

    pub fn compute_jump(
        &self,
        i: usize,
        direction: Direction,
        composite: &LineSet,
    ) -> Option<usize> {
        let compute = |i: usize, match_filter: bool| {
            let active_filters = self.filters.iter_active();
            let iter = active_filters.filter(|fitler| !match_filter || fitler.has_line(i));
            match direction {
                Direction::Back => iter
                    .filter_map(|filter| filter.nearest_backward(i))
                    .filter(|&ln| ln < i)
                    .max(),
                Direction::Next => iter
                    .filter_map(|filter| filter.nearest_forward(i))
                    .filter(|&ln| ln > i)
                    .min(),
            }
        };
        if !self.filters.all.is_enabled() {
            composite.find(compute(composite.get(i)?, true)?)
        } else {
            compute(i, false)
        }
    }

    pub fn cursor(&self) -> &CursorState {
        &self.cursor
    }

    pub fn set_cursor(&mut self, cursor: CursorState) {
        self.cursor = cursor
    }

    pub fn color_selector(&self) -> &ColorSelector {
        &self.color_selector
    }
}
