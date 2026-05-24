use crate::colors;
use ratatui::prelude::*;

struct Tokens<'a> {
    input: &'a str,
    i: usize,
}

impl<'a> Tokens<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, i: 0 }
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
}

pub(super) struct RegexHighlighter<'a> {
    tokens: Tokens<'a>,

    // depth: usize,
    in_class: bool,
    lit_start: Option<usize>,

    spans: Vec<Span<'a>>,

    group_stack: Vec<usize>,
    class_open_idx: Option<usize>,
}

impl<'a> RegexHighlighter<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            tokens: Tokens::new(input),
            // depth: 0,
            in_class: false,
            lit_start: None,
            spans: Vec::new(),
            group_stack: Vec::new(),
            class_open_idx: None,
        }
    }

    pub fn highlight(mut self) -> Line<'a> {
        while let Some(ch) = self.tokens.current() {
            let old_i = self.tokens.i;

            if self.in_class {
                self.step_in_class(ch);
            } else {
                self.step_outside_class(ch);
            }

            // Ensure we are making forward progress
            assert!(
                self.tokens.i > old_i,
                "highlighter did not advance at position {}",
                self.tokens.i
            );
        }
        self.flush_lit();
        self.finalize()
    }

    fn eat_and_color(&mut self, len: usize, color: Color) {
        self.spans.push(Span::raw(self.tokens.eat(len)).fg(color));
    }

    /// Flush any accumulated literal characters as an unstyled span.
    fn flush_lit(&mut self) {
        if let Some(s) = self.lit_start.take() {
            if s < self.tokens.i {
                self.spans
                    .push(Span::raw(&self.tokens.input[s..self.tokens.i]));
            }
        }
    }

    /// Advance one character while inside a `[...]` character class.
    fn step_in_class(&mut self, ch: char) {
        match ch {
            '\\' if let Some(nc) = self.tokens.peek() => {
                self.flush_lit();
                self.eat_and_color(1 + nc.len_utf8(), colors::regex::ESCAPE);
            }
            ']' => {
                self.flush_lit();
                self.spans
                    .push(Span::raw(self.tokens.eat(1)).fg(colors::regex::CLASS));
                self.in_class = false;
                self.class_open_idx = None;
            }
            ch => {
                self.lit_start.get_or_insert(self.tokens.i);
                self.tokens.eat(ch.len_utf8());
            }
        }
    }

    /// Advance one character while outside any character class.
    fn step_outside_class(&mut self, ch: char) {
        match ch {
            '\\' if let Some(nc) = self.tokens.peek() => {
                self.flush_lit();
                self.eat_and_color(1 + nc.len_utf8(), colors::regex::ESCAPE);
            }
            '[' => {
                self.flush_lit();
                self.class_open_idx = Some(self.spans.len());
                self.in_class = true;

                if let Some('^') = self.tokens.peek() {
                    self.eat_and_color(2, colors::regex::CLASS);
                } else {
                    self.eat_and_color(1, colors::regex::CLASS);
                }
            }
            '(' => {
                self.flush_lit();
                let color =
                    colors::regex::GROUP[self.group_stack.len() % colors::regex::GROUP.len()];
                // `(`, `(?:`, `(?=`, `(?!`, `(?<=`, `(?<!` `(?<name>`
                let group_prefix_size = {
                    let mut done = false;
                    let mut saw_question = false;
                    let mut named_group = false;
                    self.tokens
                        .scan_bytes_until(|b| {
                            if b == b'?' {
                                saw_question = true;
                                return false;
                            }
                            if saw_question && (b == b':' || b == b'=' || b == b'!' || b == b'=') {
                                done = true;
                                return true;
                            }
                            if saw_question && b == b'<' {
                                named_group = true;
                                return false;
                            }
                            if named_group && b == b'>' {
                                done = true;
                                return true;
                            }
                            // Not a valid group prefix, treat as normal `(` literal
                            done = true;
                            false
                        })
                        .unwrap_or(0)
                        + 1
                };
                self.group_stack.push(self.spans.len());
                self.eat_and_color(group_prefix_size, color);
            }
            ')' => {
                self.flush_lit();
                if self.group_stack.pop().is_some() {
                    let color =
                        colors::regex::GROUP[self.group_stack.len() % colors::regex::GROUP.len()];
                    self.eat_and_color(1, color);
                } else {
                    // Unmatched `)` — no opening paren
                    self.spans
                        .push(Span::raw(self.tokens.eat(1)).bg(colors::ERROR));
                }
            }
            '*' | '+' | '?' => {
                self.flush_lit();
                self.eat_and_color(1, colors::regex::QUANTIFIER);
            }
            '{' => {
                if let Some(qlen) = self.tokens.scan_bytes_until(|b| b == b'}') {
                    self.flush_lit();
                    // Include the closing `}`
                    let qlen = qlen + 1;
                    self.eat_and_color(qlen, colors::regex::QUANTIFIER);
                } else {
                    // just a regular literal
                    self.lit_start.get_or_insert(self.tokens.i);
                    self.tokens.eat(1);
                }
            }
            '^' | '$' => {
                self.flush_lit();
                self.eat_and_color(1, colors::regex::ANCHOR);
            }
            '.' | '|' => {
                self.flush_lit();
                self.eat_and_color(1, colors::regex::META);
            }
            ch => {
                self.lit_start.get_or_insert(self.tokens.i);
                self.tokens.eat(ch.len_utf8());
            }
        }
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
        }

        Line::from(self.spans)
    }
}
