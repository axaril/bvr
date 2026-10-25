use unicode_segmentation::UnicodeSegmentation;
use std::ops::Range;
use unicode_width::UnicodeWidthStr;

/// Default number of columns between tab stops. Tabs are shown as spaces up to the next one.
pub const DEFAULT_TAB_WIDTH: usize = 4;

/// Largest number of columns between tab stops.
pub const MAX_TAB_WIDTH: usize = 8;

/// Columns `grapheme` takes when it starts at column `col`, with a tab stop
/// every `tab_width` columns.
///
/// Control characters take none, since they are not drawn. Tabs are the exception,
/// they fill up to the next tab stop.
#[inline]
pub fn grapheme_width(grapheme: &str, col: usize, tab_width: usize) -> usize {
    if grapheme == "\t" {
        tab_width - col % tab_width
    } else if grapheme.chars().next().is_some_and(char::is_control) {
        0
    } else {
        grapheme.width()
    }
}

/// Whether every byte of `s` is a printable ASCII character, which is exactly one
/// column wide and one grapheme. Checking this is much cheaper than segmenting `s`.
pub fn is_printable_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// Whether `s` is only printable ASCII and tabs, so each byte is one grapheme.
fn is_simple_ascii(s: &str) -> bool {
    s.bytes().all(|b| b == b'\t' || (0x20..0x7f).contains(&b))
}

/// The bytes of ASCII `s` as graphemes, which avoids segmenting it.
fn ascii_graphemes(s: &str) -> impl Iterator<Item = (usize, &str)> {
    (0..s.len()).map(move |i| (i, &s[i..=i]))
}

