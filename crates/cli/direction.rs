use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Back,
    Next,
}

impl Direction {
    #[inline(always)]
    pub fn back_if(condition: bool) -> Self {
        if condition { Self::Back } else { Self::Next }
    }
}
