use crate::direction::Direction;

#[derive(Clone, Copy)]
pub struct ViewBounds {
    /// Top of the view
    top: usize,
    /// Left of the view
    left: usize,
    /// Visible height
    height: usize,
    /// Visible width
    width: usize,
}

impl ViewBounds {
    #[inline]
    pub const fn new() -> Self {
        Self {
            top: 0,
            left: 0,
            height: 0,
            width: 0,
        }
    }

    #[inline(always)]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline(always)]
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn fit(&mut self, height: usize, width: usize) {
        self.height = height;
        self.width = width;
    }

    #[inline(always)]
    pub fn left(&self) -> usize {
        self.left
    }

    #[inline(always)]
    pub fn top(&self) -> usize {
        self.top
    }

    #[inline(always)]
    pub fn right(&self) -> usize {
        self.left + self.width
    }

    #[inline(always)]
    pub fn bottom(&self) -> usize {
        self.top + self.height
    }

    pub fn clamp(&mut self, end_index: usize) {
        if self.top >= end_index {
            self.top = end_index.saturating_sub(1);
        }
    }

    pub fn top_to(&mut self, index: usize) {
        self.top = index;
    }

    fn jump_to(pos: &mut usize, len: usize, index: usize) {
        let start = *pos;
        let end = start + len;
        if !(start..end).contains(&index) {
            if start.abs_diff(index) < end.abs_diff(index) {
                *pos = index;
            } else {
                *pos = index.saturating_sub(len).saturating_add(1);
            }
        }
    }

    fn pan(pos: &mut usize, direction: Direction, delta: usize) -> usize {
        let old = *pos;
        *pos = match direction {
            Direction::Back => (*pos).saturating_sub(delta),
            Direction::Next => (*pos).saturating_add(delta),
        };
        pos.abs_diff(old)
    }

    pub fn jump_vertically_to(&mut self, index: usize) {
        Self::jump_to(&mut self.top, self.height, index)
    }

    pub fn jump_horizontally_to(&mut self, index: usize) {
        Self::jump_to(&mut self.left, self.width, index)
    }

    pub fn pan_vertical(&mut self, direction: Direction, delta: usize) -> usize {
        Self::pan(&mut self.top, direction, delta)
    }

    pub fn pan_horizontal(&mut self, direction: Direction, delta: usize) -> usize {
        Self::pan(&mut self.left, direction, delta)
    }
}
