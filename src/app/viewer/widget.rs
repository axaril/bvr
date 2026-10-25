use crate::app::common::gutter::GutterType;
use crate::app::{
    actions::{Action, NormalAction},
    control::ViewDelta,
    mouse::EventHandler,
    viewer::{instance::Instance, virtual_view::CachedLine},
};
use crate::{app::actions::VisualAction, colors, direction::Direction};
use crossterm::event::MouseEventKind;
use ratatui::prelude::*;
use regex::bytes::Regex;
use std::ops::Range;
use crate::text::{MAX_TAB_WIDTH, clip_columns, grapheme_width};

pub struct ViewWidget<'a> {
    view_index: usize,
    instance: &'a mut Instance,
    show_selection: bool,
    gutter: Option<u16>,
    wrap: bool,
    tab_width: usize,
    regex: Option<(Color, &'a Regex)>,
    left: usize,
}

impl<'a> ViewWidget<'a> {
    pub fn new(
        view_index: usize,
        instance: &'a mut Instance,
        show_selection: bool,
        show_gutter: bool,
        wrap: bool,
        tab_width: usize,
        regex: Option<(Color, &'a Regex)>,
    ) -> Self {
        Self {
            view_index,
            show_selection,
            gutter: show_gutter
                .then(|| (instance.total_line_count().max(1).ilog10() as u16 + 1).max(4)),
            wrap,
            tab_width,
            left: if wrap {
                0
            } else {
                instance.view_bounds().left()
            },
            regex,
            instance,
        }
    }

    /// Width taken up by the gutter and selection columns, to the left of the line contents.
    fn data_offset(&self) -> u16 {
        if self.gutter.is_none() && !self.show_selection {
            0
        } else {
            self.gutter.unwrap_or(0) + ViewerLineWidget::SPECIAL_SIZE
        }
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut EventHandler) {
        let cursor_state = self.instance.cursor().state();

        self.instance.update_view_bounds(
            area.height as usize,
            area.width.saturating_sub(self.data_offset()) as usize,
            self.wrap,
            self.tab_width,
        );

        self.instance.populate_view();
        let view = self.instance.view_cache();

        // Matches of the line being rendered, found once on the whole line so
        // that wrapped rows and horizontal scrolling can't change them.
        let mut matches: Option<(usize, Vec<Range<usize>>)> = None;

        (area.y..area.bottom())
            .zip(view.map(Some).chain(std::iter::repeat(None)))
            .for_each(|(y, row_data)| {
                if let (Some((_, regex)), Some((line, _))) = (self.regex, row_data) {
                    if matches.as_ref().is_none_or(|(index, _)| *index != line.index) {
                        matches = Some((line.index, match_ranges(regex, &line.data)));
                    }
                }
                ViewerLineWidget {
                    parent: &self,
                    matches: matches.as_ref().map_or(&[], |(_, m)| m.as_slice()),
                    line_data: row_data.map(|(line, _)| line),
                    row: row_data.map_or(0, |(_, row)| row),
                    ty: row_data
                        .map(|(line, _)| GutterType::map_cursor_state(cursor_state, line.index))
                        .unwrap_or_default(),
                }
                .render(Rect::new(area.x, y, area.width, 1), buf, handle);
            });

        handle.on_mouse(area, |event| match event.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                Some(Action::Normal(NormalAction::PanVertical {
                    direction: Direction::back_if(event.kind == MouseEventKind::ScrollUp),
                    delta: ViewDelta::Number { value: 5 },
                    target_view: Some(self.view_index),
                }))
            }
            _ => None,
        });
    }
}

struct ViewerLineWidget<'a> {
    parent: &'a ViewWidget<'a>,

    line_data: Option<&'a CachedLine>,
    /// Byte ranges of `line_data` that match the highlighted regex.
    matches: &'a [Range<usize>],
    /// Visual row of `line_data` being rendered, nonzero for wrapped continuations.
    row: usize,
    ty: GutterType,
}

