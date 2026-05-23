mod actions;
mod command;
pub mod control;
mod keybinding;
mod mouse;
mod terminal;
mod widgets;
mod help;

use self::{
    actions::{Action, CommandAction, NormalAction, VisualAction},
    control::{InputMode, PromptMode},
    keybinding::Keybinding,
    mouse::MouseHandler,
};
use crate::{
    app::{
        actions::HelpAction, command::Command, terminal::{Terminal, TerminalState}, widgets::{MultiplexerWidget, PromptWidget}
    },
    components::{
        config::filter::FilterConfigApp,
        cursor::Cursor,
        instance::Instance,
        mux::{MultiplexerApp, MultiplexerMode},
        prompt::{self, PromptApp, PromptMovement},
        status::StatusApp,
    },
    direction::Direction,
    regex_compile,
};
use actions::{ConfigAction, FilterAction};
use anyhow::Result;
use bvr_core::{SegBuffer, err::Error, index::BoxedStream, matches::CompositeStrategy};
use crossterm::{clipboard::CopyToClipboard, event};
use regex::bytes::Regex;
use std::{
    borrow::Cow, collections::VecDeque, fs::OpenOptions, num::NonZeroUsize, path::{Path, PathBuf}, time::{Duration, Instant}
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
    action_queue: VecDeque<Action>,
    commands: Vec<Command>,
}

impl App {
    pub fn new(state: State, term: Terminal) -> Self {
        let mut app = Self {
            app: state,
            term: TerminalState::new(term),
            action_queue: VecDeque::new(),
            refresh: false,
            commands: Vec::new(),
        };

        app.add_command(Command::new("help", &["h"]).set_action(Self::command_help));
        app.add_command(Command::new("quit", &["q"]).set_action(Self::command_quit));
        app.add_command(Command::new("mcap", &[]).set_action(Self::command_mcap));
        app.add_command(
            Command::new("realpath", &["rp", "readlink", "rl"]).set_action(Self::command_realpath),
        );
        app.add_command(Command::new("pbcopy", &["pb"]).set_action(Self::command_pbcopy));
        app.add_command(Command::new("refresh", &[]).set_action(Self::command_refresh));
        app.add_command(Command::new("open", &["o"]).set_action(Self::command_open));
        app.add_command(Command::new("export", &[]).set_action(Self::command_export));
        app.add_command(Command::new("close", &["c"]).set_action(Self::command_close));
        app.add_command(Command::new("gutter", &["g"]).set_action(Self::command_gutter));
        app.add_command(
            Command::new("mux", &["m"])
                .add_subcommand(
                    Command::new("tabs", &["t", "none"]).set_action(Self::command_mux_tabs),
                )
                .add_subcommand(
                    Command::new("split", &["s", "win"]).set_action(Self::command_mux_panes),
                )
                .set_action(Self::command_mux),
        );
        app.add_command(
            Command::new("filter", &["f", "find"])
                .add_subcommand(Command::new("link", &[]).set_action(Self::command_filter_linked))
                .add_subcommand(
                    Command::new("persist", &["p"]).set_action(Self::command_filter_persist),
                )
                .add_subcommand(Command::new("copy", &["c"]).set_action(Self::command_filter_copy))
                .add_subcommand(Command::new("save", &["s"]).set_action(Self::command_filter_save))
                .add_subcommand(Command::new("load", &[]).set_action(Self::command_filter_load))
                .add_subcommand(
                    Command::new("clear", &["c"]).set_action(Self::command_filter_clear),
                )
                .add_subcommand(
                    Command::new("union", &["u"]).set_action(Self::command_filter_union),
                )
                .add_subcommand(
                    Command::new("intersect", &["i"]).set_action(Self::command_filter_intersect),
                ),
        );

        app.app.viewer.help = help::HelpManual::generate(&app.commands);

        app
    }

