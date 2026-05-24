use ratatui::prelude::*;

pub mod regex;

struct Highlighter<'a> {
    input: &'a str,
    i: usize,
    spans: Vec<Span<'a>>,
}

impl<'a> Highlighter<'a> {
    pub fn new(input: &'a str) -> Self {
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

    fn eat_and_color(&mut self, len: usize, color: Color) {
        let token = self.eat(len);
        self.spans.push(Span::raw(token).fg(color));
    }
}
