use std::borrow::Cow;

use ratatui::prelude::{Color, Span, Style};

mod command;
mod regex;
mod selection;

pub use command::CommandHighlighter;
pub use regex::RegexHighlighter;
pub use selection::SelectionHighlighter;

struct Highlighter<'a> {
    input: &'a str,
    i: usize,
    spans: Vec<Span<'a>>,
}

struct ColorableSpan<'r, 'a> {
    content: &'a str,
    span: &'r mut Span<'a>,
}

impl ColorableSpan<'_, '_> {
    fn fg(self, color: Color) -> Self {
        self.span.style.fg = Some(color);
        self
    }

    fn bg(self, color: Color) -> Self {
        self.span.style.bg = Some(color);
        self
    }
}

impl<'a> Highlighter<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            i: 0,
            spans: Vec::new(),
        }
    }

    fn current(&self) -> Option<char> {
        self.input[self.i..].chars().next()
    }

    fn peek(&self) -> Option<char> {
        self.input[self.i..].chars().skip(1).next()
    }

    fn eat(&mut self, len: usize) -> &'a str {
        let content = &self.input[self.i..][..len];
        self.i += len;
        content
    }

    fn scan_bytes_until(&self, pred: impl FnMut(u8) -> bool) -> Option<usize> {
        self.input.as_bytes()[self.i..]
            .iter()
            .copied()
            .position(pred)
    }

    fn eat_and_color(&mut self, len: usize) -> ColorableSpan<'_, 'a> {
        let content = self.eat(len);
        ColorableSpan {
            content,
            span: self.spans.push_mut(Span::raw(content)),
        }
    }

    fn eat_remaining_and_color(&mut self) -> Option<ColorableSpan<'_, 'a>> {
        let len = self.input.len() - self.i;
        (len > 0).then_some(self.eat_and_color(len))
    }

    fn extract(self) -> Vec<Span<'a>> {
        self.spans
    }
}

/// Returns the byte index in `s` that corresponds to column offset `col`.
///
/// Walks characters, accumulating their display widths. Returns `s.len()` when
/// `col` is at or past the end of the string.
fn col_to_byte(s: &str, col: usize) -> usize {
    use unicode_width::UnicodeWidthChar;
    let mut c = 0;
    for (i, ch) in s.char_indices() {
        if c >= col {
            return i;
        }
        c += ch.width().unwrap_or(0);
    }
    s.len()
}

/// Borrows (or clones) the sub-content of `span` covering columns `[col_start, col_end)`.
fn span_content_slice<'a>(span: &Span<'a>, col_start: usize, col_end: usize) -> Cow<'a, str> {
    if col_start == 0 && col_end == span.width() {
        return span.content.clone();
    }
    let s = span.content.as_ref();
    let bs = col_to_byte(s, col_start);
    let be = col_to_byte(s, col_end);
    match &span.content {
        Cow::Borrowed(s) => Cow::Borrowed(&s[bs..be]),
        Cow::Owned(s) => Cow::Owned(s[bs..be].to_owned()),
    }
}

/// Attempts to extend `base` in-place by appending `extra`, provided both are
/// borrowed slices that are adjacent in memory. Returns `true` on success.
fn try_extend_borrowed<'a>(base: &mut Cow<'a, str>, extra: &Cow<'a, str>) -> bool {
    let (Cow::Borrowed(a), Cow::Borrowed(b)) = (&*base, extra) else {
        return false;
    };
    if a.as_ptr() as usize + a.len() != b.as_ptr() as usize {
        return false;
    }
    // SAFETY: `a` and `b` are adjacent slices of the same `&'a str`, so their
    // concatenation is valid UTF-8 with lifetime `'a`.
    *base = Cow::Borrowed(unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(a.as_ptr(), a.len() + b.len()))
    });
    true
}

