use crate::colors;
use ratatui::prelude::*;

use super::Highlighter;

pub struct RegexHighlighter<'a> {
    base: Highlighter<'a>,

    // depth: usize,
    in_class: bool,
    lit_start: Option<usize>,

    group_stack: Vec<usize>,
    class_open_idx: Option<usize>,
}

impl<'a> RegexHighlighter<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            base: Highlighter::new(input),
            // depth: 0,
            in_class: false,
            lit_start: None,
            group_stack: Vec::new(),
            class_open_idx: None,
        }
    }

    pub fn highlight(mut self) -> Vec<Span<'a>> {
        while let Some(ch) = self.base.current() {
            let old_i = self.base.i;

            if self.in_class {
                self.step_in_class(ch);
            } else {
                self.step_outside_class(ch);
            }

            // Ensure we are making forward progress
            assert!(
                self.base.i > old_i,
                "highlighter did not advance at position {}",
                self.base.i
            );
        }
        self.flush_lit();
        self.finalize()
    }

    /// Flush any accumulated literal characters as an unstyled span.
    fn flush_lit(&mut self) {
        if let Some(s) = self.lit_start.take() {
            if s < self.base.i {
                self.base
                    .spans
                    .push(Span::raw(&self.base.input[s..self.base.i]));
            }
        }
    }

    /// Advance one character while inside a `[...]` character class.
    fn step_in_class(&mut self, ch: char) {
        match ch {
            '\\' if let Some(nc) = self.base.peek() => {
                self.flush_lit();
                self.base
                    .eat_and_color(1 + nc.len_utf8()).fg(colors::regex::ESCAPE);
            }
            ']' => {
                self.flush_lit();
                self.base.eat_and_color(1).fg(colors::regex::CLASS);
                self.in_class = false;
                self.class_open_idx = None;
            }
            ch => {
                self.lit_start.get_or_insert(self.base.i);
                self.base.eat(ch.len_utf8());
            }
        }
    }

    /// Advance one character while outside any character class.
    fn step_outside_class(&mut self, ch: char) {
        match ch {
            '\\' if let Some(nc) = self.base.peek() => {
                self.flush_lit();
                self.base
                    .eat_and_color(1 + nc.len_utf8()).fg(colors::regex::ESCAPE);
            }
            '[' => {
                self.flush_lit();
                self.class_open_idx = Some(self.base.spans.len());
                self.in_class = true;

                if let Some('^') = self.base.peek() {
                    self.base.eat_and_color(2).fg(colors::regex::CLASS);
                } else {
                    self.base.eat_and_color(1).fg(colors::regex::CLASS);
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
                    self.base
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
                self.group_stack.push(self.base.spans.len());
                self.base.eat_and_color(group_prefix_size).fg(color);
            }
            ')' => {
                self.flush_lit();
                if self.group_stack.pop().is_some() {
                    let color =
                        colors::regex::GROUP[self.group_stack.len() % colors::regex::GROUP.len()];
                    self.base.eat_and_color(1).fg(color);
                } else {
                    // Unmatched `)` — no opening paren
                    self.base.eat_and_color(1).bg(colors::ERROR);
                }
            }
            '*' | '+' | '?' => {
                self.flush_lit();
                self.base.eat_and_color(1).fg(colors::regex::QUANTIFIER);
            }
            '{' => {
                if let Some(qlen) = self.base.scan_bytes_until(|b| b == b'}') {
                    self.flush_lit();
                    // Include the closing `}`
                    let qlen = qlen + 1;
                    self.base.eat_and_color(qlen).fg(colors::regex::QUANTIFIER);
                } else {
                    // just a regular literal
                    self.lit_start.get_or_insert(self.base.i);
                    self.base.eat(1);
                }
            }
            '^' | '$' => {
                self.flush_lit();
                self.base.eat_and_color(1).fg(colors::regex::ANCHOR);
            }
            '.' | '|' => {
                self.flush_lit();
                self.base.eat_and_color(1).fg(colors::regex::META);
            }
            ch => {
                self.lit_start.get_or_insert(self.base.i);
                self.base.eat(ch.len_utf8());
            }
        }
    }

    /// Apply retroactive ERROR coloring to any still-open delimiters, then
    /// assemble and return the final [`Line`].
    fn finalize(mut self) -> Vec<Span<'a>> {
        // Every `(` still on the stack was never closed.
        for idx in self.group_stack {
            self.base.spans[idx].style = Style::new().bg(colors::ERROR);
        }

        // The open `[` (and optional `^` negation) was never closed.
        if let Some(idx) = self.class_open_idx {
            self.base.spans[idx].style = Style::new().bg(colors::ERROR);
        }

        self.base.extract()
    }
}
