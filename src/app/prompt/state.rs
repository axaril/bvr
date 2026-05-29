use crate::{
    cursor::{Cursor, CursorAnchor, CursorState},
    direction::Direction,
    view_bounds::ViewBounds,
};

#[derive(Clone, Copy)]
pub enum PromptDelta {
    Number(usize),
    Word,
    Boundary,
}

#[derive(Clone, Copy)]
pub struct PromptMovement {
    select: bool,
    delta: PromptDelta,
}

impl PromptMovement {
    pub const DEFAULT: Self = Self::new(false, PromptDelta::Number(1));

    pub const fn new(select: bool, delta: PromptDelta) -> Self {
        Self { select, delta }
    }
}

pub struct CompletionCycle {
    /// Part of the buffer that comes before the token being completed.
    pub prefix: String,
    /// Ordered list of candidates to cycle through.
    pub candidates: Vec<&'static str>,
    /// Index of the candidate currently shown in the buffer.
    pub index: usize,
}

pub struct PromptApp {
    history: Vec<String>,
    index: usize,
    buf: String,
    cursor: CursorState,
    bounds: ViewBounds,
    completion_cycle: Option<CompletionCycle>,
}

impl PromptApp {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            index: 0,
            buf: String::new(),
            cursor: CursorState::new(),
            bounds: ViewBounds::new(),
            completion_cycle: None,
        }
    }

    #[inline(always)]
    pub fn buf(&self) -> &str {
        if self.index < self.history.len() {
            &self.history[self.index]
        } else {
            &self.buf
        }
    }

    #[inline(always)]
    pub fn cursor(&self) -> Cursor {
        self.cursor.state()
    }

    #[inline(always)]
    pub fn view_bounds(&self) -> &ViewBounds {
        &self.bounds
    }

    pub fn update_view_bounds(&mut self, width: usize) {
        self.bounds.fit(1, width);
        match self.cursor.state() {
            Cursor::Singleton(i)
            | Cursor::Selection(i, _, CursorAnchor::End)
            | Cursor::Selection(_, i, CursorAnchor::Start) => {
                self.bounds.jump_horizontally_to(i);
            }
        }
    }

    pub fn move_cursor(&mut self, direction: Direction, movement: PromptMovement) {
        self.clear_completion_cycle();

        let buf = if self.index < self.history.len() {
            &self.history[self.index]
        } else {
            &self.buf
        };
        match direction {
            Direction::Back => self.cursor.back(movement.select, |i| match movement.delta {
                PromptDelta::Word => {
                    if buf[..i]
                        .chars()
                        .rev()
                        .nth(0)
                        .map(|c| c.is_whitespace())
                        .unwrap_or(false)
                    {
                        i.saturating_sub(
                            buf[..i]
                                .chars()
                                .rev()
                                .position(|c| c.is_alphanumeric())
                                .unwrap_or(0),
                        )
                    } else {
                        buf[..i].rfind(' ').map(|p| p + 1).unwrap_or(0)
                    }
                }
                PromptDelta::Boundary => 0,
                PromptDelta::Number(delta) => i.saturating_sub(
                    buf[..i]
                        .chars()
                        .rev()
                        .take(delta)
                        .map(|c| c.len_utf8())
                        .sum::<usize>(),
                ),
            }),
            Direction::Next => self.cursor.forward(movement.select, |i| {
                match movement.delta {
                    PromptDelta::Word => {
                        if buf[i..]
                            .chars()
                            .nth(0)
                            .map(|c| c.is_whitespace())
                            .unwrap_or(false)
                        {
                            i.saturating_add(
                                buf[i..]
                                    .chars()
                                    .position(|c| c.is_alphanumeric())
                                    .unwrap_or(usize::MAX),
                            )
                        } else {
                            buf[(i + 1).min(buf.len())..]
                                .chars()
                                .position(|c| c.is_whitespace())
                                .map(|z| z + i + 1)
                                .unwrap_or(usize::MAX)
                        }
                    }
                    PromptDelta::Boundary => usize::MAX,
                    PromptDelta::Number(delta) => i.saturating_add(
                        buf[i..]
                            .chars()
                            .take(delta)
                            .map(|c| c.len_utf8())
                            .sum::<usize>(),
                    ),
                }
                .min(buf.len())
            }),
        }
    }

    pub fn clear_completion_cycle(&mut self) {
        self.completion_cycle = None;
    }

    pub fn add_completion_cycle(&mut self, prefix: String, candidates: Vec<&'static str>) {
        self.completion_cycle = Some(CompletionCycle {
            prefix,
            candidates,
            index: 0,
        });
    }

    pub fn advance_completion(&mut self) -> Option<&CompletionCycle> {
        self.completion_cycle
            .as_mut()
            .map(|cycle| {
                cycle.index = (cycle.index + 1) % cycle.candidates.len();
                let index = cycle.index;
                format!("{}{} ", cycle.prefix, cycle.candidates[index])
            })
            .map(|buf| self.set_current(buf));

        self.completion_cycle.as_ref()
    }

    pub fn enter_char(&mut self, input: char) {
        self.clear_completion_cycle();

        let mut b = [0; 4];
        self.enter_str(input.encode_utf8(&mut b));
    }

    pub fn enter_str(&mut self, input: &str) {
        self.clear_completion_cycle();

        if self.index < self.history.len() {
            self.buf = self.history[self.index].clone();
            self.index = self.history.len();
        }

        match self.cursor.state() {
            Cursor::Singleton(i) => {
                self.buf.insert_str(i, input);
                self.move_cursor(
                    Direction::Next,
                    PromptMovement {
                        select: false,
                        delta: PromptDelta::Number(input.len()),
                    },
                )
            }
            Cursor::Selection(start, end, _) => {
                self.buf.replace_range(start..end, input);
                self.move_cursor(Direction::Back, PromptMovement::DEFAULT);
                self.move_cursor(
                    Direction::Next,
                    PromptMovement {
                        select: false,
                        delta: PromptDelta::Number(input.len()),
                    },
                )
            }
        }
    }

    pub fn delete(&mut self) -> bool {
        self.clear_completion_cycle();

        if self.index < self.history.len() {
            self.buf = self.history[self.index].clone();
            self.index = self.history.len();
        }

        match self.cursor.state() {
            Cursor::Singleton(curr) => {
                if curr == 0 {
                    return !self.buf.is_empty();
                }
                self.move_cursor(Direction::Back, PromptMovement::DEFAULT);
                let Cursor::Singleton(prev) = self.cursor.state() else {
                    unreachable!()
                };
                self.buf.replace_range(prev..curr, "");
            }
            Cursor::Selection(start, end, _) => {
                self.buf.replace_range(start..end, "");
                self.move_cursor(Direction::Back, PromptMovement::DEFAULT);
            }
        }
        true
    }

    /// Replace the current buffer with `text`, placing the cursor at the end.
    /// History is not modified.
    pub fn set_current(&mut self, text: String) {
        if self.index < self.history.len() {
            self.index = self.history.len();
        }
        self.buf = text;
        self.cursor.place(self.buf.len());
    }

    pub fn backward(&mut self) {
        self.clear_completion_cycle();

        self.index = self.index.saturating_sub(1);
        self.cursor.place(self.buf().len());
    }

    pub fn forward(&mut self) {
        self.clear_completion_cycle();

        self.index = self.index.saturating_add(1).min(self.history.len());
        self.cursor.place(self.buf().len());
    }

    pub fn submit(&mut self) -> String {
        let output = self.take();
        if self.history.last() != Some(&output) {
            self.history.push(output.clone());
            self.index = self.history.len();
        }
        output
    }

    pub fn take(&mut self) -> String {
        self.clear_completion_cycle();

        self.cursor.reset();
        if self.index < self.history.len() {
            let output = self.history.remove(self.index);
            self.index = self.history.len();
            output
        } else {
            std::mem::take(&mut self.buf)
        }
    }
}
