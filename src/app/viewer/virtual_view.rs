use crate::{
    app::filters,
    direction::Direction,
    text::{DEFAULT_TAB_WIDTH, wrap_points},
    view_bounds::ViewBounds,
};

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
    /// Byte offsets at which each visual row after the first one begins.
    /// Empty when the line is not wrapped.
    breaks: Vec<usize>,
}

impl CachedLine {
    fn new(
        index: usize,
        line_number: usize,
        data: SegStr,
        wrap_width: Option<usize>,
        tab_width: usize,
    ) -> Self {
        let breaks = wrap_width
            .map(|width| wrap_points(&data, width, tab_width))
            .unwrap_or_default();

        Self {
            index,
            line_number,
            data,
            color: Color::Reset,
            bookmarked: false,
            breaks,
        }
    }

    /// Number of visual rows this line occupies.
    #[inline]
    pub fn row_count(&self) -> usize {
        self.breaks.len() + 1
    }

    /// Byte offset in the line at which the visual row `row` begins.
    pub fn row_start(&self, row: usize) -> usize {
        match row {
            0 => 0,
            _ => self.breaks.get(row - 1).copied().unwrap_or(self.data.len()),
        }
    }

    /// Text of the visual row `row`. The whole line if it is not wrapped.
    pub fn row(&self, row: usize) -> &str {
        let end = self.breaks.get(row).copied().unwrap_or(self.data.len());
        &self.data[self.row_start(row)..end]
    }
}

pub struct VirtualView {
    composite: LineSet,
    /// Contiguous run of lines starting at `bounds.top()` that covers the view.
    cache: VecDeque<CachedLine>,

    bounds: ViewBounds,
    /// Visual row within the top line at which the view begins.
    /// Always 0 when line wrapping is disabled.
    top_row: usize,
    wrap: bool,
    tab_width: usize,

    follow_output: bool,
    end_index: usize,

    need_recoloring: bool,
}