    pub fn run(&mut self) -> Result<()> {
        self.term.enter_terminal()?;

        self.event_loop()?;

        if self.app.viewer.filter_config.is_persistent() {
            if let Some(source) = self.app.viewer.mux.active_mut() {
                let export = source.compositor_mut().filters().export(None);

                if let Err(err) = self.app.viewer.filter_config.set_persistent_filter(export) {
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

            let mut render = |f: &mut ratatui::Frame| self.app.viewer.ui(f, &mut mouse_handler);

            const MIN_REFRESH_DURATION: Duration = Duration::from_millis(16);
            const MIN_POLL_DURATION: Duration = Duration::from_millis(32);

            let now = Instant::now();

            if last_drawn
                .map(|last_drawn| now.duration_since(last_drawn) > MIN_REFRESH_DURATION)
                .unwrap_or(true)
            {
                self.term.draw(|f| {
                    let cursor = render(f);
                    if let Some(cursor) = cursor {
                        f.set_cursor_position(cursor);
                    }
                })?;
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
                HelpAction::PanVertical {
                    direction,
                    delta,
                } => {
                    self.app.viewer.help.pan_vertically(direction, delta);
                }
            },
            Action::SwitchMode(new_mode) => {
                let old_mode = self.app.viewer.mode;
                self.app.viewer.mode = new_mode;

                match new_mode {
                    InputMode::Visual => {
                        if let Some(instance) = self.app.viewer.mux.active_mut() {
                            instance.move_selected_into_view();
                            instance.set_follow_output(false);
                        }
                    }
                    InputMode::Prompt(PromptMode::Search { edit: true, .. }) => {
                        if let InputMode::Prompt(PromptMode::Search { edit: true, .. }) = old_mode {
                            return Ok(true);
                        }
                        match self
                            .app
                            .viewer
                            .mux
                            .active_mut()
                            .and_then(|instance| instance.compositor_mut().selected_filter())
                            .and_then(|filter| filter.mask().regex())
                        {
                            Some(regex) => {
                                self.app.viewer.prompt.take();
                                self.app.viewer.prompt.enter_str(regex.as_str());
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
                        instance.move_viewport_vertical(direction, delta)
                    }
                }
                NormalAction::PanHorizontal {
                    direction,
                    delta,
                    target_view,
                } => {
                    if let Some(instance) = self.app.viewer.get_target_view(target_view) {
                        instance.move_viewport_horizontal(direction, delta)
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
                } => self
                    .app
                    .viewer
                    .filter_config
                    .move_select(direction, select, delta),
                ConfigAction::LoadSelectedFilter => {
                    let Some(export) = self.app.viewer.filter_config.selected_filter() else {
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
                    let selected_filters = self.app.viewer.filter_config.selected_filter_indices();
                    if let Err(err) = self
                        .app
                        .viewer
                        .filter_config
                        .remove_filters(selected_filters)
                    {
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
                    PromptMovement::new(
                        select,
                        match jump {
                            actions::CommandJump::Word => prompt::PromptDelta::Word,
                            actions::CommandJump::Boundary => prompt::PromptDelta::Boundary,
                            actions::CommandJump::None => prompt::PromptDelta::Number(1),
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

                    if !self.app.viewer.prompt.advance_completion() {
                        // Fresh completion — compute candidates from the current buffer.
                        let candidates = self.complete_command(&buf);
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
            if let Some(instance) = self.app.viewer.mux.active_mut() {
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

    pub fn add_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn process_command_system(&mut self, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();

        fn find_cmd<'a>(cmds: &'a mut [Command], token: Option<&str>) -> Option<&'a mut Command> {
            cmds.iter_mut().find(|cmd| {
                Some(cmd.name) == token
                    || token
                        .map(|token| cmd.aliases.contains(&token))
                        .unwrap_or(false)
            })
        }

        if parts.is_empty() {
            return;
        }

        let mut cmd: Option<&mut Command> = None;
        let mut parts = parts.as_slice();
        loop {
            let (part, rem): (Option<&str>, &[&str]) = parts
                .split_first()
                .map_or((None, &[]), |(&first, rest)| (Some(first), rest));

            if let Some(current_cmd) = cmd {
                let subcommand_string = current_cmd
                    .subcommands
                    .iter()
                    .map(|cmd| cmd.name)
                    .collect::<Vec<_>>()
                    .join(", ");

                let subcmd = find_cmd(&mut current_cmd.subcommands, part);

                if subcmd.is_none() {
                    if let Some(action) = current_cmd.action.as_mut() {
                        action(self, parts);
                    } else {
                        self.app.viewer.status.msg(format!(
                            "Command '{}' requires subcommand: {}",
                            current_cmd.name, subcommand_string
                        ));
                    }
                    return;
                }

                cmd = subcmd;
                parts = rem;
            } else {
                let basecmd = find_cmd(&mut self.commands, part);

                if basecmd.is_none() {
                    self.app
                        .viewer
                        .status
                        .msg(format!("Unknown command '{}'", part.unwrap_or("UNKNOWN")));
                    return;
                }

                cmd = basecmd;
                parts = rem;
            }
        }
    }

    fn process_command(&mut self, command: &str) -> bool {
        if let Ok(line_number) = command.parse::<usize>() {
            if let Some(instance) = self.app.viewer.mux.active_mut() {
                if let Some(idx) = instance.nearest_index(line_number) {
                    instance.viewport_mut().jump_vertically_to(idx);
                }
            }
        } else {
            self.process_command_system(command)
        }

        true
    }

    fn command_help(&mut self, _: &[&str]) {
        self.app.viewer.mode = InputMode::Help;
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

    fn command_pbcopy(&mut self, _: &[&str]) {
        if let Some(instance) = self.app.viewer.mux.active_mut() {
            match instance.export_string() {
                Ok(text) => {
                    match crossterm::execute!(
                        self.term.backend_mut(),
                        CopyToClipboard::to_clipboard_from(text)
                    ) {
                        Ok(_) => {
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
    }

    fn command_mux_tabs(&mut self, _: &[&str]) {
        self.app.viewer.mux.set_mode(MultiplexerMode::Tabs);
    }

    fn command_mux_panes(&mut self, _: &[&str]) {
        self.app.viewer.mux.set_mode(MultiplexerMode::Panes);
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
        let new_persistence = !self.app.viewer.filter_config.is_persistent();

        if let Err(err) = self
            .app
            .viewer
            .filter_config
            .set_persistent(new_persistence)
        {
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
        let name: String = args.into_iter().copied().collect::<Vec<&str>>().join(" ");
        let export = source.compositor_mut().filters().export(Some(name));

        if let Err(err) = self.app.viewer.filter_config.add_filter(export) {
            self.app.viewer.status.msg(format!("filter save: {err}"));
        }

        self.app
            .viewer
            .status
            .msg("filter save: saved filters".to_string());
    }

    fn command_filter_load(&mut self, _: &[&str]) {
        self.app.viewer.mode = InputMode::Config;
    }

    fn command_filter_clear(&mut self, _: &[&str]) {
        self.app.viewer.demux_mut(|instance| {
            instance.clear_filters();
        });
    }

    fn command_filter_union(&mut self, _: &[&str]) {
        self.app.viewer.demux_mut(|instance| {
            instance.set_composite_strategy(CompositeStrategy::Union);
        });
    }

    fn command_filter_intersect(&mut self, _: &[&str]) {
        self.app.viewer.demux_mut(|instance| {
            instance.set_composite_strategy(CompositeStrategy::Intersection);
        });
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
        if let Some(instance) = self.app.viewer.mux.active_mut() {
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

    fn complete_command(&self, buf: &str) -> Vec<&'static str> {
        fn all_names(cmds: &[Command]) -> Vec<&'static str> {
            cmds.iter().map(|cmd| cmd.name).collect()
        }

        fn find_cmd<'a>(cmds: &'a [Command], token: &str) -> Option<&'a Command> {
            cmds.iter()
                .find(|cmd| cmd.name == token || cmd.aliases.contains(&token))
        }

        if buf.is_empty() {
            return all_names(&self.commands);
        }

        let ends_with_space = buf.ends_with(char::is_whitespace);
        let mut tokens: Vec<&str> = buf.split_whitespace().collect();

        if ends_with_space {
            // Every token is complete; navigate the hierarchy and offer the next level.
            let mut current: &[Command] = &self.commands;
            for token in &tokens {
                match find_cmd(current, token) {
                    Some(cmd) if !cmd.subcommands.is_empty() => {
                        current = &cmd.subcommands;
                    }
                    _ => return vec![],
                }
            }

            all_names(current)
        } else {
            // Last token is a partial word; complete it against the current level.
            let partial = match tokens.pop() {
                Some(p) => p,
                None => return vec![],
            };

            let mut current: &[Command] = &self.commands;
            for token in &tokens {
                match find_cmd(current, token) {
                    Some(cmd) if !cmd.subcommands.is_empty() => {
                        current = &cmd.subcommands;
                    }
                    _ => return vec![],
                }
            }

            all_names(current)
                .into_iter()
                .filter(|c| c.starts_with(partial))
                .collect()
        }
    }
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

struct RegexCache {
    pattern: String,
    escaped: bool,
    regex: Option<Regex>,
}

pub struct Viewer {
    mode: InputMode,
    mux: MultiplexerApp,
    status: StatusApp,
    prompt: PromptApp,
    regex_cache: Option<RegexCache>,
    filter_config: FilterConfigApp,
    help: help::HelpManual,
    gutter: bool,
    linked_filters: bool,
}

impl Viewer {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
            prompt: PromptApp::new(),
            mux: MultiplexerApp::new(),
            status: StatusApp::new(),
            regex_cache: None,
            filter_config: FilterConfigApp::new(),
            gutter: true,
            linked_filters: false,
            help: help::HelpManual::new(),
        }
    }

    fn push_instance(&mut self, name: String, link: Option<PathBuf>, file: SegBuffer) {
        self.mux.push(Instance::new(name, link, file));
    }

    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        let load_filters = self.mux.is_empty() && self.filter_config.is_persistent();

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
            let filter_set = match self.filter_config.get_persistent_filter() {
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

    fn ui(&mut self, f: &mut ratatui::Frame, handler: &mut MouseHandler) -> Option<(u16, u16)> {
        let [mux_chunk, cmd_chunk] = MultiplexerWidget::split_bottom(f.area(), 1);

        match self.mode {
            InputMode::Prompt(PromptMode::Search { escaped, .. }) => {
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
            }
            InputMode::Prompt(_)
            | InputMode::Normal
            | InputMode::Visual
            | InputMode::Filter
            | InputMode::Help
            | InputMode::Config => {
                self.regex_cache = None;
            }
        }

        MultiplexerWidget {
            mux: &mut self.mux,
            status: &mut self.status,
            mode: self.mode,
            config: &mut self.filter_config,
            gutter: self.gutter,
            linked_filters: self.linked_filters,
            help: &mut self.help,
            regex: self
                .regex_cache
                .as_ref()
                .and_then(|cache| cache.regex.as_ref()),
        }
        .render(mux_chunk, f.buffer_mut(), handler);

        let mut cursor = None;
        PromptWidget {
            mode: self.mode,
            prompt: &mut self.prompt,
            cursor: &mut cursor,
        }
        .render(cmd_chunk, f.buffer_mut());

        cursor
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
