mod actions;
mod command;
mod control;
mod keybinding;
mod mouse;
mod terminal;

mod config;
mod filters;
mod help;
mod highlight;
mod mux;
mod prompt;
mod status;
mod viewer;

use self::{
    actions::{Action, CommandAction, NormalAction, VisualAction},
    control::{InputMode, PromptMode},
    keybinding::Keybinding,
    mouse::MouseHandler,
};
use crate::{
    app::{
        actions::HelpAction,
        command::{Command, CommandSystem},
        terminal::{Terminal, TerminalState},
        viewer::Instance,
    },
    cursor::Cursor,
    direction::Direction,
    regex_compile,
};
use actions::{ConfigAction, FilterAction};
use anyhow::Result;
use arboard::Clipboard;
use bvr_core::{SegBuffer, err::Error, index::BoxedStream, matches::CompositeStrategy};
use crossterm::{clipboard::CopyToClipboard, event};
use regex::bytes::Regex;
use std::{
    borrow::Cow,
    collections::VecDeque,
    fs::OpenOptions,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct State {
    viewer: Viewer,
    keybinds: Keybinding,
}

impl State {
    pub fn new() -> Self {
        Self {
            viewer: Viewer::new(),
            keybinds: Keybinding::Hardcoded,
            // keybinds: Keybinding::from_toml_file(Path::new("default_keybindings.toml"), false)
            //     .unwrap(),
        }
    }

    pub fn viewer_mut(&mut self) -> &mut Viewer {
        &mut self.viewer
    }
}

pub struct App {
    app: State,
    term: terminal::TerminalState,
    refresh: bool,
    clipboard: Option<Clipboard>,
    action_queue: VecDeque<Action>,
    commands: CommandSystem,
}

impl App {
    const DEFAULT_COMMANDS: &'static [Command] = &[
        Command::new("help")
            .aliases(&["h", "?"])
            .description("Display all commands.")
            .bind(Self::command_help),
        Command::new("quit")
            .aliases(&["q"])
            .description("Quit the application.")
            .bind(Self::command_quit),
        Command::new("mcap")
            .description("Toggle mouse capture.")
            .bind(Self::command_mcap),
        Command::new("realpath")
            .aliases(&["rp", "readlink", "rl"])
            .description("Copy the path of the current file to the clipboard if applicable.")
            .bind(Self::command_realpath),
        Command::new("pbcopy")
            .aliases(&["pb"])
            .description("Copy the content of the current view to the clipboard.")
            .bind(Self::command_pbcopy),
        Command::new("refresh")
            .description("Refresh the screen.")
            .bind(Self::command_refresh),
        Command::new("open")
            .aliases(&["o"])
            .args("<path>")
            .description("Open a file in a new tab/view.")
            .bind(Self::command_open),
        Command::new("export")
            .args("<path>")
            .description("Export the content of the current view to a file.")
            .bind(Self::command_export),
        Command::new("close")
            .aliases(&["c"])
            .description("Close the current tab/view.")
            .bind(Self::command_close),
        Command::new("gutter")
            .aliases(&["g"])
            .description("Toggle the gutter line numbers.")
            .bind(Self::command_gutter),
        Command::new("mux")
            .aliases(&["m"])
            .description("Toggle the multiplexer mode between windows or tabs.")
            .subcommands(&[
                Command::new("tabs")
                    .aliases(&["t", "none"])
                    .description("Set the multiplexer to tabs mode.")
                    .bind(Self::command_mux_tabs),
                Command::new("split")
                    .aliases(&["s", "win"])
                    .description("Set the multiplexer to split window mode.")
                    .bind(Self::command_mux_panes),
            ])
            .bind(Self::command_mux),
        Command::new("filter")
            .aliases(&["f", "find"])
            .description("Commands for managing filters.")
            .subcommands(&[
                Command::new("link")
                    .description("Toggle whether filters are linked across all views.")
                    .bind(Self::command_filter_linked),
                Command::new("persist")
                    .aliases(&["p"])
                    .description("Toggle whether filters are persisted on shutdown and launch.")
                    .bind(Self::command_filter_persist),
                Command::new("copy")
                    .aliases(&["c"])
                    .args("<view index>")
                    .description("Copy the filter set of the current view to a target view.")
                    .bind(Self::command_filter_copy),
                Command::new("save")
                    .aliases(&["s"])
                    .args("[name]")
                    .description("Save the currently selected filter set to the config.")
                    .bind(Self::command_filter_save),
                Command::new("load")
                    .description("Load a filter set from the config into the current view.")
                    .bind(Self::command_filter_load),
                Command::new("clear")
                    .aliases(&["c"])
                    .description("Clear all filters from the current view.")
                    .bind(Self::command_filter_clear),
                Command::new("union")
                    .aliases(&["u"])
                    .description("Union the selected filter sets into the current view.")
                    .bind(Self::command_filter_union),
                Command::new("intersect")
                    .aliases(&["i"])
                    .description("Intersect the selected filter sets into the current view.")
                    .bind(Self::command_filter_intersect),
            ]),
    ];

    pub fn new(state: State, term: Terminal) -> Self {
        Self {
            app: state,
            term: TerminalState::new(term),
            clipboard: Clipboard::new().ok(),
            action_queue: VecDeque::new(),
            refresh: false,
            commands: CommandSystem::new(Self::DEFAULT_COMMANDS),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        self.term.enter_terminal()?;

        self.event_loop()?;

        if self.app.viewer.config.is_persistent() {
            if let Some(source) = self.app.viewer.mux.active_mut() {
                let export = source.compositor_mut().filters().export(None);

                if let Err(err) = self.app.viewer.config.set_persistent_filter(export) {
                    self.app.viewer.status.msg(format!("filter save: {err}"));
                }

                self.app
                    .viewer
                    .status
                    .msg("filter save: saved filters".to_string());
            }
        }

        Ok(())
    }

    fn event_loop(&mut self) -> Result<()> {
        let mut mouse_handler = MouseHandler::new();

        let mut last_drawn: Option<Instant> = None;
        loop {
            if self.refresh {
                self.term.clear()?;
                self.refresh = false;
            }

            let mut render =
                |f: &mut ratatui::Frame| self.app.viewer.ui(f, &self.commands, &mut mouse_handler);

            const MIN_REFRESH_DURATION: Duration = Duration::from_millis(16);
            const MIN_POLL_DURATION: Duration = Duration::from_millis(32);

            let now = Instant::now();

            if last_drawn
                .map(|last_drawn| now.duration_since(last_drawn) > MIN_REFRESH_DURATION)
                .unwrap_or(true)
            {
                self.term.draw(render)?;
                last_drawn = Some(now);
            } else if self.term.mouse_capture {
                // We render to capture mouse actions
                render(&mut self.term.get_frame());
                // But we avoid drawing so terminal won't look weird
                self.term.current_buffer_mut().reset();
            }

            let action = match self
                .action_queue
                .pop_front()
                .or_else(|| mouse_handler.extract())
            {
                Some(action) => action,
                None if event::poll(MIN_POLL_DURATION)? => {
                    let mut event = event::read()?;
                    let key = self.app.keybinds.map_key(self.app.viewer.mode, &mut event);
                    mouse_handler.publish_event(event);
                    let Some(action) = key else { continue };
                    action
                }
                None => continue,
            };

            if !self.process_action(action)? {
                break Ok(());
            }
        }
    }

    fn process_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Exit => return Ok(false),
            Action::Help(action) => match action {
                HelpAction::PanVertical { direction, delta } => {
                    self.app.viewer.help.pan_vertical(direction, delta);
                }
            },
            Action::SwitchMode(new_mode) => {
                let old_mode = self.app.viewer.mode;
                self.app.viewer.mode = new_mode;

                match &mut self.app.viewer.mode {
                    InputMode::Visual => {
                        if let Some(instance) = self.app.viewer.mux.active_mut() {
                            instance.move_selected_into_view();
                            instance.set_follow_output(false);
                        }
                    }
                    InputMode::Prompt(PromptMode::Search {
                        edit: true,
                        escaped,
                    }) => {
                        if let InputMode::Prompt(PromptMode::Search { edit: true, .. }) = old_mode {
                            return Ok(true);
                        }
                        match self
                            .app
                            .viewer
                            .mux
                            .active_mut()
                            .and_then(|instance| instance.compositor_mut().selected_filter())
                            .map(|filter| filter.mask())
                        {
                            Some(filters::Mask::Regex {
                                name,
                                escaped: mask_escaped,
                                ..
                            }) => {
                                *escaped = *mask_escaped;
                                self.app.viewer.prompt.take();
                                self.app.viewer.prompt.enter_str(name.as_str());
                            }
                            _ => {
                                self.app.viewer.mode = old_mode;
                                return Ok(true);
                            }
                        };
                    }
                    _ => {
                        if !old_mode.is_prompt_search() || !new_mode.is_prompt_search() {
                            self.app.viewer.prompt.take();
                        }
                    }
                }
            }
            Action::Normal(action) => match action {
                NormalAction::PanVertical {
                    direction,
                    delta,
                    target_view,
                } => {
                    if let Some(instance) = self.app.viewer.get_target_view(target_view) {
                        instance.pan_vertical(direction, delta)
                    }
                }
                NormalAction::PanHorizontal {
                    direction,
                    delta,
                    target_view,
                } => {
                    if let Some(instance) = self.app.viewer.get_target_view(target_view) {
                        instance.pan_horizontal(direction, delta)
                    }
                }
                NormalAction::FollowOutput => {
                    if let Some(instance) = self.app.viewer.mux.active_mut() {
                        instance.set_follow_output(true);
                    }
                }
                NormalAction::SwitchActiveIndex { target_view } => {
                    self.app.viewer.mux.move_active_index(target_view)
                }
                NormalAction::SwitchActive { direction } => {
                    self.app.viewer.mux.move_active(direction)
                }
            },
            Action::Visual(action) => match action {
                VisualAction::Move {
                    direction,
                    select,
                    delta,
                } => {
                    if let Some(instance) = self.app.viewer.mux.active_mut() {
                        instance.move_select(direction, select, delta);
                        instance.set_follow_output(false);
                    }
                }
                VisualAction::ToggleSelectedLine => {
                    if let Some(instance) = self.app.viewer.mux.active_mut() {
                        instance.toggle_select_bookmarks();
                    }
                }
                VisualAction::ToggleLine {
                    target_view,
                    line_number,
                } => {
                    if let Some(instance) = self.app.viewer.mux.instances_mut().get_mut(target_view)
                    {
                        instance.toggle_bookmark_line_number(line_number)
                    }
                }
            },
            Action::Filter(action) => match action {
                FilterAction::Move {
                    direction,
                    select,
                    delta,
                } => {
                    self.app.viewer.demux_mut(|instance| {
                        instance
                            .compositor_mut()
                            .move_select(direction, select, delta)
                    });
                }
                FilterAction::ToggleSelectedFilter => {
                    self.app.viewer.demux_mut(|instance| {
                        instance.toggle_selected_filters();
                    });
                }
                FilterAction::RemoveSelectedFilter => {
                    self.app.viewer.demux_mut(|instance| {
                        instance.remove_selected_filters();
                    });
                }
                FilterAction::Displace { direction, delta } => {
                    self.app.viewer.demux_mut(|instance| {
                        instance.displace_selected_filters(direction, delta);
                    });
                }
                FilterAction::ToggleSpecificFilter {
                    target_view,
                    filter_index,
                } => {
                    if self.app.viewer.linked_filters {
                        self.app
                            .viewer
                            .mux
                            .instances_mut()
                            .iter_mut()
                            .for_each(|instance| {
                                instance.toggle_filter(filter_index);
                            });
                    } else if let Some(instance) =
                        self.app.viewer.mux.instances_mut().get_mut(target_view)
                    {
                        instance.toggle_filter(filter_index)
                    }
                }
            },
            Action::Config(action) => match action {
                ConfigAction::Move {
                    direction,
                    select,
                    delta,
                } => self.app.viewer.config.move_select(direction, select, delta),
                ConfigAction::LoadSelectedFilter => {
                    let Some(export) = self.app.viewer.config.selected_filter() else {
                        return Ok(true);
                    };

                    self.app
                        .viewer
                        .mux
                        .demux_mut(self.app.viewer.linked_filters, |target| {
                            target.import_user_filters(&export);
                        });
                }
                ConfigAction::RemoveSelectedFilter => {
                    let selected_filters = self.app.viewer.config.selected_filter_indices();
                    if let Err(err) = self.app.viewer.config.remove_filters(selected_filters) {
                        self.app
                            .viewer
                            .status
                            .msg(format!("filter save remove: {err}"));
                    }
                }
            },
            Action::Command(action) => match action {
                CommandAction::Move {
                    direction,
                    select,
                    jump,
                } => self.app.viewer.prompt.move_cursor(
                    direction,
                    prompt::Movement::new(
                        select,
                        match jump {
                            actions::CommandJump::Word => prompt::Delta::Word,
                            actions::CommandJump::Boundary => prompt::Delta::Boundary,
                            actions::CommandJump::None => prompt::Delta::Number(1),
                        },
                    ),
                ),
                CommandAction::Type { input } => self.app.viewer.prompt.enter_char(input),
                CommandAction::Paste { input } => self.app.viewer.prompt.enter_str(&input),
                CommandAction::Backspace => {
                    if !self.app.viewer.prompt.delete() {
                        self.app.viewer.mode = InputMode::Normal;
                    }
                }
                CommandAction::Submit => {
                    let result = match self.app.viewer.mode {
                        InputMode::Prompt(PromptMode::Command) => {
                            self.app.viewer.mode = InputMode::Normal;
                            let command = self.app.viewer.prompt.submit();
                            Ok(self.process_command(&command))
                        }
                        InputMode::Prompt(PromptMode::Search { escaped, edit }) => {
                            self.app.viewer.mode = InputMode::Normal;
                            let command = self.app.viewer.prompt.take();
                            Ok(self.process_search(&command, escaped, edit))
                        }
                        InputMode::Prompt(PromptMode::Shell { pipe }) => {
                            self.app.viewer.mode = InputMode::Normal;
                            let command = self.app.viewer.prompt.take();
                            self.process_shell(&command, true, pipe)
                        }
                        InputMode::Prompt(PromptMode::FilterColor) => {
                            self.app.viewer.mode = InputMode::Normal;
                            let command = self.app.viewer.prompt.take();
                            use std::str::FromStr;
                            let color = match ratatui::style::Color::from_str(&command) {
                                Ok(color) => color,
                                Err(_) => {
                                    self.app
                                        .viewer
                                        .status
                                        .msg(format!("filter color: invalid color `{command}`"));
                                    return Ok(true);
                                }
                            };
                            self.app.viewer.demux_mut(|instance| {
                                instance.set_selected_filter_color(color);
                            });
                            Ok(true)
                        }
                        InputMode::Normal
                        | InputMode::Visual
                        | InputMode::Filter
                        | InputMode::Help
                        | InputMode::Config => unreachable!(),
                    };
                    return result;
                }
                CommandAction::History { direction } => {
                    if self.app.viewer.mode != InputMode::Prompt(PromptMode::Command) {
                        return Ok(true);
                    }
                    match direction {
                        Direction::Back => self.app.viewer.prompt.backward(),
                        Direction::Next => self.app.viewer.prompt.forward(),
                    }
                }
                CommandAction::Complete => {
                    if self.app.viewer.mode != InputMode::Prompt(PromptMode::Command) {
                        return Ok(true);
                    }
                    // Only attempt completion when the cursor is at the end of the buffer.
                    let buf = self.app.viewer.prompt.buf().to_owned();
                    let Cursor::Singleton(cursor_pos) = self.app.viewer.prompt.cursor() else {
                        return Ok(true);
                    };
                    if cursor_pos != buf.len() {
                        return Ok(true);
                    }

                    /// Build the new buffer content after accepting `completion` for the last token.
                    fn build_completion(buf: &str, completion: &str) -> String {
                        if buf.ends_with(char::is_whitespace) {
                            format!("{buf}{completion} ")
                        } else {
                            let prefix = match buf.rfind(char::is_whitespace) {
                                Some(pos) => &buf[..=pos],
                                None => "",
                            };
                            format!("{prefix}{completion} ")
                        }
                    }

                    if !self.app.viewer.prompt.advance_completion() {
                        // Fresh completion — compute candidates from the current buffer.
                        let candidates = self.commands.complete_command(&buf);
                        match candidates.as_slice() {
                            &[] => {}
                            &[candidate] => {
                                let new_buf = build_completion(&buf, candidate);
                                self.app.viewer.prompt.set_current(new_buf);
                            }
                            &[candidate, ..] => {
                                // Determine the static prefix (everything before the partial token).
                                let prefix = if buf.ends_with(char::is_whitespace) {
                                    buf.clone()
                                } else {
                                    match buf.rfind(char::is_whitespace) {
                                        Some(pos) => buf[..=pos].to_owned(),
                                        None => String::new(),
                                    }
                                };
                                // Show the first candidate and enter cycling mode.
                                let new_buf = build_completion(&buf, candidate);
                                self.app.viewer.prompt.set_current(new_buf);
                                self.app.viewer.status.msg(candidates.join("  "));
                                self.app
                                    .viewer
                                    .prompt
                                    .add_completion_cycle(prefix, candidates);
                            }
                        }
                    }
                }
            },
        };

        Ok(true)
    }

    fn context(&mut self, s: &str) -> Result<Option<Cow<'static, str>>, std::env::VarError> {
        match s {
            "SEL" | "sel" => {
                if let Some(instance) = self.app.viewer.mux.active_mut() {
                    match instance.export_string() {
                        Ok(text) => Ok(Some(text.into())),
                        Err(err) => {
                            self.app
                                .viewer
                                .status
                                .msg(format!("selection expansion: {err}"));
                            Ok(Some("".into()))
                        }
                    }
                } else {
                    Ok(Some("".into()))
                }
            }
            s => match std::env::var(s) {
                Ok(value) => Ok(Some(value.into())),
                Err(std::env::VarError::NotPresent) => Ok(Some("".into())),
                Err(e) => Err(e),
            },
        }
    }

    fn process_shell(&mut self, command: &str, terminate: bool, pipe: bool) -> Result<bool> {
        let Ok(expanded) = shellexpand::env_with_context(command, |s| self.context(s)) else {
            self.app
                .viewer
                .status
                .msg("shell: expansion failed".to_string());
            return Ok(true);
        };

        let mut shl = shlex::Shlex::new(&expanded);
        let Some(cmd) = shl.next() else {
            self.app
                .viewer
                .status
                .msg("shell: no command provided".to_string());
            return Ok(true);
        };

        let args = shl.by_ref().collect::<Vec<_>>();

        if shl.had_error {
            self.app
                .viewer
                .status
                .msg("shell: lexing failed".to_string());
            return Ok(true);
        }

        let mut command = std::process::Command::new(cmd);
        command.args(args);

        self.term.exit_terminal()?;
        let mut child = match command.spawn() {
            Err(err) => {
                self.term.clear()?;
                self.term.enter_terminal()?;
                self.app.viewer.status.msg(format!("shell: {err}"));
                return Ok(true);
            }
            Ok(child) => child,
        };

        if pipe {
            let mut stdin = child.stdin.take().unwrap();
            if let Some(instance) = self.app.viewer.mux.active() {
                instance.write_bytes(&mut stdin)?;
            }
        }

        if terminate {
            self.app.viewer.mux.clear();
        }

        let status = match child.wait() {
            Err(err) => {
                self.app.viewer.status.msg(format!("shell: {err}"));
                return Ok(true);
            }
            Ok(status) => status,
        };

        if terminate {
            std::process::exit(status.code().unwrap_or(0));
        }

        Ok(!terminate)
    }

    fn process_search(&mut self, pat: &str, escaped: bool, edit: bool) -> bool {
        if pat.is_empty() {
            return true;
        }

        let mut e = None;
        self.app.viewer.demux_mut(|instance| {
            let result = if edit {
                instance.edit_search_filter(pat, escaped)
            } else {
                instance.add_search_filter(pat, escaped)
            };
            if let Err(err) = result {
                e.get_or_insert(err);
            };
        });

        if let Some(err) = e {
            self.app.viewer.status.msg(match err {
                regex::Error::Syntax(err) => format!("{pat}: syntax ({err})"),
                regex::Error::CompiledTooBig(sz) => {
                    format!("{pat}: regex surpassed size limit ({sz} bytes)")
                }
                _ => format!("{pat}: {err}"),
            });
        }

        true
    }

    fn process_command(&mut self, command: &str) -> bool {
        if let Ok(line_number) = command.parse::<usize>() {
            if let Some(instance) = self.app.viewer.mux.active_mut() {
                if let Some(idx) = instance.nearest_index(line_number) {
                    instance.jump_vertically_to(idx);
                }
            }
        } else {
            match self.commands.resolve(command) {
                Ok((command, args)) => {
                    command(self, args.as_slice());
                }
                Err(err) => {
                    self.app.viewer.status.msg(format!("{err}"));
                }
            }
        }

        true
    }

    fn command_help(&mut self, _: &[&str]) {
        self.app.viewer.mode = InputMode::Help;
        self.app
            .viewer
            .status
            .msg("help: displaying help".to_string());
    }

    fn command_quit(&mut self, _: &[&str]) {
        self.action_queue.push_back(Action::Exit);
    }

    fn command_mcap(&mut self, _: &[&str]) {
        match self.term.toggle_mouse_capture() {
            Ok(_) => {
                self.app.viewer.status.msg(format!(
                    "mouse capture {}",
                    if self.term.mouse_capture {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            Err(_) => {
                self.app
                    .viewer
                    .status
                    .msg("mouse capture toggle failed".to_string());
            }
        }
    }

    fn command_realpath(&mut self, _: &[&str]) {
        if let Some(instance) = self.app.viewer.mux.active_mut() {
            if let Some(link) = instance.link() {
                let link = link.display();
                self.app.viewer.status.msg(format!("readlink: {}", link));
                crossterm::execute!(
                    self.term.backend_mut(),
                    CopyToClipboard::to_clipboard_from(link.to_string())
                )
                .ok();
            } else {
                self.app.viewer.status.msg("readlink: no link".to_string());
            }
        } else {
            self.app
                .viewer
                .status
                .msg(String::from("No active instances"));
        }
    }

    fn command_refresh(&mut self, _: &[&str]) {
        self.refresh = true;
        self.app
            .viewer
            .status
            .msg("refresh: refresh requested".to_string());
    }

    fn command_open(&mut self, args: &[&str]) {
        if args.len() != 1 {
            self.app.viewer.status.msg("usage: open <path>".to_string());
            return;
        }

        let path = args.into_iter().collect::<PathBuf>();
        if let Err(err) = self.app.viewer.open_file(path.as_ref()) {
            self.app
                .viewer
                .status
                .msg(format!("{}: {err}", path.display()));
        }
    }

    fn pbcopy(&mut self, text: &str) -> anyhow::Result<()> {
        if let Some(clipboard) = &mut self.clipboard {
            clipboard.set_text(text)?;
        } else {
            crossterm::execute!(
                self.term.backend_mut(),
                CopyToClipboard::to_clipboard_from(text)
            )?;
        }

        Ok(())
    }

    fn command_pbcopy(&mut self, _: &[&str]) {
        if let Some(instance) = self.app.viewer.mux.active_mut() {
            match instance.export_string() {
                Ok(ref text) => {
                    match self.pbcopy(text) {
                        Ok(()) => {
                            self.app
                                .viewer
                                .status
                                .msg("pbcopy: copied to clipboard".to_string());
                        }
                        Err(err) => {
                            self.app.viewer.status.msg(format!("pbcopy: {err}"));
                        }
                    };
                }
                Err(err) => {
                    self.app.viewer.status.msg(format!("pbcopy: {err}"));
                }
            }
        }
    }

    fn command_close(&mut self, _: &[&str]) {
        if self.app.viewer.mux.active_mut().is_some() {
            self.app.viewer.mux.close_active()
        } else {
            self.app
                .viewer
                .status
                .msg(String::from("No active instances"));
        }
    }

    fn command_gutter(&mut self, _: &[&str]) {
        self.app.viewer.toggle_gutter();
        self.app.viewer.status.msg(format!(
            "gutter: {}",
            if self.app.viewer.gutter {
                "enabled"
            } else {
                "disabled"
            }
        ));
    }

    fn command_mux_tabs(&mut self, _: &[&str]) {
        self.app.viewer.mux.set_mode(mux::Mode::ActiveOnly);
        self.app
            .viewer
            .status
            .msg("mux: set to tabs mode".to_string());
    }

    fn command_mux_panes(&mut self, _: &[&str]) {
        self.app.viewer.mux.set_mode(mux::Mode::SplitView);
        self.app
            .viewer
            .status
            .msg("mux: set to split view mode".to_string());
    }

    fn command_mux(&mut self, _: &[&str]) {
        self.app
            .viewer
            .mux
            .set_mode(self.app.viewer.mux.mode().swap());
    }

    fn command_filter_linked(&mut self, _: &[&str]) {
        self.app.viewer.linked_filters = !self.app.viewer.linked_filters;
        if self.app.viewer.linked_filters {
            self.app.viewer.replicate_filters_on_all_instances();
        }
    }

    fn command_filter_persist(&mut self, _: &[&str]) {
        let new_persistence = !self.app.viewer.config.is_persistent();

        if let Err(err) = self.app.viewer.config.set_persistent(new_persistence) {
            self.app.viewer.status.msg(format!("filter persist: {err}"));
            return;
        }

        self.app
            .viewer
            .status
            .msg(format!("filter persist: persistence = {new_persistence}"));
    }

    fn command_filter_copy(&mut self, args: &[&str]) {
        let Some(source) = self.app.viewer.mux.active_mut() else {
            return;
        };
        let export = source.compositor_mut().filters().export(None);

        let Some(idx) = args.first() else {
            self.app
                .viewer
                .status
                .msg(String::from("filter export: requires instance index"));
            return;
        };

        let Ok(idx) = idx.parse::<usize>() else {
            self.app
                .viewer
                .status
                .msg(format!("filter export {idx}: invalid index"));
            return;
        };
        let idx = idx.saturating_sub(1);
        if self.app.viewer.mux.active_index() == idx {
            self.app.viewer.status.msg(String::from(
                "filter export: cannot export to active instance",
            ));
            return;
        }
        let Some(target) = self.app.viewer.mux.instances_mut().get_mut(idx) else {
            self.app
                .viewer
                .status
                .msg(format!("filter export {idx}: invalid index"));
            return;
        };

        target.import_user_filters(&export);
    }

    fn command_filter_save(&mut self, args: &[&str]) {
        let Some(source) = self.app.viewer.mux.active_mut() else {
            return;
        };
        let name: String = args.join(" ");
        let export = source.compositor_mut().filters().export(Some(name));

        if let Err(err) = self.app.viewer.config.add_filter(export) {
            self.app.viewer.status.msg(format!("filter save: {err}"));
        }

        self.app
            .viewer
            .status
            .msg("filter save: saved filters".to_string());
    }

    fn command_filter_load(&mut self, _: &[&str]) {
        self.app.viewer.mode = InputMode::Config;
        self.app
            .viewer
            .status
            .msg("filter load: select filter to load".to_string());
    }

    fn command_filter_clear(&mut self, _: &[&str]) {
        self.app.viewer.demux_mut(|instance| {
            instance.clear_filters();
        });
        self.app
            .viewer
            .status
            .msg("filter clear: cleared filters".to_string());
    }

    fn command_filter_union(&mut self, _: &[&str]) {
        self.app.viewer.demux_mut(|instance| {
            instance.set_composite_strategy(CompositeStrategy::Union);
        });
        self.app
            .viewer
            .status
            .msg("filter strategy: set to union".to_string());
    }

    fn command_filter_intersect(&mut self, _: &[&str]) {
        self.app.viewer.demux_mut(|instance| {
            instance.set_composite_strategy(CompositeStrategy::Intersection);
        });
        self.app
            .viewer
            .status
            .msg("filter strategy: set to intersection".to_string());
    }

    fn command_export(&mut self, args: &[&str]) {
        let Some(path) = args.first() else {
            self.app
                .viewer
                .status
                .msg("usage: export <path>".to_string());
            return;
        };
        let path = PathBuf::from(path);
        if let Some(instance) = self.app.viewer.mux.active() {
            if let Err(err) = OpenOptions::new()
                .create_new(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .map_err(Error::from)
                .and_then(|mut file| instance.write_bytes(&mut file))
            {
                self.app
                    .viewer
                    .status
                    .msg(format!("{}: {err}", path.display()));
            } else {
                self.app
                    .viewer
                    .status
                    .msg(format!("{}: export complete", path.display()));
            }
        } else {
            self.app
                .viewer
                .status
                .msg(String::from("No active instances"));
        }
    }
}

struct RegexCache {
    pattern: String,
    escaped: bool,
    regex: Option<Regex>,
}

pub struct Viewer {
    mode: InputMode,
    mux: mux::State,
    status: status::State,
    prompt: prompt::State,
    config: config::filters::State,
    help: help::State,
    regex_cache: Option<RegexCache>,
    gutter: bool,
    linked_filters: bool,
}

impl Viewer {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
            mux: mux::State::new(),
            status: status::State::new(),
            prompt: prompt::State::new(),
            config: config::filters::State::new(),
            help: help::State::new(),
            regex_cache: None,
            gutter: true,
            linked_filters: false,
        }
    }

    fn push_instance(&mut self, name: String, link: Option<PathBuf>, file: SegBuffer) {
        self.mux.push(Instance::new(name, link, file));
    }

    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        let load_filters = self.mux.is_empty() && self.config.is_persistent();

        let file = std::fs::File::open(path)?;

        if !file.metadata()?.is_file() {
            return Err(anyhow::anyhow!("Not a file"));
        }

        let link = std::fs::canonicalize(path).ok();

        let name = path
            .file_name()
            .map(|str| str.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from("Unnamed File"));
        self.push_instance(
            name,
            link,
            SegBuffer::read_file(file, NonZeroUsize::new(25).unwrap(), false)?,
        );

        if load_filters {
            let filter_set = match self.config.get_persistent_filter() {
                Ok(filters) => filters,
                Err(err) => {
                    self.status.msg(format!("filter persist/load: {err}"));
                    return Ok(());
                }
            };
            let instance = self.mux.active_mut().unwrap();
            match filter_set {
                Some(export) => instance.import_user_filters(export),
                None => {}
            }
        }
        if self.linked_filters {
            if let Some(source) = self.mux.active_mut() {
                let export = source.compositor_mut().filters().export(None);
                let cursor = *source.compositor_mut().cursor();

                let instance = self.mux.instances_mut().last_mut().unwrap();
                instance.import_user_filters(&export);
                instance.compositor_mut().set_cursor(cursor)
            }
        }

        Ok(())
    }

    pub fn open_stream(&mut self, name: String, stream: BoxedStream) -> Result<()> {
        self.push_instance(name, None, SegBuffer::read_stream(stream, false)?);
        Ok(())
    }

    fn get_target_view(&mut self, target_view: Option<usize>) -> Option<&mut Instance> {
        if let Some(index) = target_view {
            self.mux.instances_mut().get_mut(index)
        } else {
            self.mux.active_mut()
        }
    }

    fn replicate_filters_on_all_instances(&mut self) {
        if let Some(source) = self.mux.active_mut() {
            let export = source.compositor_mut().filters().export(None);
            let cursor = *source.compositor_mut().cursor();
            let active = self.mux.active_index();
            self.mux
                .instances_mut()
                .iter_mut()
                .enumerate()
                .filter(|(i, _)| *i != active)
                .for_each(|(_, instance)| {
                    instance.import_user_filters(&export);
                    instance.compositor_mut().set_cursor(cursor)
                });
        }
    }

    fn ui(&mut self, f: &mut ratatui::Frame, commands: &CommandSystem, handler: &mut MouseHandler) {
        let Some(chunks) = mux::split_mux(f.area()) else {
            return;
        };
        let [tab_chunk, mut mux_chunk, status_chunk, cmd_chunk] = chunks;

        let buf = f.buffer_mut();
        let active_index = self.mux.active_index();

        mux::Widget::hydrate(&mut self.mux)
            .override_mode(Some(mux::Mode::SplitView))
            .render(tab_chunk, buf, |pane_chunk, buf, view_index, instance| {
                mux::TabWidget {
                    view_index,
                    name: instance.name(),
                    active: active_index == view_index,
                }
                .render(pane_chunk, buf, handler);
            });

        if let InputMode::Filter | InputMode::Config | InputMode::Help = self.mode {
            const FILTER_MAX_HEIGHT: u16 = 10;
            let mut area = mux_chunk;
            if let Some([view_chunk, panel_area]) =
                crate::split::split_bottom(mux_chunk, FILTER_MAX_HEIGHT)
            {
                mux_chunk = view_chunk;
                area = panel_area;
            } else {
                mux_chunk = ratatui::layout::Rect::default();
            }

            match self.mode {
                InputMode::Filter => {
                    mux::Widget::hydrate(&mut self.mux)
                        .override_mode(self.linked_filters.then_some(mux::Mode::ActiveOnly))
                        .render(area, buf, |pane_chunk, buf, view_index, instance| {
                            filters::Widget {
                                view_index,
                                compositor: instance.compositor_mut(),
                            }
                            .render(pane_chunk, buf, handler);
                        });
                }
                InputMode::Config => {
                    config::Widget::hydrate(&mut self.config).render(area, buf, handler);
                }
                InputMode::Help => {
                    self.help.set_height(usize::from(area.height));
                    help::Widget::hydrate(&mut self.help)
                        .commands(commands.commands())
                        .render(area, buf);
                }
                _ => {}
            }
        }

        mux::Widget::hydrate(&mut self.mux).render(
            mux_chunk,
            buf,
            |pane_chunk, buf, view_index, instance| {
                let regex = {
                    let mut editing_search = false;
                    match self.mode {
                        InputMode::Prompt(PromptMode::Search { escaped, edit }) => {
                            let pattern = self.prompt.buf();

                            let pattern_mismatch = self
                                .regex_cache
                                .as_ref()
                                .map(|cache| cache.escaped != escaped || cache.pattern != pattern)
                                .unwrap_or(true);

                            if pattern_mismatch {
                                let regex = if !escaped {
                                    regex_compile(pattern)
                                } else {
                                    regex_compile(&regex::escape(pattern))
                                }
                                .ok();

                                self.regex_cache = Some(RegexCache {
                                    pattern: pattern.to_owned(),
                                    escaped,
                                    regex,
                                })
                            }

                            editing_search = edit;
                        }
                        _ => {
                            self.regex_cache = None;
                        }
                    }

                    if active_index == view_index {
                        let regex = self
                            .regex_cache
                            .as_ref()
                            .and_then(|cache| cache.regex.as_ref());

                        regex.map(|regex| {
                            (
                                if editing_search {
                                    instance.compositor_mut().selected_filter().unwrap().color()
                                } else {
                                    instance.color_selector().peek_color()
                                },
                                regex,
                            )
                        })
                    } else {
                        None
                    }
                };

                viewer::Widget {
                    view_index,
                    instance,
                    show_selection: self.mode == InputMode::Visual,
                    gutter: self.gutter,
                    regex,
                }
                .render(pane_chunk, buf, handler);
            },
        );

        use ratatui::widgets::Widget as _;
        status::Widget::new(self.mode)
            .with_instance(self.mux.active_mut().map(|v| &*v))
            .with_message(self.status.get_message_update().as_deref())
            .render(status_chunk, buf);

        prompt::Widget {
            mode: self.mode,
            commands,
            prompt: &mut self.prompt,
        }
        .render(cmd_chunk, f);
    }

    pub fn toggle_gutter(&mut self) {
        self.gutter = !self.gutter;
    }

    pub fn demux_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut Instance),
    {
        self.mux.demux_mut(self.linked_filters, f);
    }
}
