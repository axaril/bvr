use ratatui::prelude::*;

use crate::{
    app::{
        control::{InputMode, PromptMode},
        viewer::Instance,
    },
    colors,
};

pub struct StatusWidget<'a> {
    input_mode: InputMode,
    instance: Option<&'a Instance>,
    message: Option<&'a str>,
}

impl<'a> StatusWidget<'a> {
    pub fn new(input_mode: InputMode) -> Self {
        Self {
            input_mode,
            instance: None,
            message: None,
        }
    }

    pub fn with_instance(mut self, instance: Option<&'a Instance>) -> Self {
        self.instance = instance;
        self
    }

    pub fn with_message(mut self, message: Option<&'a str>) -> Self {
        self.message = message;
        self
    }
}

impl<'a> Widget for StatusWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        const STATUS_BAR_STYLE: Style = Style::new()
            .fg(colors::STATUS_BAR_TEXT)
            .bg(colors::STATUS_BAR);

        let (accent_color, mode_name) = match self.input_mode {
            InputMode::Prompt(PromptMode::Command) => (colors::COMMAND_ACCENT, " COMMAND "),
            InputMode::Prompt(PromptMode::Shell { .. }) => (colors::SHELL_ACCENT, " SHELL "),
            InputMode::Prompt(PromptMode::Search { escaped, edit }) => (
                colors::FILTER_ACCENT,
                match (escaped, edit) {
                    (true, true) => " EDIT FILTER (ESCAPED) ",
                    (true, false) => " FILTER (ESCAPED) ",
                    (false, true) => " EDIT FILTER ",
                    (false, false) => " FILTER ",
                },
            ),
            InputMode::Prompt(PromptMode::FilterColor) => (colors::FILTER_ACCENT, " FILTER COLOR "),
            InputMode::Normal => (colors::NORMAL_ACCENT, " NORMAL "),
            InputMode::Visual => (colors::SELECT_ACCENT, " VISUAL "),
            InputMode::Filter => (colors::FILTER_ACCENT, " FILTER "),
            InputMode::Config => (colors::CONFIG_ACCENT, " CONFIG "),
            InputMode::Help => (colors::COMMAND_ACCENT, " HELP "),
        };

        let mut v = Vec::with_capacity(16);

        v.push(Span::raw(mode_name).fg(colors::WHITE).bg(accent_color));
        v.push(Span::raw(" "));

        if let Some(instance) = self.instance {
            v.push(Span::raw(instance.name()).fg(colors::STATUS_BAR_TEXT));
        } else {
            v.push(Span::raw("Empty").fg(colors::STATUS_BAR_TEXT));
        }
        v.push(Span::raw(" │ ").fg(colors::STATUS_BAR_TEXT));

        if let Some(message) = self.message {
            v.push(Span::raw(message));
        } else if let Some(instance) = self.instance {
            let ln_cnt = instance.file().line_count();
            let ln_vis = instance.visible_line_count();
            v.push(Span::raw(format!("{} lines", ln_cnt)).fg(accent_color));
            if ln_vis < ln_cnt {
                v.push(Span::raw(format!(" ({} visible)", ln_vis)).fg(colors::STATUS_BAR_TEXT));
            }
            v.push(Span::raw(" │ ").fg(accent_color));
            v.push(Span::raw(instance.name()).fg(accent_color));
            let index = instance.file().index();
            if !index.is_complete() {
                if let Some(progress) = index.report().progress() {
                    v.push(
                        Span::raw(format!(" ({:.0}% loaded)", progress * 100f32))
                            .fg(colors::STATUS_BAR_TEXT),
                    );
                }
            }
        } else {
            v.push(Span::raw(":open [file name]").fg(accent_color));
            v.push(Span::raw(" to view a file").fg(colors::STATUS_BAR_TEXT));
        }

        Line::from(v).style(STATUS_BAR_STYLE).render(area, buf);

        if let Some(instance) = self.instance {
            if instance.is_following_output() {
                Line::raw("Follow  ").fg(colors::STATUS_BAR_TEXT)
            } else {
                let bottom = instance.view_bounds().bottom();
                let ln_vis = instance.visible_line_count();
                let percentage = if ln_vis == 0 {
                    1.0
                } else {
                    bottom as f64 / ln_vis as f64
                }
                .clamp(0.0, 1.0);

                let row = instance.view_bounds().top();
                let col = instance.view_bounds().left();

                Line::from(vec![
                    Span::raw(format!("{}:{}", row + 1, col + 1)).fg(colors::STATUS_BAR_TEXT),
                    Span::raw(format!("  {:.0}%  ", percentage * 100.0))
                        .fg(colors::STATUS_BAR_TEXT),
                ])
            }
            .alignment(Alignment::Right)
            .render(area, buf)
        }
    }
}