/// Merges two consecutive slices of [`Span`]s into a single `Vec<Span<'a>>`.
///
/// Column ranges are derived from [`Span::width`]: span[0] covers `[0, w0)`,
/// span[1] covers `[w0, w0+w1)`, and so on.
///
/// When the column ranges from `a` and `b` overlap:
///
/// - sub-ranges covered only by `a` → keep `a`'s style, `a`'s content
/// - sub-ranges covered only by `b` → keep `b`'s style, `b`'s content
/// - sub-ranges covered by both     → `resolve(style_from_a, style_from_b)`, `a`'s content
///
/// **Panics** if overlapping sub-ranges carry different text content.
///
/// Adjacent output spans with identical styles whose content is contiguous in
/// memory are coalesced into a single span without allocating.
pub fn merge_spans<'a>(
    a: &[Span<'a>],
    b: &[Span<'a>],
    resolve: impl Fn(Style, Style) -> Style,
) -> Vec<Span<'a>> {
    let mut result: Vec<Span<'a>> = Vec::new();

    // Index and column-start of the current span in each list.
    let mut ai = 0usize;
    let mut bi = 0usize;
    let mut pos_a = 0usize;
    let mut pos_b = 0usize;

    // The sweep position always equals the end of the last emitted segment.
    let mut cur = 0usize;

    loop {
        // Skip zero-width spans; they occupy no columns and produce no range.
        while ai < a.len() && a[ai].width() == 0 {
            ai += 1;
        }
        while bi < b.len() && b[bi].width() == 0 {
            bi += 1;
        }

        // The next boundary is the nearer span-end. `usize::MAX` means exhausted.
        let end_a = if ai < a.len() {
            pos_a + a[ai].width()
        } else {
            usize::MAX
        };
        let end_b = if bi < b.len() {
            pos_b + b[bi].width()
        } else {
            usize::MAX
        };
        let next = end_a.min(end_b);

        if next == usize::MAX {
            break; // both lists exhausted
        }

        // Content for [cur, next) — taken from `a` when available, else `b`.
        let content = if ai < a.len() {
            span_content_slice(&a[ai], cur - pos_a, next - pos_a)
        } else {
            span_content_slice(&b[bi], cur - pos_b, next - pos_b)
        };

        // When both lists cover this segment their content must be identical.
        if ai < a.len() && bi < b.len() {
            let content_b = span_content_slice(&b[bi], cur - pos_b, next - pos_b);
            assert_eq!(
                content, content_b,
                "overlapping spans at columns {cur}..{next} have different content",
            );
        }

        // Styles active over [cur, next).
        let style_a = (ai < a.len()).then(|| a[ai].style);
        let style_b = (bi < b.len()).then(|| b[bi].style);

        let style = match (style_a, style_b) {
            (Some(sa), Some(sb)) => resolve(sa, sb),
            (Some(sa), None) => sa,
            (None, Some(sb)) => sb,
            (None, None) => unreachable!("at least one list is active"),
        };

        // Coalesce with the previous span when it has the same style and its
        // content is a borrowed slice that is contiguous with the new content.
        if let Some(last) = result.last_mut()
            && last.style == style
            && try_extend_borrowed(&mut last.content, &content)
        {
            // extended in-place, nothing more to do
        } else {
            result.push(Span::styled(content, style));
        }

        cur = next;

        // Advance whichever span(s) ended at `next`.
        if end_a <= end_b {
            pos_a = end_a;
            ai += 1;
        }
        if end_b <= end_a {
            pos_b = end_b;
            bi += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::prelude::Color;

    // Shared source string — all test spans are subslices of this, so adjacent
    // segments produced by the sweep are pointer-contiguous and can be coalesced.
    const SRC: &str = "xxxxxxxxxx"; // 10 ASCII chars; width == byte-length

    /// Borrows a sub-slice of SRC as a Span with the given style.
    fn s(start: usize, end: usize, style: Style) -> Span<'static> {
        Span::styled(&SRC[start..end], style)
    }

    fn fg(color: Color) -> Style {
        Style::new().fg(color)
    }

    /// `b` overlays on top of `a`: fields set in `b` win; unset fields fall back to `a`.
    fn overlay(a: Style, b: Style) -> Style {
        a.patch(b)
    }

    #[test]
    fn same_boundaries_full_overlap() {
        // a: [0,5) Red   b: [0,5) Blue  — resolved everywhere
        let a = [s(0, 5, fg(Color::Red))];
        let b = [s(0, 5, fg(Color::Blue))];
        let got = merge_spans(&a, &b, overlay);
        assert_eq!(got, [Span::styled("xxxxx", fg(Color::Blue))]);
    }

    #[test]
    fn different_split_points() {
        // a: [0,3) Red  [3,7) Green
        // b: [0,5) Blue [5,7) Cyan
        // [0,3): Red.patch(Blue)   = Blue
        // [3,5): Green.patch(Blue) = Blue  → pointer-adjacent to [0,3) → coalesced
        // [5,7): Green.patch(Cyan) = Cyan
        let a = [s(0, 3, fg(Color::Red)), s(3, 7, fg(Color::Green))];
        let b = [s(0, 5, fg(Color::Blue)), s(5, 7, fg(Color::Cyan))];
        let got = merge_spans(&a, &b, overlay);
        assert_eq!(
            got,
            [
                Span::styled("xxxxx", fg(Color::Blue)),
                Span::styled("xx", fg(Color::Cyan))
            ]
        );
    }

    #[test]
    fn a_contains_b() {
        // a: [0,10) Red
        // b: [0,3) default  [3,7) Blue  [7,10) default
        // [0,3):  Red.patch(default) = Red
        // [3,7):  Red.patch(Blue)    = Blue
        // [7,10): Red.patch(default) = Red
        let a = [s(0, 10, fg(Color::Red))];
        let b = [
            s(0, 3, Style::default()),
            s(3, 7, fg(Color::Blue)),
            s(7, 10, Style::default()),
        ];
        let got = merge_spans(&a, &b, overlay);
        assert_eq!(
            got,
            [
                Span::styled("xxx", fg(Color::Red)),
                Span::styled("xxxx", fg(Color::Blue)),
                Span::styled("xxx", fg(Color::Red)),
            ]
        );
    }

    #[test]
    fn adjacent_same_style_coalesced() {
        // a: [0,3) Green  [3,6) default
        // b: [0,3) default  [3,6) Green
        // [0,3): Green.patch(default) = Green
        // [3,6): default.patch(Green) = Green → pointer-adjacent to [0,3) → coalesced
        let green = fg(Color::Green);
        let a = [s(0, 3, green), s(3, 6, Style::default())];
        let b = [s(0, 3, Style::default()), s(3, 6, green)];
        let got = merge_spans(&a, &b, overlay);
        assert_eq!(got, [Span::styled("xxxxxx", green)]);
    }

    #[test]
    fn empty_inputs() {
        let got = merge_spans(&[s(0, 5, fg(Color::Red))], &[], overlay);
        assert_eq!(got, [Span::styled("xxxxx", fg(Color::Red))]);

        let got2 = merge_spans(&[], &[s(0, 5, fg(Color::Blue))], overlay);
        assert_eq!(got2, [Span::styled("xxxxx", fg(Color::Blue))]);
    }

    #[test]
    fn multiple_spans_multiple_overlaps() {
        // a: [0,4) Red  [4,6) default  [6,10) Red
        // b: [0,2) default  [2,8) Blue  [8,10) default
        // [0,2):  Red.patch(default)  = Red
        // [2,4):  Red.patch(Blue)     = Blue
        // [4,6):  default.patch(Blue) = Blue → pointer-adjacent → coalesced with [2,4)
        // [6,8):  Red.patch(Blue)     = Blue → pointer-adjacent → coalesced with [2,6)
        // [8,10): Red.patch(default)  = Red
        let a = [
            s(0, 4, fg(Color::Red)),
            s(4, 6, Style::default()),
            s(6, 10, fg(Color::Red)),
        ];
        let b = [
            s(0, 2, Style::default()),
            s(2, 8, fg(Color::Blue)),
            s(8, 10, Style::default()),
        ];
        let got = merge_spans(&a, &b, overlay);
        assert_eq!(
            got,
            [
                Span::styled("xx", fg(Color::Red)),
                Span::styled("xxxxxx", fg(Color::Blue)),
                Span::styled("xx", fg(Color::Red)),
            ]
        );
    }

    #[test]
    #[should_panic(expected = "different content")]
    fn mismatched_content_panics() {
        // "hello" and "world" are the same width but different text.
        let a = [Span::styled("hello", Style::default())];
        let b = [Span::styled("world", Style::default())];
        merge_spans(&a, &b, overlay);
    }
}
