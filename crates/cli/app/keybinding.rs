use super::{
    InputMode, PromptMode,
    actions::{
        Action, CommandAction, CommandJump, ConfigAction, FilterAction, NormalAction, VisualAction,
    },
    control::ViewDelta,
};
use crate::{app::actions::HelpAction, direction::Direction};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, fmt, path::Path, str::FromStr};

/// Represents a key combination (key code + modifiers).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct KeyCombo {
    pub code: KeyCodeWrapper,
    pub modifiers: KeyModifiersWrapper,
}

/// Wrapper around crossterm's KeyCode for serde support.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyCodeWrapper {
    Char(char),
    F(u8),
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Delete,
    Insert,
    Esc,
    Space,
}

impl From<KeyCodeWrapper> for KeyCode {
    fn from(wrapper: KeyCodeWrapper) -> Self {
        match wrapper {
            KeyCodeWrapper::Char(c) => KeyCode::Char(c),
            KeyCodeWrapper::F(n) => KeyCode::F(n),
            KeyCodeWrapper::Backspace => KeyCode::Backspace,
            KeyCodeWrapper::Enter => KeyCode::Enter,
            KeyCodeWrapper::Left => KeyCode::Left,
            KeyCodeWrapper::Right => KeyCode::Right,
            KeyCodeWrapper::Up => KeyCode::Up,
            KeyCodeWrapper::Down => KeyCode::Down,
            KeyCodeWrapper::Home => KeyCode::Home,
            KeyCodeWrapper::End => KeyCode::End,
            KeyCodeWrapper::PageUp => KeyCode::PageUp,
            KeyCodeWrapper::PageDown => KeyCode::PageDown,
            KeyCodeWrapper::Tab => KeyCode::Tab,
            KeyCodeWrapper::BackTab => KeyCode::BackTab,
            KeyCodeWrapper::Delete => KeyCode::Delete,
            KeyCodeWrapper::Insert => KeyCode::Insert,
            KeyCodeWrapper::Esc => KeyCode::Esc,
            KeyCodeWrapper::Space => KeyCode::Char(' '),
        }
    }
}

impl TryFrom<KeyCode> for KeyCodeWrapper {
    type Error = ();

    fn try_from(code: KeyCode) -> Result<Self, Self::Error> {
        match code {
            KeyCode::Char(' ') => Ok(KeyCodeWrapper::Space),
            KeyCode::Char(c) => Ok(KeyCodeWrapper::Char(c)),
            KeyCode::F(n) => Ok(KeyCodeWrapper::F(n)),
            KeyCode::Backspace => Ok(KeyCodeWrapper::Backspace),
            KeyCode::Enter => Ok(KeyCodeWrapper::Enter),
            KeyCode::Left => Ok(KeyCodeWrapper::Left),
            KeyCode::Right => Ok(KeyCodeWrapper::Right),
            KeyCode::Up => Ok(KeyCodeWrapper::Up),
            KeyCode::Down => Ok(KeyCodeWrapper::Down),
            KeyCode::Home => Ok(KeyCodeWrapper::Home),
            KeyCode::End => Ok(KeyCodeWrapper::End),
            KeyCode::PageUp => Ok(KeyCodeWrapper::PageUp),
            KeyCode::PageDown => Ok(KeyCodeWrapper::PageDown),
            KeyCode::Tab => Ok(KeyCodeWrapper::Tab),
            KeyCode::BackTab => Ok(KeyCodeWrapper::BackTab),
            KeyCode::Delete => Ok(KeyCodeWrapper::Delete),
            KeyCode::Insert => Ok(KeyCodeWrapper::Insert),
            KeyCode::Esc => Ok(KeyCodeWrapper::Esc),
            _ => Err(()),
        }
    }
}

/// Wrapper around crossterm's KeyModifiers for serde support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct KeyModifiersWrapper(u8);

