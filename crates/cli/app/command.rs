use crate::app::App;

type CommandAction = fn(&mut App, &[&str]);

pub struct Command {
    pub(super) name: String,
    pub(super) aliases: Vec<&'static str>,
    pub(super) subcommands: Vec<Command>,
    pub(super) action: Option<CommandAction>,
}

impl Command {
    pub fn new(name: &str, aliases: &[&'static str]) -> Self {
        Self {
            name: name.into(),
            aliases: aliases.into_iter().cloned().collect(),
            subcommands: Vec::new(),
            action: None,
        }
    }

    pub fn add_subcommand(mut self, subcommand: Command) -> Self {
        self.subcommands.push(subcommand);
        self
    }

    pub fn set_action(mut self, action: CommandAction) -> Self {
        self.action = Some(action);
        self
    }
}
