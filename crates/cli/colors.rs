use ratatui::style::Color;

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
pub const HELP_ACCENT: Color = Color::Indexed(40);

pub const SHELL_ACCENT: Color = Color::Indexed(161);

pub const ERROR: Color = Color::Red;

const PALETTE_ANSI: [Color; 12] = [
    Color::Red,
    Color::Blue,
    Color::Yellow,
    Color::Green,
    Color::Magenta,
    Color::Cyan,
    Color::LightRed,
    Color::LightBlue,
    Color::LightYellow,
    Color::LightGreen,
    Color::LightMagenta,
    Color::LightCyan,
];

const PALETTE_256: [u8; 32] = [
    196, 33, 220, 46, 201, 51, 203, 39, 226, 41, 207, 87, 167, 75, 178, 78, 170, 80, 210, 69, 184,
    119, 213, 50, 174, 111, 222, 84, 177, 117, 217, 27,
];

pub mod regex {
    use ratatui::style::Color;

    /// Rainbow colors for nested capture groups `()` — cycles by nesting depth.
    pub const GROUP: [Color; 5] = [
        Color::Indexed(82),  // bright green
        Color::Indexed(39),  // bright blue
        Color::Indexed(44),  // cyan
        Color::Indexed(170), // magenta
        Color::Indexed(214), // orange
    ];
    /// `[` and `]` character-class delimiters.
    pub const CLASS: Color = Color::Yellow;
    /// Quantifiers: `*`, `+`, `?`, `{n,m}` and their lazy variants.
    pub const QUANTIFIER: Color = Color::Blue;
    /// Anchors: `^` and `$`.
    pub const ANCHOR: Color = Color::Magenta;
    /// Escape sequences: `\d`, `\w`, `\.`, etc.
    pub const ESCAPE: Color = Color::Yellow;
    /// Other metacharacters: `.` (any-char) and `|` (alternation).
    pub const META: Color = Color::Cyan;
}

pub enum ColorSelector {
    Ansi { index: usize },
    Color256 { index: usize },
}

impl ColorSelector {
    pub fn new() -> Self {
        use supports_color::Stream;

        if let Some(support) = supports_color::on(Stream::Stdout) {
            if support.has_256 {
                return Self::Color256 { index: 0 };
            } else if support.has_256 {
                return Self::Ansi { index: 0 };
            }
        }

        panic!("Application requires at least color support");
    }

    pub fn reset(&mut self) {
        match self {
            ColorSelector::Ansi { index } => *index = 0,
            ColorSelector::Color256 { index } => *index = 0,
        }
    }

    pub fn peek_color(&self) -> Color {
        match self {
            ColorSelector::Ansi { index } => PALETTE_ANSI[*index % PALETTE_ANSI.len()],
            ColorSelector::Color256 { index } => {
                Color::Indexed(PALETTE_256[*index % PALETTE_256.len()])
            }
        }
    }

    pub fn next_color(&mut self) -> Color {
        let color = self.peek_color();
        match self {
            ColorSelector::Ansi { index } => *index = (*index + 1) % PALETTE_ANSI.len(),
            ColorSelector::Color256 { index } => *index = (*index + 1) % PALETTE_256.len(),
        }
        color
    }
}