impl KeyModifiersWrapper {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CONTROL: Self = Self(2);
    pub const ALT: Self = Self(4);

    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl From<KeyModifiers> for KeyModifiersWrapper {
    fn from(modifiers: KeyModifiers) -> Self {
        let mut result = 0u8;
        if modifiers.contains(KeyModifiers::SHIFT) {
            result |= Self::SHIFT.0;
        }
        if modifiers.contains(KeyModifiers::CONTROL) {
            result |= Self::CONTROL.0;
        }
        if modifiers.contains(KeyModifiers::ALT) {
            result |= Self::ALT.0;
        }
        Self(result)
    }
}

impl From<KeyModifiersWrapper> for KeyModifiers {
    fn from(wrapper: KeyModifiersWrapper) -> Self {
        let mut result = KeyModifiers::empty();
        if wrapper.contains(KeyModifiersWrapper::SHIFT) {
            result |= KeyModifiers::SHIFT;
        }
        if wrapper.contains(KeyModifiersWrapper::CONTROL) {
            result |= KeyModifiers::CONTROL;
        }
        if wrapper.contains(KeyModifiersWrapper::ALT) {
            result |= KeyModifiers::ALT;
        }
        result
    }
}

impl FromStr for KeyCombo {
    type Err = KeyParseError;

    /// Parses a key combination string like "ctrl-c", "shift-up", "alt-f1", etc.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_lowercase();
        let parts: Vec<&str> = s.split('-').collect();

        let mut modifiers = KeyModifiersWrapper::NONE;
        let mut key_part = None;

        for (i, part) in parts.iter().enumerate() {
            match *part {
                "ctrl" | "control" => modifiers.0 |= KeyModifiersWrapper::CONTROL.0,
                "shift" => modifiers.0 |= KeyModifiersWrapper::SHIFT.0,
                "alt" => modifiers.0 |= KeyModifiersWrapper::ALT.0,
                _ => {
                    // This should be the key part (last element or standalone)
                    if i == parts.len() - 1 || parts.len() == 1 {
                        key_part = Some(*part);
                    } else {
                        return Err(KeyParseError::InvalidModifier(part.to_string()));
                    }
                }
            }
        }

        let key_str = key_part.ok_or(KeyParseError::MissingKey)?;
        let code = parse_key_code(key_str)?;

        Ok(KeyCombo { code, modifiers })
    }
}

