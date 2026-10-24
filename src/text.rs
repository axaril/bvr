use unicode_segmentation::UnicodeSegmentation;
use std::ops::Range;
use unicode_width::UnicodeWidthStr;

/// Whether every byte of `s` is a printable ASCII character, which is exactly one
/// column wide and one grapheme. Checking this is much cheaper than segmenting `s`.
pub fn is_printable_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// Strips the line terminator off the end of `line`.
pub fn strip_terminator(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

/// Byte range of `row` that is displayed in the columns `left..left + width`.
///
/// A wide grapheme that straddles either edge is left out. The line terminator
/// takes no columns, so it is kept when the end of the row is in range.
pub fn clip_columns(row: &str, left: usize, width: usize) -> Range<usize> {
    let content = strip_terminator(row);

    if is_printable_ascii(content) {
        if left >= content.len() {
            return row.len()..row.len();
        }
        let end = (left + width).min(content.len());
        return left..if end == content.len() { row.len() } else { end };
    }

    clip_columns_by_width(row, left, width)
}

fn clip_columns_by_width(row: &str, left: usize, width: usize) -> Range<usize> {
    let mut graphemes = row.grapheme_indices(true).peekable();
    let mut col = 0;
    while let Some(&(_, grapheme)) = graphemes.peek() {
        if col >= left {
            break;
        }
        col += grapheme.width();
        graphemes.next();
    }
    let start = graphemes.peek().map_or(row.len(), |&(idx, _)| idx);

    let mut used = 0;
    let mut end = row.len();
    for (idx, grapheme) in graphemes {
        let grapheme_width = grapheme.width();
        if used + grapheme_width > width {
            end = idx;
            break;
        }
        used += grapheme_width;
    }
    start..end
}

#[allow(dead_code)]
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for grapheme in text.graphemes(true) {
        let grapheme_width = grapheme.width();

        if current_width + grapheme_width > max_width && !current_line.is_empty() {
            lines.push(std::mem::take(&mut current_line));
            current_width = 0;
        }

        current_line.push_str(grapheme);
        current_width += grapheme_width;
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

#[cfg(test)]
mod test {
    use super::*;

    fn clip(row: &str, left: usize, width: usize) -> &str {
        &row[clip_columns(row, left, width)]
    }

    #[test]
    fn clips_ascii_by_column() {
        assert_eq!(clip("abcdefgh\n", 0, 4), "abcd");
        assert_eq!(clip("abcdefgh\n", 2, 4), "cdef");
        assert_eq!(clip("abcdefgh\n", 4, 4), "efgh\n");
        assert_eq!(clip("abcdefgh\n", 6, 10), "gh\n");
        assert_eq!(clip("abcdefgh\n", 8, 4), "");
        assert_eq!(clip("abcdefgh", 100, 4), "");
        assert_eq!(clip("", 0, 4), "");
    }

    #[test]
    fn clips_wide_text_by_display_width() {
        // Each of these characters takes two columns.
        assert_eq!(clip("漢字漢字", 0, 4), "漢字");
        assert_eq!(clip("漢字漢字", 2, 4), "字漢");
        // Wide graphemes that straddle an edge are left out.
        assert_eq!(clip("漢字漢字", 0, 3), "漢");
        assert_eq!(clip("漢字漢字", 1, 4), "字漢");
        assert_eq!(clip("a漢b", 0, 2), "a");
        assert_eq!(clip("a漢b", 1, 5), "漢b");
    }

    #[test]
    fn fast_path_agrees_with_segmentation() {
        for row in ["the quick brown fox\n", "the quick brown fox\r\n", "short", "\n", ""] {
            for left in 0..24 {
                for width in 0..24 {
                    // The slow path has no terminator special case, only compare content.
                    let fast = &row[clip_columns(row, left, width)];
                    let slow = &row[clip_columns_by_width(row, left, width)];
                    assert_eq!(
                        strip_terminator(fast),
                        strip_terminator(slow),
                        "{row:?} {left} {width}"
                    );
                }
            }
        }
    }

    #[test]
    fn control_characters_avoid_the_fast_path() {
        assert!(!is_printable_ascii("a\tb"));
        assert!(is_printable_ascii("a b~"));
    }
}
