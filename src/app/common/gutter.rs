use bitflags::bitflags;

use crate::cursor::{Cursor, CursorAnchor};

bitflags! {
    pub struct GutterType: u8 {
        const None = 0;
        const Origin = 1 << 0;
        const OriginStart = 1 << 1;
        const OriginEnd = 1 << 2;
        const Within = 1 << 3;
        const Bookmarked = 1 << 4;
    }
}

impl Default for GutterType {
    fn default() -> Self {
        Self::None
    }
}

impl GutterType {
    pub fn map_cursor_state(cursor_state: Cursor, index: usize) -> Self {
        match cursor_state {
            Cursor::Singleton(i) => {
                if index == i {
                    GutterType::Origin
                } else {
                    GutterType::None
                }
            }
            Cursor::Selection(start, end, anchor) => {
                if !(start..=end).contains(&index) {
                    GutterType::None
                } else if index == start {
                    if anchor == CursorAnchor::End {
                        GutterType::Origin | GutterType::OriginStart
                    } else {
                        GutterType::OriginStart
                    }
                } else if index == end {
                    if anchor == CursorAnchor::Start {
                        GutterType::Origin | GutterType::OriginEnd
                    } else {
                        GutterType::OriginEnd
                    }
                } else {
                    GutterType::Within
                }
            }
        }
    }

    pub fn to_gutter(self, empty: &'static str) -> &'static str {
        if self.contains(GutterType::Origin) {
            if self.contains(GutterType::OriginStart) {
                " ┍ "
            } else if self.contains(GutterType::OriginEnd) {
                " ┕ "
            } else {
                " ▶ "
            }
        } else if self.contains(GutterType::OriginStart) {
            " ┌ "
        } else if self.contains(GutterType::OriginEnd) {
            " └ "
        } else if self.contains(GutterType::Within) {
            " │ "
        } else {
            empty
        }
    }
}