/// Parse a key code from a string.
fn parse_key_code(s: &str) -> Result<KeyCodeWrapper, KeyParseError> {
    match s {
        "backspace" | "bs" => Ok(KeyCodeWrapper::Backspace),
        "enter" | "return" | "cr" => Ok(KeyCodeWrapper::Enter),
        "left" => Ok(KeyCodeWrapper::Left),
        "right" => Ok(KeyCodeWrapper::Right),
        "up" => Ok(KeyCodeWrapper::Up),
        "down" => Ok(KeyCodeWrapper::Down),
        "home" => Ok(KeyCodeWrapper::Home),
        "end" => Ok(KeyCodeWrapper::End),
        "pageup" | "pgup" => Ok(KeyCodeWrapper::PageUp),
        "pagedown" | "pgdn" => Ok(KeyCodeWrapper::PageDown),
        "tab" => Ok(KeyCodeWrapper::Tab),
        "backtab" | "btab" => Ok(KeyCodeWrapper::BackTab),
        "delete" | "del" => Ok(KeyCodeWrapper::Delete),
        "insert" | "ins" => Ok(KeyCodeWrapper::Insert),
        "esc" | "escape" => Ok(KeyCodeWrapper::Esc),
        "space" => Ok(KeyCodeWrapper::Space),
        s if s.starts_with('f') && s.len() > 1 => {
            let num: u8 = s[1..]
                .parse()
                .map_err(|_| KeyParseError::InvalidKey(s.to_string()))?;
            if num >= 1 && num <= 24 {
                Ok(KeyCodeWrapper::F(num))
            } else {
                Err(KeyParseError::InvalidKey(s.to_string()))
            }
        }
        s if s.chars().count() == 1 => Ok(KeyCodeWrapper::Char(s.chars().next().unwrap())),
        _ => Err(KeyParseError::InvalidKey(s.to_string())),
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.modifiers.contains(KeyModifiersWrapper::CONTROL) {
            parts.push("ctrl".to_string());
        }
        if self.modifiers.contains(KeyModifiersWrapper::ALT) {
            parts.push("alt".to_string());
        }
        if self.modifiers.contains(KeyModifiersWrapper::SHIFT) {
            parts.push("shift".to_string());
        }

        let key_str = match &self.code {
            KeyCodeWrapper::Char(c) => c.to_string(),
            KeyCodeWrapper::F(n) => format!("f{}", n),
            KeyCodeWrapper::Backspace => "backspace".to_string(),
            KeyCodeWrapper::Enter => "enter".to_string(),
            KeyCodeWrapper::Left => "left".to_string(),
            KeyCodeWrapper::Right => "right".to_string(),
            KeyCodeWrapper::Up => "up".to_string(),
            KeyCodeWrapper::Down => "down".to_string(),
            KeyCodeWrapper::Home => "home".to_string(),
            KeyCodeWrapper::End => "end".to_string(),
            KeyCodeWrapper::PageUp => "pageup".to_string(),
            KeyCodeWrapper::PageDown => "pagedown".to_string(),
            KeyCodeWrapper::Tab => "tab".to_string(),
            KeyCodeWrapper::BackTab => "backtab".to_string(),
            KeyCodeWrapper::Delete => "delete".to_string(),
            KeyCodeWrapper::Insert => "insert".to_string(),
            KeyCodeWrapper::Esc => "esc".to_string(),
            KeyCodeWrapper::Space => "space".to_string(),
        };
        parts.push(key_str);

        write!(f, "{}", parts.join("-"))
    }
}

#[derive(Debug, Clone)]
pub enum KeyParseError {
    InvalidModifier(String),
    InvalidKey(String),
    MissingKey,
}

impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyParseError::InvalidModifier(m) => write!(f, "Invalid modifier: {}", m),
            KeyParseError::InvalidKey(k) => write!(f, "Invalid key: {}", k),
            KeyParseError::MissingKey => write!(f, "Missing key in key combination"),
        }
    }
}

impl std::error::Error for KeyParseError {}

/// Mode-specific keybinding configuration.
/// Stores parsed KeyCombo objects as keys for efficient lookup.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModeBindings {
    pub bindings: HashMap<KeyCombo, Action>,
}

impl<'de> Deserialize<'de> for ModeBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize as a HashMap<String, Action> first, then parse the keys
        let string_map: HashMap<String, Action> = HashMap::deserialize(deserializer)?;
        let mut bindings = HashMap::with_capacity(string_map.len());

        for (key_str, action) in string_map {
            let combo = key_str
                .parse::<KeyCombo>()
                .map_err(serde::de::Error::custom)?;
            bindings.insert(combo, action);
        }

        Ok(ModeBindings { bindings })
    }
}

/// The complete keybindings configuration loaded from TOML.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingConfig {
    /// Global keybindings (mode-independent).
    #[serde(default)]
    pub global: ModeBindings,
    /// Normal mode keybindings.
    #[serde(default)]
    pub normal: ModeBindings,
    /// Visual mode keybindings.
    #[serde(default)]
    pub visual: ModeBindings,
    /// Filter mode keybindings.
    #[serde(default)]
    pub filter: ModeBindings,
    /// Config mode keybindings.
    #[serde(default)]
    pub config: ModeBindings,
    /// Prompt mode keybindings.
    #[serde(default)]
    pub prompt: ModeBindings,
    /// Help mode keybindings.
    #[serde(default)]
    pub help: ModeBindings,
}

