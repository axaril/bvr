use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{InputMode, control::ViewDelta};
use crate::direction::Direction;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Exit,
    SwitchMode(InputMode),
    Normal(NormalAction),
    Visual(VisualAction),
    Filter(FilterAction),
    Config(ConfigAction),
    Command(CommandAction),
    ExportFile { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NormalAction {
    PanVertical {
        direction: Direction,
        delta: ViewDelta,
        target_view: Option<usize>,
    },
    PanHorizontal {
        direction: Direction,
        delta: ViewDelta,
        target_view: Option<usize>,
    },
    FollowOutput,
    SwitchActive {
        direction: Direction,
    },
    SwitchActiveIndex {
        target_view: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisualAction {
    Move {
        direction: Direction,
        select: bool,
        delta: ViewDelta,
    },
    ToggleSelectedLine,
    ToggleLine {
        target_view: usize,
        line_number: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilterAction {
    Move {
        direction: Direction,
        select: bool,
        delta: ViewDelta,
    },
    Displace {
        direction: Direction,
        delta: ViewDelta,
    },
    ToggleSelectedFilter,
    RemoveSelectedFilter,
    ToggleSpecificFilter {
        target_view: usize,
        filter_index: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigAction {
    Move {
        direction: Direction,
        select: bool,
        delta: ViewDelta,
    },
    LoadSelectedFilter,
    RemoveSelectedFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandAction {
    Move {
        direction: Direction,
        select: bool,
        jump: CommandJump,
    },
    History {
        direction: Direction,
    },
    Type {
        input: char,
    },
    Paste {
        input: String,
    },
    Backspace,
    Submit,
    Complete,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandJump {
    Word,
    Boundary,
    None,
}