impl VirtualView {
    pub fn new(composite: LineSet) -> Self {
        Self {
            composite,
            cache: VecDeque::new(),
            bounds: ViewBounds::new(),
            top_row: 0,
            wrap: false,
            tab_width: DEFAULT_TAB_WIDTH,
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

    pub fn view_bounds(&self) -> &ViewBounds {
        &self.bounds
    }

    /// One past the view index of the last line that is fully visible.
    ///
    /// The top line always counts as visible, even if it is taller than the view.
    pub fn bottom(&self) -> usize {
        if self.cache.is_empty() {
            return self.bounds.bottom();
        }

        let height = self.bounds.height();
        let mut rows = 0;
        let visible = self
            .cache
            .iter()
            .take_while(|line| {
                rows += line.row_count();
                rows.saturating_sub(self.top_row) <= height
            })
            .count();

        self.bounds.top() + visible.max(1)
    }

    pub fn line_at_view_index(&self, index: usize) -> Option<usize> {
        self.composite.get(index)
    }

    fn wrap_width(&self) -> Option<usize> {
        self.wrap
            .then_some(self.bounds.width())
            .filter(|&width| width > 0)
    }

    fn get_cached_line(&self, index: usize, buf: &SegBuffer) -> Option<CachedLine> {
        let line_number = self.line_at_view_index(index)?;
        let data = buf.get_line(line_number)?;

        Some(CachedLine::new(
            index,
            line_number,
            data,
            self.wrap_width(),
            self.tab_width,
        ))
    }

    fn push_front(&mut self, index: usize, buf: &SegBuffer) -> bool {
        if let Some(line) = self.get_cached_line(index, buf) {
            self.cache.push_front(line);
            true
        } else {
            false
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
        if self.cache.is_empty() {
            self.push_back(self.bounds.top(), buf);
        }

        self.top_row = match self.cache.front() {
            Some(line) => self.top_row.min(line.row_count() - 1),
            None => 0,
        };

        let height = self.bounds.height();
        let mut rows = self.cache.iter().map(CachedLine::row_count).sum::<usize>() - self.top_row;

        while rows < height {
            if !self.push_back(self.cache.len() + self.bounds.top(), buf) {
                break;
            }
            rows += self.cache.back().map_or(0, CachedLine::row_count);
        }

        while self.cache.len() > 1 {
            let last_rows = self.cache.back().map_or(0, CachedLine::row_count);
            if rows - last_rows < height {
                break;
            }
            rows -= last_rows;
            self.cache.pop_back();
        }
    }

    /// Scrolls so that the last row of the line at `index` sits at the bottom of the view.
    /// If the line is taller than the view, its first row is placed at the top instead.
    fn anchor_bottom(&mut self, buf: &SegBuffer, index: usize) {
        let height = self.bounds.height();
        let line_rows = |line: &Option<CachedLine>| line.as_ref().map_or(1, CachedLine::row_count);

        self.cache.clear();
        let mut contiguous = true;
        let mut top = index;
        let mut rows = 0;
        loop {
            let line = self.get_cached_line(top, buf);
            rows += line_rows(&line);
            match line {
                Some(line) if contiguous => self.cache.push_front(line),
                _ => {
                    contiguous = false;
                    self.cache.clear();
                }
            }
            if rows >= height || top == 0 {
                break;
            }
            top -= 1;
        }

        self.bounds.top_to(top);
        self.top_row = if top == index {
            0
        } else {
            rows.saturating_sub(height)
        };
    }

    pub fn jump_vertically_to(&mut self, buf: &SegBuffer, index: usize) {
        self.cache.clear();

        let top = self.bounds.top();
        if index < top || (index == top && self.top_row > 0) {
            self.bounds.top_to(index);
            self.top_row = 0;
        } else if index >= top + self.bounds.height() {
            // Every line takes at least one row, so this is out of view without looking.
            self.anchor_bottom(buf, index);
        } else {
            self.fill_cache(buf);
            if index >= self.bottom() {
                self.anchor_bottom(buf, index);
            }
        }

        self.fill_cache(buf);
    }

    pub fn pan_vertical(&mut self, buf: &SegBuffer, direction: Direction, delta: usize) {
        if self.wrap_width().is_some() {
            return self.pan_vertical_rows(buf, direction, delta);
        }

        match direction {
            Direction::Back => {
                let delta = self.bounds.pan_vertical(direction, delta);
                for i in (0..delta).rev() {
                    self.push_front(self.bounds.top() + i, buf);
                }
                self.cache.truncate(self.bounds.height());
            }
            Direction::Next => {
                let delta = self.bounds.pan_vertical(direction, delta);
                for _ in 0..delta {
                    self.cache.pop_front();
                }
                self.fill_cache(buf);
            }
        }
    }

    /// Pans by `delta` visual rows, stepping into and out of wrapped lines.
    fn pan_vertical_rows(&mut self, buf: &SegBuffer, direction: Direction, mut delta: usize) {
        self.fill_cache(buf);

        match direction {
            Direction::Back => {
                while delta > self.top_row {
                    let top = self.bounds.top();
                    if top == 0 || !self.push_front(top - 1, buf) {
                        self.top_row = 0;
                        delta = 0;
                        break;
                    }
                    delta -= self.top_row + 1;
                    self.bounds.top_to(top - 1);
                    self.top_row = self.cache.front().map_or(0, |line| line.row_count() - 1);
                }
                self.top_row -= delta;
            }
            Direction::Next => {
                while let Some(line) = self.cache.front() {
                    let rows_below = line.row_count() - 1 - self.top_row;
                    if delta <= rows_below {
                        self.top_row += delta;
                        break;
                    }

                    let next = self.bounds.top() + 1;
                    if next >= self.end_index
                        || (self.cache.len() == 1 && !self.push_back(next, buf))
                    {
                        self.top_row += rows_below;
                        break;
                    }
                    delta -= rows_below + 1;
                    self.cache.pop_front();
                    self.bounds.top_to(next);
                    self.top_row = 0;
                }
            }
        }

        self.fill_cache(buf);
    }

    pub fn populate(&mut self, buf: &SegBuffer, compositor: Option<&filters::State>) {
        if self.follow_output {
            self.jump_vertically_to(buf, self.end_index.saturating_sub(1));
        }

        let top = self.bounds.top();
        self.bounds.clamp(self.end_index);
        if top != self.bounds.top() {
            self.top_row = 0;
        }

        self.fill_cache(buf);

        if let Some(compositor) = compositor {
            self.color_cache(compositor);
        }
    }

    /// Visible rows, in order, as the line they belong to and the row within that line.
    pub fn view(&self) -> impl Iterator<Item = (&CachedLine, usize)> {
        self.cache
            .iter()
            .flat_map(|line| (0..line.row_count()).map(move |row| (line, row)))
            .skip(self.top_row)
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
        let old_line_number = self.line_at_view_index(self.bounds.top());
        self.composite = line_set;
        match old_line_number.and_then(|ln| self.composite.find(ln)) {
            Some(index) => self.bounds.top_to(index),
            None => self.top_row = 0,
        }
    }

    pub fn is_following_output(&self) -> bool {
        self.follow_output
    }

    /// Resizes the view. `width` is the width available to line contents,
    /// which is also the width lines are wrapped at when `wrap` is set.
    pub fn fit(&mut self, height: usize, width: usize, wrap: bool, tab_width: usize) {
        let old_wrap_width = self.wrap_width();
        let old_tab_width = self.tab_width;
        self.bounds.fit(height, width);
        self.wrap = wrap;
        self.tab_width = tab_width;

        // Tabs change where lines wrap, but only when they are wrapped.
        if self.wrap_width() != old_wrap_width || (self.wrap && tab_width != old_tab_width) {
            self.cache.clear();
        }
    }

    pub fn pan_horizontal(&mut self, dir: Direction, delta: usize) {
        if !self.wrap {
            self.bounds.pan_horizontal(dir, delta);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn setup(lines: &[&str], height: usize, width: usize, wrap: bool) -> (SegBuffer, VirtualView) {
        let data = lines.join("\n");
        let buf = SegBuffer::read_stream(Box::new(std::io::Cursor::new(data))).unwrap();
        buf.wait_complete();

        let mut view = VirtualView::new(buf.all_line_matches());
        view.fit(height, width, wrap, DEFAULT_TAB_WIDTH);
        view.set_end_index(view.composite().len());
        assert_eq!(view.composite().len(), lines.len());
        view.populate(&buf, None);
        (buf, view)
    }

    fn rows(view: &VirtualView) -> Vec<(usize, String)> {
        view.view()
            .take(view.view_bounds().height())
            .map(|(line, row)| (line.index, line.row(row).trim_end().to_owned()))
            .collect()
    }

    #[test]
    fn breaks_by_display_width() {
        let line = |data: &str, width| {
            let buf = SegBuffer::read_stream(Box::new(std::io::Cursor::new(format!("{data}\n"))))
                .unwrap();
            buf.wait_complete();
            CachedLine::new(0, 0, buf.get_line(0).unwrap(), Some(width), DEFAULT_TAB_WIDTH)
        };
        let row_strs = |line: &CachedLine| {
            (0..line.row_count())
                .map(|row| line.row(row).trim_end().to_owned())
                .collect::<Vec<_>>()
        };

        assert_eq!(row_strs(&line("abcdefghij", 4)), ["abcd", "efgh", "ij"]);
        assert_eq!(row_strs(&line("abcd", 4)), ["abcd"]);
        assert_eq!(row_strs(&line("abcd\r", 4)), ["abcd"]);
        assert_eq!(row_strs(&line("", 4)), [""]);
        assert_eq!(row_strs(&line("a漢字b", 4)), ["a漢", "字b"]);
        // A grapheme wider than the view still gets a row to itself.
        assert_eq!(row_strs(&line("漢字", 1)), ["漢", "字"]);
    }

    #[test]
    fn unwrapped_view_is_one_row_per_line() {
        let (_, view) = setup(&["aaaaaaaa", "b", "c", "d"], 3, 4, false);
        assert_eq!(
            rows(&view),
            [(0, "aaaaaaaa".into()), (1, "b".into()), (2, "c".into())]
        );
        assert_eq!(view.bottom(), 3);
    }

    #[test]
    fn wrapped_view_fills_height_with_rows() {
        let (_, view) = setup(&["aaaaaaaa", "b", "cccccc", "d"], 4, 4, true);
        assert_eq!(
            rows(&view),
            [
                (0, "aaaa".into()),
                (0, "aaaa".into()),
                (1, "b".into()),
                (2, "cccc".into()),
            ]
        );
        // Line 2 is cut off, so it doesn't count as visible.
        assert_eq!(view.bottom(), 2);
    }

    #[test]
    fn pan_steps_through_wrapped_rows() {
        let (buf, mut view) = setup(&["aaaaaaaa", "b", "cccccc", "d"], 2, 4, true);

        view.pan_vertical(&buf, Direction::Next, 1);
        assert_eq!(rows(&view), [(0, "aaaa".into()), (1, "b".into())]);

        view.pan_vertical(&buf, Direction::Next, 2);
        assert_eq!(rows(&view), [(2, "cccc".into()), (2, "cc".into())]);

        view.pan_vertical(&buf, Direction::Next, 100);
        assert_eq!(view.view_bounds().top(), 3);
        assert_eq!(rows(&view), [(3, "d".into())]);

        view.pan_vertical(&buf, Direction::Back, 1);
        assert_eq!(rows(&view), [(2, "cc".into()), (3, "d".into())]);

        view.pan_vertical(&buf, Direction::Back, 3);
        assert_eq!(rows(&view), [(0, "aaaa".into()), (1, "b".into())]);

        view.pan_vertical(&buf, Direction::Back, 100);
        assert_eq!(rows(&view), [(0, "aaaa".into()), (0, "aaaa".into())]);
    }

    #[test]
    fn jump_places_wrapped_line_at_bottom() {
        let (buf, mut view) = setup(&["a", "b", "cccccc", "d", "eeeeeeeeee"], 3, 4, true);

        // Already visible: nothing moves.
        view.jump_vertically_to(&buf, 1);
        assert_eq!(view.view_bounds().top(), 0);

        view.jump_vertically_to(&buf, 3);
        assert_eq!(
            rows(&view),
            [(2, "cccc".into()), (2, "cc".into()), (3, "d".into())]
        );

        view.jump_vertically_to(&buf, 2);
        assert_eq!(view.bottom(), 4);

        // Exactly as tall as the view.
        view.jump_vertically_to(&buf, 4);
        assert_eq!(
            rows(&view),
            [(4, "eeee".into()), (4, "eeee".into()), (4, "ee".into())]
        );

        // Partially scrolled into the top line: jumping to it shows its start.
        view.jump_vertically_to(&buf, 2);
        view.pan_vertical(&buf, Direction::Next, 1);
        assert_eq!(rows(&view)[0], (2, "cc".into()));
        view.jump_vertically_to(&buf, 2);
        assert_eq!(rows(&view)[0], (2, "cccc".into()));
    }

    #[test]
    fn jump_into_line_taller_than_view_shows_its_start() {
        let (buf, mut view) = setup(&["a", "bbbbbbbbbbbbbbbb"], 2, 4, true);
        view.jump_vertically_to(&buf, 1);
        assert_eq!(rows(&view), [(1, "bbbb".into()), (1, "bbbb".into())]);
        assert_eq!(view.bottom(), 2);
    }

    #[test]
    fn refit_rewraps_lines() {
        let (buf, mut view) = setup(&["aaaaaaaa", "b"], 3, 4, true);
        view.pan_vertical(&buf, Direction::Next, 1);
        assert_eq!(rows(&view), [(0, "aaaa".into()), (1, "b".into())]);

        view.fit(3, 8, true, DEFAULT_TAB_WIDTH);
        view.populate(&buf, None);
        assert_eq!(rows(&view), [(0, "aaaaaaaa".into()), (1, "b".into())]);

        view.fit(3, 2, true, DEFAULT_TAB_WIDTH);
        view.populate(&buf, None);
        assert_eq!(
            rows(&view),
            [(0, "aa".into()), (0, "aa".into()), (0, "aa".into())]
        );

        view.fit(3, 2, false, DEFAULT_TAB_WIDTH);
        view.populate(&buf, None);
        assert_eq!(rows(&view), [(0, "aaaaaaaa".into()), (1, "b".into())]);
    }
}
