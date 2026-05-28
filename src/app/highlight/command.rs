use crate::app::{Command, command::CommandSystem};
use crate::colors;

use ratatui::prelude::*;

use super::ColorableSpan;

pub struct CommandHighlighter<'a> {
    base: super::Highlighter<'a>,
}

enum TakeRemaining {
    None,
    Args,
    NoArgs,
}

impl<'a> CommandHighlighter<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            base: super::Highlighter::new(input),
        }
    }

    fn eat_arg(&mut self) -> Option<ColorableSpan<'_, 'a>> {
        if self.base.input.is_empty() {
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
            .unwrap_or(self.base.input.len());

        let mut span = self.base.eat_and_color(len);
        span.content = span.content.trim();
        Some(span)
    }

    pub fn highlight(mut self, commands: &CommandSystem) -> Vec<Span<'a>> {
        let mut cmd: Option<&Command> = None;
        let mut take_remaining_as_args = TakeRemaining::None;

        let skip = self.base.scan_bytes_until(|b| !b.is_ascii_whitespace());
        if let Some(skip) = skip {
            self.base.eat_and_color(skip);
        }

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

            let arg_str = arg.content;

            if let Some(current_cmd) = cmd.as_ref() {
                let has_action = current_cmd.action.is_some();
                let takes_args = !current_cmd.arguments.is_empty();

                cmd = current_cmd
                    .subcommands
                    .iter()
                    .find(|cmd| cmd.name == arg_str || cmd.aliases.contains(&arg_str));

                if let Some(_) = cmd {
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
                cmd = commands
                    .commands()
                    .iter()
                    .find(|cmd| cmd.name == arg_str || cmd.aliases.contains(&arg_str));

                if let Some(_) = cmd {
                    arg.fg(colors::COMMAND_ACCENT);
                } else {
                    arg.fg(colors::ERROR);
                    break;
                }
            }
        }

        // Color any remaining input as inactive.
        if let Some(span) = self.base.eat_remaining_and_color() {
            span.fg(colors::TEXT_INACTIVE);
        }

        self.base.extract()
    }
}
