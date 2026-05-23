use crate::app::App;

type CommandAction = fn(&mut App, &[&str]);

pub struct Command {
    pub(super) name: &'static str,
    pub(super) aliases: &'static [&'static str],
    pub(super) description: &'static str,
    pub(super) arguments: &'static str,
    pub(super) subcommands: Vec<Command>,
    pub(super) action: Option<CommandAction>,
}

impl Command {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name: name,
            aliases: &[],
            description: "",
            arguments: "",
            subcommands: Vec::new(),
            action: None,
        }
    }

    pub const fn aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
        self
    }

    pub const fn description(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }

    pub fn subcommand(mut self, subcommand: Command) -> Self {
        self.subcommands.push(subcommand);
        self
    }

    pub const fn args(mut self, arguments: &'static str) -> Self {
        self.arguments = arguments;
        self
    }

    pub const fn bind(mut self, action: CommandAction) -> Self {
        self.action = Some(action);
        self
    }
}
