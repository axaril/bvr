mod state;
mod tab;
mod widget;

use ratatui::layout::Rect;
pub use state::MultiplexerApp as State;
pub use state::MultiplexerMode as Mode;
pub use tab::TabWidget;
pub use widget::MultiplexerWidget as Widget;

pub fn split_mux(area: Rect) -> Option<[Rect; 4]> {
    let [area, cmd_chunk] = crate::split::split_bottom(area, 1)?;
    let [area, status_chunk] = crate::split::split_bottom(area, 1)?;
    let [tab_chunk, mux_chunk] = crate::split::split_top(area, 1)?;
    Some([tab_chunk, mux_chunk, status_chunk, cmd_chunk])
}
