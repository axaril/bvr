use ratatui::layout::Rect;

pub fn split_top(area: Rect, top_height: u16) -> Option<[Rect; 2]> {
    let bottom_height = area.height.checked_sub(top_height)?;
    Some([
        Rect { height: top_height, ..area },
        Rect { y: area.y + top_height, height: bottom_height, ..area },
    ])
}

pub fn split_bottom(area: Rect, bottom_height: u16) -> Option<[Rect; 2]> {
    let top_height = area.height.checked_sub(bottom_height)?;
    Some([
        Rect { height: top_height, ..area },
        Rect { y: area.y + top_height, height: bottom_height, ..area },
    ])
}

pub fn split_left(area: Rect, left_width: u16) -> Option<[Rect; 2]> {
    let right_width = area.width.checked_sub(left_width)?;
    Some([
        Rect { width: left_width, ..area },
        Rect { x: area.x + left_width, width: right_width, ..area },
    ])
}

pub fn split_half(area: Rect) -> Option<[Rect; 2]> {
    split_left(area, area.width / 2)
}

pub fn split_columns(area: Rect, n: u16) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let base_width = area.width / n;
    let remainder = area.width % n;
    (0..n)
        .scan(area.x, |x, i| {
            let width = base_width + u16::from(i < remainder);
            let chunk = Rect { x: *x, width, ..area };
            *x += width;
            Some(chunk)
        })
        .collect()
}
