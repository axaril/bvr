use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum InputMode {
    Prompt(PromptMode),
    Normal,
    Visual,
    Filter,
    Config,
}

impl InputMode {
    pub fn is_prompt_search(&self) -> bool {
        matches!(self, InputMode::Prompt(PromptMode::Search { .. }))
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "prompt", rename_all = "snake_case")]
pub enum PromptMode {
    Command,
    Shell { pipe: bool },
    Search { escaped: bool, edit: bool },
    FilterColor,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewDelta {
    Number { value: u16 },
    Page,
    HalfPage,
    Boundary,
    Match,
}
