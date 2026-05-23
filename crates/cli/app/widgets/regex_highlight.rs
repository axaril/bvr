use crate::colors;
use ratatui::prelude::*;

pub(super) struct RegexHighlighter<'a> {
    input: &'a str,
    bytes: &'a [u8],

    i: usize,

    depth: usize,
    in_class: bool,
    lit_start: Option<usize>,

    spans: Vec<Span<'a>>,

    group_stack: Vec<usize>,
    class_open_idx: Option<usize>,
    class_negation_idx: Option<usize>,
}

impl<'a> RegexHighlighter<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            i: 0,
            depth: 0,
            in_class: false,
            lit_start: None,
            spans: Vec::new(),
            group_stack: Vec::new(),
            class_open_idx: None,
            class_negation_idx: None,
        }
    }

    pub fn highlight(mut self) -> Line<'a> {
        let len = self.bytes.len();
        while self.i < len {
            let ch = self.input[self.i..].chars().next().unwrap();
            let ch_len = ch.len_utf8();
            if self.in_class {
                self.step_in_class(ch, ch_len);
            } else {
                self.step_outside_class(ch, ch_len);
            }
        }
        self.flush_lit(len);
        self.finalize()
    }

    /// Flush any accumulated literal characters as an unstyled span.
    fn flush_lit(&mut self, end: usize) {
        if let Some(s) = self.lit_start.take() {
            if s < end {
                self.spans.push(Span::raw(&self.input[s..end]));
            }
        }
    }

    /// Advance one character while inside a `[…]` character class.
    fn step_in_class(&mut self, ch: char, ch_len: usize) {
        let input = self.input; // Copy — does not borrow self
        let bytes = self.bytes;
        let i = self.i;
        let len = bytes.len();

        match ch {
            '\\' if i + 1 < len => {
                self.flush_lit(i);
                let nc = input[i + 1..].chars().next().unwrap();
                let end = i + 1 + nc.len_utf8();
                self.spans.push(Span::raw(&input[i..end]).fg(colors::regex::ESCAPE));
                self.i = end;
            }
            ']' => {
                self.flush_lit(i);
                self.spans.push(Span::raw(&input[i..i + 1]).fg(colors::regex::CLASS));
                self.in_class = false;
                self.class_open_idx = None;
                self.class_negation_idx = None;
                self.i += 1;
            }
            _ => {
                self.lit_start.get_or_insert(i);
                self.i += ch_len;
            }
        }
    }

    /// Advance one character while outside any character class.
    fn step_outside_class(&mut self, ch: char, ch_len: usize) {
        let input = self.input; // Copy — does not borrow self
        let bytes = self.bytes;
        let i = self.i;
        let len = bytes.len();

        match ch {
            '\\' if i + 1 < len => {
                self.flush_lit(i);
                let nc = input[i + 1..].chars().next().unwrap();
                let end = i + 1 + nc.len_utf8();
                self.spans.push(Span::raw(&input[i..end]).fg(colors::regex::ESCAPE));
                self.i = end;
            }
            '[' => {
                self.flush_lit(i);
                self.class_open_idx = Some(self.spans.len());
                self.spans.push(Span::raw(&input[i..i + 1]).fg(colors::regex::CLASS));
                self.in_class = true;
                self.i += 1;
                if self.i < len && bytes[self.i] == b'^' {
                    self.class_negation_idx = Some(self.spans.len());
                    self.spans
                        .push(Span::raw(&input[self.i..self.i + 1]).fg(colors::regex::ANCHOR));
                    self.i += 1;
                }
            }
            '(' => {
                self.flush_lit(i);
                let color = colors::regex::GROUP[self.depth % colors::regex::GROUP.len()];
                self.depth += 1;
                let end = self.scan_group_prefix(i);
                self.group_stack.push(self.spans.len());
                self.spans.push(Span::raw(&input[i..end]).fg(color));
                self.i = end;
            }
            ')' => {
                self.flush_lit(i);
                if self.group_stack.pop().is_some() {
                    self.depth -= 1;
                    let color = colors::regex::GROUP[self.depth % colors::regex::GROUP.len()];
                    self.spans.push(Span::raw(&input[i..i + 1]).fg(color));
                } else {
                    // Unmatched `)` — no opening paren
                    self.spans.push(Span::raw(&input[i..i + 1]).bg(colors::ERROR));
                }
                self.i += 1;
            }
            '*' | '+' => {
                self.flush_lit(i);
                let mut end = i + 1;
                if end < len && bytes[end] == b'?' {
                    end += 1; // lazy modifier
                }
                self.spans
                    .push(Span::raw(&input[i..end]).fg(colors::regex::QUANTIFIER));
                self.i = end;
            }
            '?' => {
                self.flush_lit(i);
                self.spans
                    .push(Span::raw(&input[i..i + 1]).fg(colors::regex::QUANTIFIER));
                self.i += 1;
            }
            '{' => {
                self.flush_lit(i);
                self.i += 1;
                while self.i < len && bytes[self.i] != b'}' {
                    self.i += 1;
                }
                if self.i < len {
                    self.i += 1; // include `}`
                    if self.i < len && bytes[self.i] == b'?' {
                        self.i += 1; // lazy modifier
                    }
                }
                self.spans
                    .push(Span::raw(&input[i..self.i]).fg(colors::regex::QUANTIFIER));
            }
            '^' | '$' => {
                self.flush_lit(i);
                self.spans
                    .push(Span::raw(&input[i..i + 1]).fg(colors::regex::ANCHOR));
                self.i += 1;
            }
            '.' | '|' => {
                self.flush_lit(i);
                self.spans
                    .push(Span::raw(&input[i..i + 1]).fg(colors::regex::META));
                self.i += 1;
            }
            _ => {
                self.lit_start.get_or_insert(i);
                self.i += ch_len;
            }
        }
    }

    /// Handles `(`, `(?:`, `(?=`, `(?!`, `(?<=`, `(?<!`.
    /// Named groups `(?<name>` — only `(?<` is consumed; `name>` is left as literal.
    fn scan_group_prefix(&self, i: usize) -> usize {
        let mut end = i + 1;
        if end < self.bytes.len() && self.bytes[end] == b'?' {
            end += 1;
            match self.bytes.get(end).copied() {
                Some(b':') | Some(b'=') | Some(b'!') => end += 1,
                Some(b'<') => {
                    end += 1;
                    // `(?<=` or `(?<!`
                    if matches!(self.bytes.get(end).copied(), Some(b'=') | Some(b'!')) {
                        end += 1;
                    }
                }
                _ => {}
            }
        }
        end
    }

    /// Apply retroactive ERROR coloring to any still-open delimiters, then
    /// assemble and return the final [`Line`].
    fn finalize(mut self) -> Line<'a> {
        // Every `(` still on the stack was never closed.
        for idx in self.group_stack {
            self.spans[idx].style = Style::new().bg(colors::ERROR);
        }

        // The open `[` (and optional `^` negation) was never closed.
        if let Some(idx) = self.class_open_idx {
            self.spans[idx].style = Style::new().bg(colors::ERROR);
            if let Some(neg_idx) = self.class_negation_idx {
                self.spans[neg_idx].style = Style::new().bg(colors::ERROR);
            }
        }

        Line::from(self.spans)
    }
}
