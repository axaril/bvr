use ratatui::{palette::Hsl, style::Color};

pub const WHITE: Color = Color::Indexed(255);
pub const BLACK: Color = Color::Indexed(16);
pub const BG: Color = Color::Reset;

pub const TEXT_ACTIVE: Color = Color::Indexed(253);
pub const TEXT_INACTIVE: Color = Color::Indexed(238);

pub const GUTTER_BG: Color = BG;
pub const GUTTER_TEXT: Color = Color::Indexed(241);

pub const TAB_INACTIVE: Color = Color::Indexed(235);
pub const TAB_ACTIVE: Color = Color::Indexed(239);
pub const TAB_SIDE_ACTIVE: Color = Color::Indexed(39);
pub const TAB_SIDE_INACTIVE: Color = Color::Black;

pub const STATUS_BAR: Color = Color::Indexed(235);
pub const STATUS_BAR_TEXT: Color = Color::Indexed(246);

pub const COMMAND_BAR_SELECT: Color = Color::Indexed(69);

pub const NORMAL_ACCENT: Color = Color::Indexed(75);
pub const COMMAND_ACCENT: Color = Color::Indexed(48);
pub const SELECT_ACCENT: Color = Color::Indexed(170);
pub const FILTER_ACCENT: Color = Color::Indexed(178);
pub const CONFIG_ACCENT: Color = Color::Indexed(213);

pub const SHELL_ACCENT: Color = Color::Indexed(161);

pub const ERROR: Color = Color::Red;

pub mod regex {
    use ratatui::style::Color;

    /// Rainbow colors for nested capture groups `()` — cycles by nesting depth.
    pub const GROUP: [Color; 5] = [
        Color::Indexed(39),  // bright blue   – depth 0
        Color::Indexed(82),  // bright green  – depth 1
        Color::Indexed(214), // orange        – depth 2
        Color::Indexed(170), // magenta       – depth 3
        Color::Indexed(44),  // cyan          – depth 4
    ];
    /// `[` and `]` character-class delimiters.
    pub const CLASS: Color = Color::Indexed(222); // light yellow
    /// Quantifiers: `*`, `+`, `?`, `{n,m}` and their lazy variants.
    pub const QUANTIFIER: Color = Color::Indexed(141); // light purple
    /// Anchors: `^` and `$`.
    pub const ANCHOR: Color = Color::Indexed(120); // light green
    /// Escape sequences: `\d`, `\w`, `\.`, etc.
    pub const ESCAPE: Color = Color::Indexed(81); // sky blue
    /// Other metacharacters: `.` (any-char) and `|` (alternation).
    pub const META: Color = Color::Indexed(203); // salmon
}


pub enum ColorSelector {
    Color256 { index: u8 },
    TrueColor { hue: f32 },
}

impl ColorSelector {
    pub fn new() -> Self {
        use supports_color::Stream;

        if let Some(support) = supports_color::on(Stream::Stdout) {
            if support.has_16m {
                return Self::TrueColor { hue: 0.0 };
            } else if support.has_256 {
                return Self::Color256 { index: 0 };
            }
        }

        panic!("Application requires at least 256-color support");
    }

    pub fn reset(&mut self) {
        match self {
            ColorSelector::Color256 { index } => *index = 0,
            ColorSelector::TrueColor { hue } => *hue = 0.0,
        }
    }

    pub fn peek_color(&self) -> Color {
        match self {
            ColorSelector::Color256 { index } => Color::Indexed(index + 9),
            ColorSelector::TrueColor { hue } => Color::from_hsl(Hsl::new(*hue, 0.8, 0.5)),
        }
    }

    pub fn next_color(&mut self) -> Color {
        let color = self.peek_color();
        match self {
            ColorSelector::Color256 { index } => {
                *index += 27;
                *index %= 230;
            }
            ColorSelector::TrueColor { hue } => {
                *hue += 208.3;
                *hue %= 360.0;
            }
        }
        color
    }
}
