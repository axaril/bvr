use crate::app::Command;
use crate::colors;

use ratatui::prelude::*;

use super::{ColorableSpan, Highlighter};

pub struct CommandHighlighter<'a> {
    base: Highlighter<'a>,
}

enum TakeRemaining {
    None,
    Args,
    NoArgs,
}

impl<'a> CommandHighlighter<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            base: Highlighter::new(input),
        }
    }

    fn eat_arg(&mut self) -> Option<ColorableSpan<'_, 'a>> {
        if self.base.i >= self.base.input.len() {
            return None;
        }

        // scan til the end, or til see another argument
        let mut seen_space = false;
        let len = self
            .base
            .scan_bytes_until(|b| {
                if b.is_ascii_whitespace() {
                    seen_space = true;
                    false
                } else if seen_space {
                    true
                } else {
                    false
                }
            })
            .unwrap_or(self.base.input.len() - self.base.i);

        let mut span = self.base.eat_and_color(len);
        span.content = span.content.trim();
        Some(span)
    }

    pub fn highlight(mut self, commands: &[Command]) -> Line<'a> {
        let mut current_command: Option<&Command> = None;
        let mut take_remaining_as_args = TakeRemaining::None;

        while let Some(arg) = self.eat_arg() {
            match take_remaining_as_args {
                TakeRemaining::Args => {
                    arg.fg(colors::TEXT_ACTIVE);
                    continue;
                }
                TakeRemaining::NoArgs => {
                    arg.fg(colors::TEXT_INACTIVE);
                    continue;
                }
                TakeRemaining::None => {}
            }

            if let Some(cmd) = current_command.as_ref() {
                let has_action = cmd.action.is_some();
                let takes_args = !cmd.arguments.is_empty();

                current_command = cmd
                    .subcommands
                    .iter()
                    .find(|cmd| cmd.name == arg.content || cmd.aliases.contains(&arg.content));

                if let Some(_) = current_command {
                    arg.fg(colors::COMMAND_ACCENT);
                } else if has_action && takes_args {
                    arg.fg(colors::TEXT_ACTIVE);
                    take_remaining_as_args = TakeRemaining::Args;
                } else if has_action && !takes_args {
                    arg.fg(colors::TEXT_INACTIVE);
                    take_remaining_as_args = TakeRemaining::NoArgs;
                } else {
                    arg.fg(colors::ERROR);
                    break;
                }
            } else {
                current_command = commands
                    .iter()
                    .find(|cmd| cmd.name == arg.content || cmd.aliases.contains(&arg.content));

                if let Some(_) = current_command {
                    arg.fg(colors::COMMAND_ACCENT);
                } else {
                    arg.fg(colors::ERROR);
                    break;
                }
            }
        }

        // Color any remaining input as inactive.
        if let len = self.base.input.len() - self.base.i
            && len > 0
        {
            self.base.eat_and_color(len).fg(colors::TEXT_INACTIVE);
        }

        self.base.extract()
    }
}