impl KeybindingConfig {
    /// Load keybindings from a TOML file.
    pub fn from_toml_file(path: &Path) -> Result<Self, KeybindingLoadError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Parse keybindings from a TOML string.
    pub fn from_toml_str(content: &str) -> Result<Self, KeybindingLoadError> {
        let config: KeybindingConfig = toml::from_str(content)?;
        Ok(config)
    }

    /// Look up an action for a given key combo in a specific mode.
    pub fn lookup(&self, mode: InputMode, combo: &KeyCombo) -> Option<&Action> {
        // First check mode-specific bindings
        let mode_bindings = match mode {
            InputMode::Normal => &self.normal,
            InputMode::Visual => &self.visual,
            InputMode::Filter => &self.filter,
            InputMode::Config => &self.config,
            InputMode::Prompt(_) => &self.prompt,
            InputMode::Help => &self.help,
        };

        mode_bindings
            .bindings
            .get(combo)
            .or_else(|| self.global.bindings.get(combo))
    }
}

#[derive(Debug)]
pub enum KeybindingLoadError {
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl From<std::io::Error> for KeybindingLoadError {
    fn from(err: std::io::Error) -> Self {
        KeybindingLoadError::Io(err)
    }
}

impl From<toml::de::Error> for KeybindingLoadError {
    fn from(err: toml::de::Error) -> Self {
        KeybindingLoadError::Parse(err)
    }
}

impl fmt::Display for KeybindingLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeybindingLoadError::Io(e) => write!(f, "IO error loading keybindings: {}", e),
            KeybindingLoadError::Parse(e) => write!(f, "Parse error in keybindings: {}", e),
        }
    }
}

impl std::error::Error for KeybindingLoadError {}

/// The main keybinding handler.
pub enum Keybinding {
    /// The keybindings are hardcoded into the program.
    Hardcoded,
    /// Custom keybindings loaded from a configuration file.
    #[allow(dead_code)]
    Custom {
        config: KeybindingConfig,
        /// Fall back to hardcoded bindings if custom binding not found.
        fallback: bool,
    },
}

impl Default for Keybinding {
    fn default() -> Self {
        Keybinding::Hardcoded
    }
}

impl Keybinding {
    /// Create a new custom keybinding handler from a config.
    #[allow(dead_code)]
    pub fn custom(config: KeybindingConfig, fallback: bool) -> Self {
        Keybinding::Custom { config, fallback }
    }

    /// Load custom keybindings from a TOML file.
    #[allow(dead_code)]
    pub fn from_toml_file(path: &Path, fallback: bool) -> Result<Self, KeybindingLoadError> {
        let config = KeybindingConfig::from_toml_file(path)?;
        Ok(Keybinding::Custom { config, fallback })
    }

    pub fn map_key(&self, input_mode: InputMode, event: &mut Event) -> Option<Action> {
        match self {
            Self::Hardcoded => Self::native_keys(input_mode, event),
            Self::Custom { config, fallback } => {
                if let Event::Key(key) = event {
                    if key.kind != KeyEventKind::Press {
                        return None;
                    }

                    // Try to convert the key event to a KeyCombo
                    if let Ok(code_wrapper) = KeyCodeWrapper::try_from(key.code) {
                        let combo = KeyCombo {
                            code: code_wrapper,
                            modifiers: KeyModifiersWrapper::from(key.modifiers),
                        };

                        // Look up in custom config
                        if let Some(action) = config.lookup(input_mode, &combo) {
                            return Some(action.clone());
                        }
                    }

                    if let InputMode::Prompt(_) = input_mode
                        && let KeyCode::Char(to_insert) = key.code
                    {
                        return Some(Action::Command(CommandAction::Type { input: to_insert }));
                    }

                    // Fallback to native keys if enabled
                    if *fallback {
                        return Self::native_keys(input_mode, event);
                    }
                }
                None
            }
        }
    }

