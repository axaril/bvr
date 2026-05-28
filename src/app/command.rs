use crate::app::App;

type CommandAction = fn(&mut App, &str);

/// Lexer for a command string.
///
/// Tokens are separated by ASCII whitespace. A token that begins with a single
/// (`'`) or double (`"`) quote character is read until the matching closing
/// quote; the surrounding quotes are stripped from the returned value. An
/// unterminated quoted token consumes the rest of the input.
///
/// [`CommandLexer::remaining`] returns the unconsumed portion of the original
/// string with leading and trailing whitespace trimmed, but with all internal
/// spacing preserved — useful for passing a free-form tail argument verbatim to
/// a command action.
///
/// # Example
/// ```
/// let mut lex = CommandLexer::new(r#"open  "my file.txt"  --read-only"#);
/// assert_eq!(lex.next_token(), Some("open"));
/// assert_eq!(lex.next_token(), Some("my file.txt"));
/// assert_eq!(lex.remaining(), "--read-only");
/// ```
pub struct CommandLexer<'a> {
    input: &'a str,
}

impl<'a> CommandLexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    /// Advance `pos` past any leading whitespace.
    fn skip_whitespace(&mut self) {
        self.input = self.input.trim_start();
    }

    /// Return the next token, or `None` when the input is exhausted.
    pub fn next_token(&mut self) -> Option<&'a str> {
        self.skip_whitespace();

        if self.input.is_empty() {
            return None;
        }

        let len = self
            .input
            .find(|c: char| c.is_whitespace())
            .unwrap_or(self.input.len());

        let token = &self.input[..len];
        self.input = &self.input[len..];
        Some(token)
    }

    /// Return the unconsumed tail of the input as a single string slice.
    ///
    /// Leading and trailing whitespace is trimmed, but all whitespace *between*
    /// remaining tokens is left untouched. This lets you forward the rest of a
    /// command line verbatim — e.g. a search pattern that may contain runs of
    /// spaces — without losing fidelity.
    pub fn remaining(&self) -> &'a str {
        self.input.trim()
    }
}

impl<'a> Iterator for CommandLexer<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

pub struct Command {
    pub(super) name: &'static str,
    pub(super) aliases: &'static [&'static str],
    pub(super) description: &'static str,
    pub(super) arguments: &'static str,
    pub(super) subcommands: &'static [Command],
    pub(super) action: Option<CommandAction>,
}

impl Command {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name: name,
            aliases: &[],
            description: "",
            arguments: "",
            subcommands: &[],
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

    pub const fn subcommands(mut self, subcommands: &'static [Command]) -> Self {
        self.subcommands = subcommands;
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

pub struct CommandSystem {
    commands: &'static [Command],
}

impl CommandSystem {
    pub fn new(commands: &'static [Command]) -> Self {
        Self { commands }
    }

    pub fn commands(&self) -> &[Command] {
        self.commands
    }

    pub fn complete_command(&self, buf: &str) -> Vec<&'static str> {
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
            let mut current = &self.commands;
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

            let mut current = &self.commands;
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

    pub fn resolve<'a>(&self, command: &'a str) -> anyhow::Result<(CommandAction, &'a str)> {
        fn find_cmd<'a>(cmds: &'a [Command], token: Option<&str>) -> Option<&'a Command> {
            cmds.iter().find(|cmd| {
                Some(cmd.name) == token
                    || token
                        .map(|token| cmd.aliases.contains(&token))
                        .unwrap_or(false)
            })
        }

        let mut lexer = CommandLexer::new(command);

        if command.is_empty() {
            anyhow::bail!("Empty command");
        }

        let mut cmd: Option<&Command> = None;
        loop {
            let rem = lexer.remaining();
            let part = lexer.next_token();

            if let Some(current_cmd) = cmd {
                let subcmd = find_cmd(&current_cmd.subcommands, part);

                if subcmd.is_none() {
                    if let Some(action) = current_cmd.action {
                        return Ok((action, rem));
                    } else {
                        let subcommand_string = current_cmd
                            .subcommands
                            .iter()
                            .map(|cmd| cmd.name)
                            .collect::<Vec<_>>()
                            .join(", ");
                        anyhow::bail!(
                            "Command '{}' requires subcommand: {}",
                            current_cmd.name,
                            subcommand_string
                        );
                    }
                }

                cmd = subcmd;
            } else {
                let basecmd = find_cmd(&self.commands, part);

                if basecmd.is_none() {
                    anyhow::bail!("Unknown command '{}'", part.unwrap_or("UNKNOWN"));
                }

                cmd = basecmd;
            }
        }
    }
}
