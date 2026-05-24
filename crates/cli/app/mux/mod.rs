mod state;
mod widget;
mod tab;

use ratatui::layout::Rect;
pub use state::MultiplexerApp as State;
pub use state::MultiplexerMode as Mode;
pub use widget::MultiplexerWidget as Widget;
pub use tab::TabWidget;

const FILTER_MAX_HEIGHT: u16 = 10;

pub fn filter_area(area: &mut Rect, f: impl FnOnce(Rect)) {
    let [view_chunk, filter_chunk] =
        split_bottom(*area, FILTER_MAX_HEIGHT);
    f(filter_chunk);
    *area = view_chunk;
}


fn split_top(area: Rect, top_height: u16) -> [Rect; 2] {
    let mut tab_chunk = area;
    tab_chunk.height = top_height;

    let mut data_chunk = area;
    data_chunk.y += top_height;
    data_chunk.height = data_chunk.height.saturating_sub(top_height);

    [tab_chunk, data_chunk]
}

fn split_bottom(area: Rect, bottom_height: u16) -> [Rect; 2] {
    let mut view_chunk = area;
    view_chunk.height = view_chunk.height.saturating_sub(bottom_height);

    let mut filter_chunk = area;
    filter_chunk.y = area.y + view_chunk.height;
    filter_chunk.height = bottom_height.min(area.height);

    [view_chunk, filter_chunk]
}

pub fn split_mux(area: Rect) -> [Rect; 4] {
    let [area, cmd_chunk] = split_bottom(area, 1);
    let [area, status_chunk] = split_bottom(area, 1);
    let [tab_chunk, mux_chunk] = split_top(area, 1);
    [tab_chunk, mux_chunk, status_chunk, cmd_chunk]
}