impl ViewerLineWidget<'_> {
    const SPECIAL_SIZE: u16 = 3;

    fn split_line(&self, area: Rect) -> (Option<Rect>, Option<Rect>, Rect) {
        if self.parent.gutter.is_none() && !self.parent.show_selection {
            return (None, None, area);
        }

        let gutter_size = self.parent.gutter.unwrap_or(0);
        let mut gutter_chunk = area;
        gutter_chunk.width = gutter_size;

        let mut cursor_chunk = area;
        cursor_chunk.x += gutter_size;
        cursor_chunk.width = 3;

        let mut data_chunk = area;
        data_chunk.x += gutter_size + Self::SPECIAL_SIZE;
        data_chunk.width = data_chunk
            .width
            .saturating_sub(gutter_size + Self::SPECIAL_SIZE);

        (Some(gutter_chunk), Some(cursor_chunk), data_chunk)
    }

    pub fn render(self, area: Rect, buf: &mut Buffer, handle: &mut EventHandler) {
        let (gutter_chunk, cursor_chunk, data_chunk) = self.split_line(area);

        let Some(line) = &self.line_data else {
            let ln = Line::raw("~")
                .alignment(Alignment::Right)
                .fg(colors::GUTTER_TEXT)
                .bg(colors::GUTTER_BG);

            if let Some(gutter_chunk) = gutter_chunk {
                ln.render(gutter_chunk, buf);
            }
            return;
        };

        if let Some(gutter_chunk) = gutter_chunk
            && self.row == 0
        {
            let mut itoa_buf = itoa::Buffer::new();
            let ln_str = itoa_buf.format(line.line_number + 1);
            let ln = Line::raw(ln_str).alignment(Alignment::Right).fg(
                if self.line_data.map(|l| l.bookmarked).unwrap_or(false) {
                    colors::SELECT_ACCENT
                } else {
                    colors::GUTTER_TEXT
                },
            );

            ln.render(gutter_chunk, buf);
        }

        if let Some(type_chunk) = cursor_chunk
            && self.parent.show_selection
        {
            let marker = if self.row == 0 {
                self.ty.to_gutter()
            } else {
                self.ty.to_continuation_gutter()
            };
            Span::raw(marker)
                .fg(colors::SELECT_ACCENT)
                .render(type_chunk, buf);
        }

        let row_start = line.row_start(self.row);
        let row = line.row(self.row);
        let left = self.parent.left;
        let clip = clip_columns(row, left, data_chunk.width as usize, self.parent.tab_width);
        let window = row_start + clip.range.start..row_start + clip.range.end;

        let mut style = Style::new().fg(line.color);
        if line.bookmarked {
            style = style.bg(colors::SELECT_ACCENT);
        }
        // The style covers the whole row, not just the text.
        buf.set_style(data_chunk, style);

        match self.parent.regex {
            Some((color, _)) if !self.matches.is_empty() => {
                let mut col = clip.start_col;
                for span in highlight_spans(&line.data, window, self.matches, color) {
                    let span_style = style.patch(span.style);
                    col = draw_text(
                        buf,
                        data_chunk,
                        left,
                        col,
                        self.parent.tab_width,
                        &span.content,
                        span_style,
                    );
                }
            }
            _ => {
                draw_text(
                    buf,
                    data_chunk,
                    left,
                    clip.start_col,
                    self.parent.tab_width,
                    &row[clip.range],
                    style,
                );
            }
        }

        if let Some(line) = self.line_data {
            handle.on_mouse(area, |event| match event.kind {
                MouseEventKind::Down(_) => Some(Action::Visual(VisualAction::ToggleLine {
                    line_number: line.line_number,
                    target_view: self.parent.view_index,
                })),
                _ => None,
            });
        }
    }
}

/// Spaces to fill a tab with.
const TAB_FILL: &str = "        ";
const _: () = assert!(MAX_TAB_WIDTH <= TAB_FILL.len());

/// Draws `text` into `chunk`, starting at column `col` of a row whose leftmost
/// visible column is `left`. Tabs are drawn as spaces up to the next tab stop.
/// Returns the column after the text.
fn draw_text(
    buf: &mut Buffer,
    chunk: Rect,
    left: usize,
    mut col: usize,
    tab_width: usize,
    text: &str,
    style: Style,
) -> usize {
    let draw = |buf: &mut Buffer, col: &mut usize, text: &str| {
        let x = chunk.x.saturating_add(col.saturating_sub(left) as u16);
        if x >= chunk.right() || text.is_empty() {
            return;
        }
        let (end, _) = buf.set_stringn(x, chunk.y, text, usize::from(chunk.right() - x), style);
        *col += usize::from(end - x);
    };

    let mut pieces = text.split('\t');
    if let Some(first) = pieces.next() {
        draw(buf, &mut col, first);
    }
    for piece in pieces {
        let fill = &TAB_FILL[..grapheme_width("\t", col, tab_width)];
        draw(buf, &mut col, fill);
        draw(buf, &mut col, piece);
    }
    col
}

/// Byte ranges of the non-empty matches of `regex` in the content of `line`.
fn match_ranges(regex: &Regex, line: &str) -> Vec<Range<usize>> {
    let content = line.strip_suffix('\n').unwrap_or(line);
    let content = content.strip_suffix('\r').unwrap_or(content);

    regex
        .find_iter(content.as_bytes())
        .map(|m| m.range())
        .filter(|range| {
            !range.is_empty()
                && content.is_char_boundary(range.start)
                && content.is_char_boundary(range.end)
        })
        .collect()
}

