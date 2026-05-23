use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

#[allow(dead_code)]
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for grapheme in text.graphemes(true) {
        let grapheme_width = grapheme
            .chars()
            .map(|ch| ch.width().unwrap_or(0))
            .sum::<usize>();

        if current_width + grapheme_width > max_width && !current_line.is_empty() {
            lines.push(current_line);
            current_line = String::new();
            current_width = 0;
        }

        current_line.push_str(grapheme);
        current_width += grapheme_width;
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}
