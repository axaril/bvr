use ratatui::{prelude::{Color, Span}, text::Line};

pub mod regex;
pub mod command;

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
        self.input[self.i + 1..].chars().next()
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

    fn extract(self) -> Line<'a> {
        Line::from(self.spans)
    }
}