/// Splits `window` of `line` into spans, highlighting the parts covered by `matches`.
fn highlight_spans<'a>(
    line: &'a str,
    window: Range<usize>,
    matches: &[Range<usize>],
    color: Color,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut at = window.start;
    for m in matches {
        if m.end <= window.start {
            continue;
        }
        if m.start >= window.end {
            break;
        }
        let (start, end) = (m.start.max(window.start), m.end.min(window.end));
        if start > at {
            spans.push(Span::raw(&line[at..start]));
        }
        spans.push(Span::raw(&line[start..end]).bg(color));
        at = end;
    }
    if at < window.end {
        spans.push(Span::raw(&line[at..window.end]));
    }
    spans
}

#[cfg(test)]
mod test {
    use super::*;

    fn rendered(line: &str, window: Range<usize>, regex: &str) -> Vec<(String, bool)> {
        let matches = match_ranges(&Regex::new(regex).unwrap(), line);
        highlight_spans(line, window, &matches, Color::Red)
            .into_iter()
            .map(|span| (span.content.to_string(), span.style.bg.is_some()))
            .collect()
    }

    #[test]
    fn match_across_a_wrap_break_is_highlighted_on_both_rows() {
        let line = "abcdefghij";
        // Wrapped at 4 columns: "abcd", "efgh", "ij".
        assert_eq!(
            rendered(line, 0..4, "cdef"),
            [("ab".into(), false), ("cd".into(), true)]
        );
        assert_eq!(
            rendered(line, 4..8, "cdef"),
            [("ef".into(), true), ("gh".into(), false)]
        );
        assert_eq!(rendered(line, 8..10, "cdef"), [("ij".into(), false)]);
    }

    #[test]
    fn anchors_apply_to_the_line_not_the_row() {
        assert_eq!(
            rendered("abcdefgh", 4..8, "^e"),
            [("efgh".into(), false)]
        );
        assert_eq!(
            rendered("abcdefgh", 0..4, "^a"),
            [("a".into(), true), ("bcd".into(), false)]
        );
        assert_eq!(
            rendered("abcdefgh\n", 4..9, "h$"),
            [("efg".into(), false), ("h".into(), true), ("\n".into(), false)]
        );
    }

    fn drawn(text: &str, left: usize, col: usize, width: u16, regex: &str) -> (String, Vec<bool>) {
        let chunk = Rect::new(0, 0, width, 1);
        let mut buf = Buffer::empty(chunk);
        let matches = match_ranges(&Regex::new(regex).unwrap(), text);

        let mut at = col;
        for span in highlight_spans(text, 0..text.len(), &matches, Color::Red) {
            at = draw_text(&mut buf, chunk, left, at, 4, &span.content, span.style);
        }

        let cells = (0..width).map(|x| &buf[(x, 0)]).collect::<Vec<_>>();
        (
            cells.iter().map(|cell| cell.symbol()).collect(),
            cells.iter().map(|cell| cell.bg == Color::Red).collect(),
        )
    }

    #[test]
    fn tabs_are_drawn_as_spaces_up_to_the_tab_stop() {
        assert_eq!(drawn("a\tb\tc", 0, 0, 10, "q").0, "a   b   c ");
        assert_eq!(drawn("ab\tc", 0, 0, 6, "q").0, "ab  c ");
        // The text starts later when the character at the left edge is left out.
        assert_eq!(drawn("b\tc", 2, 4, 7, "q").0, "  b   c");
        assert_eq!(drawn("b\tc", 4, 4, 6, "q").0, "b   c ");
    }

    #[test]
    fn highlighted_match_covers_the_tab_it_spans() {
        let (text, highlighted) = drawn("a\tb\tc", 0, 0, 9, "\tb\t");
        assert_eq!(text, "a   b   c");
        assert_eq!(
            highlighted,
            [false, true, true, true, true, true, true, true, false]
        );
        // Cut off by the edge of the view.
        let (text, highlighted) = drawn("a\tb", 0, 0, 3, "\t");
        assert_eq!(text, "a  ");
        assert_eq!(highlighted, [false, true, true]);
    }

    #[test]
    fn every_match_is_highlighted() {
        assert_eq!(
            rendered("abab", 0..4, "b"),
            [
                ("a".into(), false),
                ("b".into(), true),
                ("a".into(), false),
                ("b".into(), true),
            ]
        );
    }

    #[test]
    fn empty_matches_and_split_chars_are_skipped() {
        assert!(match_ranges(&Regex::new("x*").unwrap(), "abc").is_empty());
        assert!(match_ranges(&Regex::new("(?-u:\\xE6)").unwrap(), "漢").is_empty());
    }
}