    fn native_keys(input_mode: InputMode, event: &mut Event) -> Option<Action> {
        match (input_mode, event) {
            (_, Event::Key(key)) => {
                if key.kind != KeyEventKind::Press {
                    return None;
                }

                Self::mode_dependent_bind(input_mode, key)
                    .or_else(|| Self::mode_independent_bind(key))
            }
            (InputMode::Prompt(_), Event::Paste(input)) => {
                Some(Action::Command(CommandAction::Paste {
                    input: std::mem::take(input),
                }))
            }
            _ => None,
        }
    }

    fn mode_dependent_bind(input_mode: InputMode, key: &mut KeyEvent) -> Option<Action> {
        match input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Up | KeyCode::Down => Some(Action::Normal(NormalAction::PanVertical {
                    direction: Direction::back_if(key.code == KeyCode::Up),
                    delta: if key.modifiers.contains(KeyModifiers::SHIFT) {
                        ViewDelta::HalfPage
                    } else {
                        ViewDelta::Number { value: 1 }
                    },
                    target_view: None,
                })),
                KeyCode::Left | KeyCode::Right => {
                    Some(Action::Normal(NormalAction::PanHorizontal {
                        direction: Direction::back_if(key.code == KeyCode::Left),
                        delta: if key.modifiers.contains(KeyModifiers::SHIFT) {
                            ViewDelta::HalfPage
                        } else {
                            ViewDelta::Number { value: 1 }
                        },
                        target_view: None,
                    }))
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    Some(Action::Normal(NormalAction::PanVertical {
                        direction: Direction::Back,
                        delta: ViewDelta::Boundary,
                        target_view: None,
                    }))
                }
                KeyCode::End | KeyCode::Char('G') => {
                    Some(Action::Normal(NormalAction::FollowOutput))
                }
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Char(' ') => {
                    Some(Action::Normal(NormalAction::PanVertical {
                        direction: Direction::back_if(key.code == KeyCode::PageUp),
                        delta: ViewDelta::Page,
                        target_view: None,
                    }))
                }
                KeyCode::Char(c @ ('u' | 'd')) => Some(Action::Normal(NormalAction::PanVertical {
                    direction: Direction::back_if(c == 'u'),
                    delta: ViewDelta::HalfPage,
                    target_view: None,
                })),
                KeyCode::Char(c @ ('N' | 'n')) => Some(Action::Normal(NormalAction::PanVertical {
                    direction: Direction::back_if(c == 'N'),
                    delta: ViewDelta::Match,
                    target_view: None,
                })),
                _ => None,
            },
            InputMode::Help => match key.code {
                KeyCode::Up | KeyCode::Down => Some(Action::Help(HelpAction::PanVertical {
                    direction: Direction::back_if(key.code == KeyCode::Up),
                    delta: if key.modifiers.contains(KeyModifiers::SHIFT) {
                        ViewDelta::HalfPage
                    } else {
                        ViewDelta::Number { value: 1 }
                    },
                })),
                KeyCode::Home | KeyCode::Char('g') => Some(Action::Help(HelpAction::PanVertical {
                    direction: Direction::Back,
                    delta: ViewDelta::Boundary,
                })),
                KeyCode::PageUp | KeyCode::PageDown | KeyCode::Char(' ') => {
                    Some(Action::Help(HelpAction::PanVertical {
                        direction: Direction::back_if(key.code == KeyCode::PageUp),
                        delta: ViewDelta::Page,
                    }))
                }
                KeyCode::Char(c @ ('u' | 'd')) => Some(Action::Help(HelpAction::PanVertical {
                    direction: Direction::back_if(c == 'u'),
                    delta: ViewDelta::HalfPage,
                })),
                KeyCode::Char(c @ ('N' | 'n')) => Some(Action::Help(HelpAction::PanVertical {
                    direction: Direction::back_if(c == 'N'),
                    delta: ViewDelta::Match,
                })),
                _ => None,
            },
            InputMode::Filter => match key.code {
                KeyCode::Char('w') | KeyCode::Char('s') => {
                    Some(Action::Filter(FilterAction::Displace {
                        direction: Direction::back_if(key.code == KeyCode::Char('w')),
                        delta: ViewDelta::Number { value: 1 },
                    }))
                }
                KeyCode::Up | KeyCode::Down => Some(Action::Filter(FilterAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Up),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Number { value: 1 },
                })),
                KeyCode::Home | KeyCode::End => Some(Action::Filter(FilterAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Home),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Boundary,
                })),
                KeyCode::PageUp | KeyCode::PageDown => Some(Action::Filter(FilterAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::PageUp),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Page,
                })),
                KeyCode::Char('/') => {
                    Some(Action::SwitchMode(InputMode::Prompt(PromptMode::Search {
                        escaped: false,
                        edit: true,
                    })))
                }
                KeyCode::Char('c') => Some(Action::SwitchMode(InputMode::Prompt(
                    PromptMode::FilterColor,
                ))),
                KeyCode::Char(c @ ('u' | 'd')) => Some(Action::Filter(FilterAction::Move {
                    direction: Direction::back_if(c == 'u'),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::HalfPage,
                })),
                KeyCode::Char(' ') | KeyCode::Enter => {
                    Some(Action::Filter(FilterAction::ToggleSelectedFilter))
                }
                KeyCode::Backspace => Some(Action::Filter(FilterAction::RemoveSelectedFilter)),
                _ => None,
            },
            InputMode::Config => match key.code {
                KeyCode::Up | KeyCode::Down => Some(Action::Config(ConfigAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Up),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Number { value: 1 },
                })),
                KeyCode::Home | KeyCode::End => Some(Action::Config(ConfigAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Home),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Boundary,
                })),
                KeyCode::PageUp | KeyCode::PageDown => Some(Action::Config(ConfigAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::PageUp),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Page,
                })),
                KeyCode::Char(c @ ('u' | 'd')) => Some(Action::Config(ConfigAction::Move {
                    direction: Direction::back_if(c == 'u'),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::HalfPage,
                })),
                KeyCode::Enter => Some(Action::Config(ConfigAction::LoadSelectedFilter)),
                KeyCode::Backspace => Some(Action::Config(ConfigAction::RemoveSelectedFilter)),
                _ => None,
            },
            InputMode::Visual => match key.code {
                KeyCode::Up | KeyCode::Down => Some(Action::Visual(VisualAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Up),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: if key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
                    {
                        ViewDelta::HalfPage
                    } else {
                        ViewDelta::Number { value: 1 }
                    },
                })),
                KeyCode::Char(c @ ('n' | 'N')) => Some(Action::Visual(VisualAction::Move {
                    direction: Direction::back_if(c.to_ascii_lowercase() == 'N'),
                    delta: ViewDelta::Match,
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                })),
                KeyCode::Home | KeyCode::End => Some(Action::Visual(VisualAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Home),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Boundary,
                })),
                KeyCode::PageUp | KeyCode::PageDown => Some(Action::Visual(VisualAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::PageUp),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    delta: ViewDelta::Page,
                })),
                KeyCode::Char(' ') | KeyCode::Enter => {
                    Some(Action::Visual(VisualAction::ToggleSelectedLine))
                }
                _ => None,
            },
            InputMode::Prompt(prompt_mode) => match key.code {
                KeyCode::Enter => Some(Action::Command(CommandAction::Submit)),
                KeyCode::Left | KeyCode::Right => Some(Action::Command(CommandAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Left),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    jump: if key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
                    {
                        CommandJump::Word
                    } else {
                        CommandJump::None
                    },
                })),
                KeyCode::Home | KeyCode::End => Some(Action::Command(CommandAction::Move {
                    direction: Direction::back_if(key.code == KeyCode::Home),
                    select: key.modifiers.contains(KeyModifiers::SHIFT),
                    jump: CommandJump::Boundary,
                })),
                KeyCode::Up | KeyCode::Down => Some(Action::Command(CommandAction::History {
                    direction: Direction::back_if(key.code == KeyCode::Up),
                })),
                KeyCode::Backspace => Some(Action::Command(CommandAction::Backspace)),
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    match prompt_mode {
                        PromptMode::Search { escaped, edit } => {
                            Some(Action::SwitchMode(InputMode::Prompt(PromptMode::Search {
                                escaped: !escaped,
                                edit,
                            })))
                        }
                        _ => None,
                    }
                }
                KeyCode::Char(to_insert) => match to_insert {
                    'b' | 'f' if key.modifiers.contains(KeyModifiers::ALT) => {
                        Some(Action::Command(CommandAction::Move {
                            direction: Direction::back_if(to_insert == 'b'),
                            select: key.modifiers.contains(KeyModifiers::SHIFT),
                            jump: CommandJump::Word,
                        }))
                    }
                    'a' | 'e' if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        Some(Action::Command(CommandAction::Move {
                            direction: Direction::back_if(to_insert == 'a'),
                            select: key.modifiers.contains(KeyModifiers::SHIFT),
                            jump: CommandJump::Boundary,
                        }))
                    }
                    input => Some(Action::Command(CommandAction::Type { input })),
                },
                KeyCode::Tab => Some(Action::Command(CommandAction::Complete)),
                _ => None,
            },
        }
    }

    fn mode_independent_bind(key: &mut KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char(':') => Some(Action::SwitchMode(InputMode::Prompt(PromptMode::Command))),
            KeyCode::Char('/') => Some(Action::SwitchMode(InputMode::Prompt(PromptMode::Search {
                escaped: false,
                edit: false,
            }))),
            KeyCode::Char('!') => Some(Action::SwitchMode(InputMode::Prompt(PromptMode::Shell {
                pipe: false,
            }))),
            // TODO: feature still in development
            // KeyCode::Char('|') => {
            //     Some(Action::SwitchMode { mode: InputMode::Prompt(PromptMode::Shell { pipe: true }) })
            // }
            KeyCode::Char('f') => Some(Action::SwitchMode(InputMode::Filter)),
            KeyCode::Tab => Some(Action::Normal(NormalAction::SwitchActive {
                direction: Direction::Next,
            })),
            KeyCode::Esc => Some(Action::SwitchMode(InputMode::Normal)),
            KeyCode::Char('v') => Some(Action::SwitchMode(InputMode::Visual)),
            KeyCode::BackTab => Some(Action::Normal(NormalAction::SwitchActive {
                direction: Direction::Back,
            })),
            KeyCode::Char(c @ '1'..='9') => Some(Action::Normal(NormalAction::SwitchActiveIndex {
                target_view: c as usize - '1' as usize,
            })),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Exit)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_key() {
        let combo: KeyCombo = "a".parse().unwrap();
        assert_eq!(combo.code, KeyCodeWrapper::Char('a'));
        assert_eq!(combo.modifiers, KeyModifiersWrapper::NONE);
    }

    #[test]
    fn test_parse_ctrl_key() {
        let combo: KeyCombo = "ctrl-c".parse().unwrap();
        assert_eq!(combo.code, KeyCodeWrapper::Char('c'));
        assert!(combo.modifiers.contains(KeyModifiersWrapper::CONTROL));
    }

    #[test]
    fn test_parse_shift_key() {
        let combo: KeyCombo = "shift-up".parse().unwrap();
        assert_eq!(combo.code, KeyCodeWrapper::Up);
        assert!(combo.modifiers.contains(KeyModifiersWrapper::SHIFT));
    }

    #[test]
    fn test_parse_alt_key() {
        let combo: KeyCombo = "alt-f1".parse().unwrap();
        assert_eq!(combo.code, KeyCodeWrapper::F(1));
        assert!(combo.modifiers.contains(KeyModifiersWrapper::ALT));
    }

    #[test]
    fn test_parse_combined_modifiers() {
        let combo: KeyCombo = "ctrl-shift-a".parse().unwrap();
        assert_eq!(combo.code, KeyCodeWrapper::Char('a'));
        assert!(combo.modifiers.contains(KeyModifiersWrapper::CONTROL));
        assert!(combo.modifiers.contains(KeyModifiersWrapper::SHIFT));
    }

    #[test]
    fn test_parse_special_keys() {
        assert_eq!(
            "enter".parse::<KeyCombo>().unwrap().code,
            KeyCodeWrapper::Enter
        );
        assert_eq!(
            "backspace".parse::<KeyCombo>().unwrap().code,
            KeyCodeWrapper::Backspace
        );
        assert_eq!("tab".parse::<KeyCombo>().unwrap().code, KeyCodeWrapper::Tab);
        assert_eq!("esc".parse::<KeyCombo>().unwrap().code, KeyCodeWrapper::Esc);
        assert_eq!(
            "space".parse::<KeyCombo>().unwrap().code,
            KeyCodeWrapper::Space
        );
        assert_eq!(
            "pageup".parse::<KeyCombo>().unwrap().code,
            KeyCodeWrapper::PageUp
        );
        assert_eq!(
            "pagedown".parse::<KeyCombo>().unwrap().code,
            KeyCodeWrapper::PageDown
        );
    }

    #[test]
    fn test_key_combo_display() {
        let combo = KeyCombo {
            code: KeyCodeWrapper::Char('c'),
            modifiers: KeyModifiersWrapper::CONTROL,
        };
        assert_eq!(combo.to_string(), "ctrl-c");

        let combo2 = KeyCombo {
            code: KeyCodeWrapper::Up,
            modifiers: KeyModifiersWrapper::NONE,
        };
        assert_eq!(combo2.to_string(), "up");
    }

    #[test]
    fn test_roundtrip_parse_display() {
        let keys = ["ctrl-c", "shift-up", "alt-f1", "enter", "a", "space"];
        for key in keys {
            let combo: KeyCombo = key.parse().unwrap();
            assert_eq!(combo.to_string(), key);
        }
    }

    #[test]
    fn test_parse_toml_simple() {
        let toml = r#"
[global]
"ctrl-c" = { action = "exit" }
"#;
        let config = KeybindingConfig::from_toml_str(toml).unwrap();
        let ctrl_c: KeyCombo = "ctrl-c".parse().unwrap();
        assert!(config.global.bindings.contains_key(&ctrl_c));
    }

    #[test]
    fn test_parse_toml_with_delta() {
        let toml = r#"
[normal]
"up" = { action = "normal", type = "pan_vertical", direction = "back", delta = { type = "number", value = 1 } }
"pageup" = { action = "normal", type = "pan_vertical", direction = "back", delta = { type = "page" } }
"#;
        let config = KeybindingConfig::from_toml_str(toml).unwrap();
        let up: KeyCombo = "up".parse().unwrap();
        let pageup: KeyCombo = "pageup".parse().unwrap();
        assert!(config.normal.bindings.contains_key(&up));
        assert!(config.normal.bindings.contains_key(&pageup));
    }

    #[test]
    fn test_parse_default_keybindings_file() {
        let toml = include_str!("../../../default_keybindings.toml");
        let config = KeybindingConfig::from_toml_str(toml);
        assert!(
            config.is_ok(),
            "Failed to parse default keybindings: {:?}",
            config.err()
        );
        let config = config.unwrap();

        // Verify some expected bindings exist
        let ctrl_c: KeyCombo = "ctrl-c".parse().unwrap();
        let up: KeyCombo = "up".parse().unwrap();
        let space: KeyCombo = "space".parse().unwrap();
        let backspace: KeyCombo = "backspace".parse().unwrap();
        let enter: KeyCombo = "enter".parse().unwrap();

        assert!(config.global.bindings.contains_key(&ctrl_c));
        assert!(config.normal.bindings.contains_key(&up));
        assert!(config.visual.bindings.contains_key(&space));
        assert!(config.filter.bindings.contains_key(&backspace));
        assert!(config.prompt.bindings.contains_key(&enter));
    }
}