/// Strips the line terminator off the end of `line`.
pub fn strip_terminator(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

/// The part of a row that is displayed.
#[derive(Debug, PartialEq, Eq)]
pub struct Clip {
    /// Byte range of the row.
    pub range: Range<usize>,
    /// Column of the first grapheme in `range`. That is after the left edge when
    /// the grapheme at the edge was left out.
    pub start_col: usize,
}

/// The part of `row` that is displayed in the columns `left..left + width`.
///
/// A grapheme that straddles either edge is left out. Columns count from the
/// start of the row, so that tabs stay aligned however far the row is scrolled.
/// The line terminator takes no columns, so it is kept when the end of the row
/// is in range.
pub fn clip_columns(row: &str, left: usize, width: usize, tab_width: usize) -> Clip {
    let content = strip_terminator(row);

    if is_printable_ascii(content) {
        let range = if left >= content.len() {
            row.len()..row.len()
        } else {
            let end = (left + width).min(content.len());
            left..if end == content.len() { row.len() } else { end }
        };
        return Clip {
            range,
            start_col: left,
        };
    }

    // The terminator is ASCII too, so it doesn't change how the row is segmented.
    if is_simple_ascii(content) {
        clip_graphemes(row, left, width, tab_width, ascii_graphemes(row))
    } else {
        clip_graphemes(row, left, width, tab_width, row.grapheme_indices(true))
    }
}

fn clip_graphemes<'a>(
    row: &str,
    left: usize,
    width: usize,
    tab_width: usize,
    graphemes: impl Iterator<Item = (usize, &'a str)>,
) -> Clip {
    let mut graphemes = graphemes.peekable();
    let mut col = 0;
    while let Some(&(_, grapheme)) = graphemes.peek() {
        if col >= left {
            break;
        }
        col += grapheme_width(grapheme, col, tab_width);
        graphemes.next();
    }
    let start = graphemes.peek().map_or(row.len(), |&(idx, _)| idx);
    let start_col = col;

    let limit = left + width;
    let mut end = row.len();
    for (idx, grapheme) in graphemes {
        let grapheme_width = grapheme_width(grapheme, col, tab_width);
        if col + grapheme_width > limit {
            end = idx;
            break;
        }
        col += grapheme_width;
    }

    Clip {
        range: start..end,
        start_col,
    }
}

/// Byte offsets of `line` at which each row after the first one begins, when it
/// is wrapped at `width` columns.
pub fn wrap_points(line: &str, width: usize, tab_width: usize) -> Vec<usize> {
    // The line terminator stays on the last row rather than spilling onto a row of its own.
    let content = strip_terminator(line);

    // Every character takes a column, so rows are simply `width` bytes long.
    if is_printable_ascii(content) {
        return (width..content.len()).step_by(width).collect();
    }

    if is_simple_ascii(content) {
        wrap_graphemes(width, tab_width, ascii_graphemes(content))
    } else {
        wrap_graphemes(width, tab_width, content.grapheme_indices(true))
    }
}

fn wrap_graphemes<'a>(
    width: usize,
    tab_width: usize,
    graphemes: impl Iterator<Item = (usize, &'a str)>,
) -> Vec<usize> {
    let mut breaks = Vec::new();
    let mut col = 0;
    for (idx, grapheme) in graphemes {
        let mut grapheme_width = grapheme_width(grapheme, col, tab_width);
        if col + grapheme_width > width && col > 0 {
            breaks.push(idx);
            col = 0;
            // A tab is as wide as the column it lands on.
            grapheme_width = self::grapheme_width(grapheme, col, tab_width);
        }
        col += grapheme_width;
    }
    breaks
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

    const TAB: usize = DEFAULT_TAB_WIDTH;

    fn clip(row: &str, left: usize, width: usize) -> &str {
        &row[clip_columns(row, left, width, TAB).range]
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
        // The text starts at column 2, so only columns 2 to 4 are left of the right edge.
        assert_eq!(clip("漢字漢字", 1, 4), "字");
        assert_eq!(clip("漢字漢字", 1, 5), "字漢");
        assert_eq!(clip("a漢b", 0, 2), "a");
        assert_eq!(clip("a漢b", 1, 5), "漢b");
    }

    #[test]
    fn clip_reports_where_the_text_starts() {
        let start_col = |row, left| clip_columns(row, left, 20, TAB).start_col;
        assert_eq!(start_col("abcdef", 2), 2);
        // The wide character at the edge is left out, so the text starts a column late.
        assert_eq!(start_col("漢字漢字", 1), 2);
        assert_eq!(start_col("a\tb", 2), 4);
    }

    #[test]
    fn tabs_fill_up_to_the_next_tab_stop() {
        // a . . . b . . . c, with tabs taking 3 and 3 columns.
        let row = "a\tb\tc";
        assert_eq!(clip(row, 0, 20), row);
        assert_eq!(clip(row, 0, 5), "a\tb");
        // The second tab would end at column 8.
        assert_eq!(clip(row, 0, 7), "a\tb");
        assert_eq!(clip(row, 0, 8), "a\tb\t");
        assert_eq!(clip(row, 0, 9), row);
        // Tab stops count from the start of the row, not from the left edge.
        assert_eq!(clip(row, 4, 20), "b\tc");
        assert_eq!(clip(row, 5, 20), "\tc");
        // A tab that straddles the left edge is left out.
        assert_eq!(clip(row, 2, 20), "b\tc");
        assert_eq!(clip("\t\tx", 0, 20), "\t\tx");
        assert_eq!(clip("\t\tx", 5, 20), "x");
        assert_eq!(clip("\t\tx", 3, 20), "\tx");
    }

    #[test]
    fn simple_ascii_agrees_with_segmentation() {
        for row in [
            "the quick brown fox
",
            "the quick brown fox
",
            "the\tquick
",
            "\t\ta\tbc\td",
            "short",
            "
",
            "",
        ] {
            for left in 0..24 {
                for width in 0..24 {
                    let fast = clip_columns(row, left, width, TAB);
                    let slow = clip_graphemes(row, left, width, TAB, row.grapheme_indices(true));
                    // Only the terminator can differ, and where nothing is drawn, the start.
                    let (fast_text, slow_text) = (&row[fast.range], &row[slow.range]);
                    assert_eq!(
                        strip_terminator(fast_text),
                        strip_terminator(slow_text),
                        "{row:?} {left} {width}"
                    );
                    if !strip_terminator(fast_text).is_empty() {
                        assert_eq!(fast.start_col, slow.start_col, "{row:?} {left} {width}");
                    }
                }
            }
            for width in 1..12 {
                let content = strip_terminator(row);
                assert_eq!(
                    wrap_points(row, width, TAB),
                    wrap_graphemes(width, TAB, content.grapheme_indices(true)),
                    "{row:?} {width}"
                );
            }
        }
    }

    #[test]
    fn wraps_around_tabs() {
        assert_eq!(wrap_points("ab\tcd", 4, TAB), [3]);
        assert_eq!(wrap_points("abc\td", 4, TAB), [4]);
        // A tab that doesn't fit moves to the next row, where it takes a full stop.
        assert_eq!(wrap_points("abcd\te", 4, TAB), [4, 5]);
        assert_eq!(wrap_points("\t\t", 4, TAB), [1]);
    }

    #[test]
    fn tab_width_is_adjustable() {
        assert_eq!(grapheme_width("\t", 0, 8), 8);
        assert_eq!(grapheme_width("\t", 3, 8), 5);
        assert_eq!(grapheme_width("\t", 3, 1), 1);
        assert_eq!(wrap_points("a\tb", 2, 2), [2]);
        assert_eq!(&"a\tb"[clip_columns("a\tb", 0, 7, 8).range], "a");
    }

    #[test]
    fn control_characters_take_no_columns() {
        assert_eq!(grapheme_width("", 0, TAB), 0);
        assert_eq!(grapheme_width("
", 3, TAB), 0);
        assert_eq!(grapheme_width("\t", 0, TAB), 4);
        assert_eq!(grapheme_width("\t", 3, TAB), 1);
        assert_eq!(grapheme_width("a", 3, TAB), 1);
        assert_eq!(grapheme_width("漢", 3, TAB), 2);
    }

    #[test]
    fn simple_ascii_is_printable_and_tabs() {
        assert!(!is_printable_ascii("a\tb"));
        assert!(is_simple_ascii("a\tb"));
        assert!(!is_simple_ascii("ab"));
        assert!(!is_simple_ascii("é"));
        assert!(is_printable_ascii("a b~"));
    }
}
