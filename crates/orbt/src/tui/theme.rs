use ratatui::style::Color;

pub struct ThemeColors {
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_card: Color,
    pub fg_primary: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_idle: Color,
    pub accent_blocked: Color,
    pub accent_error: Color,
    pub border: Color,
    /// Border for panes that are not focused. Kept noticeably lighter than
    /// `border` so the pane edges stay readable against `bg_primary`.
    pub border_dim: Color,
}

impl ThemeColors {
    // "orbt" — Tokyo Night (default)
    pub const fn orbt() -> Self {
        Self {
            bg_primary: Color::Rgb(26, 27, 38),
            bg_secondary: Color::Rgb(36, 40, 59),
            bg_tertiary: Color::Rgb(43, 47, 68),
            bg_card: Color::Rgb(41, 46, 68),
            fg_primary: Color::Rgb(169, 177, 214),
            fg_secondary: Color::Rgb(148, 156, 187),
            fg_muted: Color::Rgb(90, 95, 128),
            accent: Color::Rgb(187, 154, 247),
            accent_hover: Color::Rgb(165, 130, 220),
            accent_idle: Color::Rgb(125, 207, 255),
            accent_blocked: Color::Rgb(224, 175, 104),
            accent_error: Color::Rgb(247, 118, 142),
            border: Color::Rgb(45, 48, 70),
            border_dim: Color::Rgb(92, 99, 140),
        }
    }

    // "orange" — original Orbit orange accent
    pub const fn orange() -> Self {
        Self {
            bg_primary: Color::Rgb(14, 14, 20),
            bg_secondary: Color::Rgb(18, 18, 26),
            bg_tertiary: Color::Rgb(24, 24, 33),
            bg_card: Color::Rgb(24, 24, 34),
            fg_primary: Color::Rgb(242, 242, 248),
            fg_secondary: Color::Rgb(180, 180, 196),
            fg_muted: Color::Rgb(120, 120, 140),
            accent: Color::Rgb(217, 119, 6),
            accent_hover: Color::Rgb(161, 86, 0),
            accent_idle: Color::Rgb(96, 120, 158),
            accent_blocked: Color::Rgb(217, 172, 0),
            accent_error: Color::Rgb(200, 50, 30),
            border: Color::Rgb(60, 60, 76),
            border_dim: Color::Rgb(110, 110, 132),
        }
    }

    // "catppuccin" — Catppuccin Mocha
    pub const fn catppuccin() -> Self {
        Self {
            bg_primary: Color::Rgb(24, 24, 37),
            bg_secondary: Color::Rgb(30, 30, 46),
            bg_tertiary: Color::Rgb(37, 38, 56),
            bg_card: Color::Rgb(30, 30, 46),
            fg_primary: Color::Rgb(205, 214, 244),
            fg_secondary: Color::Rgb(166, 173, 200),
            fg_muted: Color::Rgb(108, 112, 134),
            accent: Color::Rgb(203, 166, 247),
            accent_hover: Color::Rgb(180, 142, 225),
            accent_idle: Color::Rgb(137, 220, 235),
            accent_blocked: Color::Rgb(249, 226, 175),
            accent_error: Color::Rgb(243, 139, 168),
            border: Color::Rgb(54, 58, 79),
            border_dim: Color::Rgb(104, 110, 140),
        }
    }

    // "gruvbox" — Gruvbox Dark
    pub const fn gruvbox() -> Self {
        Self {
            bg_primary: Color::Rgb(29, 32, 33),
            bg_secondary: Color::Rgb(40, 40, 40),
            bg_tertiary: Color::Rgb(50, 48, 47),
            bg_card: Color::Rgb(40, 40, 40),
            fg_primary: Color::Rgb(235, 219, 178),
            fg_secondary: Color::Rgb(213, 196, 161),
            fg_muted: Color::Rgb(146, 131, 116),
            accent: Color::Rgb(250, 189, 47),
            accent_hover: Color::Rgb(215, 153, 33),
            accent_idle: Color::Rgb(142, 192, 124),
            accent_blocked: Color::Rgb(254, 128, 25),
            accent_error: Color::Rgb(251, 73, 52),
            border: Color::Rgb(80, 73, 69),
            border_dim: Color::Rgb(140, 130, 122),
        }
    }

    // "nord" — Nord
    pub const fn nord() -> Self {
        Self {
            bg_primary: Color::Rgb(46, 52, 64),
            bg_secondary: Color::Rgb(59, 66, 82),
            bg_tertiary: Color::Rgb(67, 76, 94),
            bg_card: Color::Rgb(59, 66, 82),
            fg_primary: Color::Rgb(236, 239, 244),
            fg_secondary: Color::Rgb(216, 222, 233),
            fg_muted: Color::Rgb(129, 161, 193),
            accent: Color::Rgb(136, 192, 208),
            accent_hover: Color::Rgb(110, 170, 188),
            accent_idle: Color::Rgb(163, 190, 140),
            accent_blocked: Color::Rgb(235, 203, 139),
            accent_error: Color::Rgb(191, 97, 106),
            border: Color::Rgb(76, 86, 106),
            border_dim: Color::Rgb(132, 143, 165),
        }
    }

    // "dracula" — Dracula
    pub const fn dracula() -> Self {
        Self {
            bg_primary: Color::Rgb(21, 22, 30),
            bg_secondary: Color::Rgb(40, 42, 54),
            bg_tertiary: Color::Rgb(52, 54, 68),
            bg_card: Color::Rgb(40, 42, 54),
            fg_primary: Color::Rgb(248, 248, 242),
            fg_secondary: Color::Rgb(189, 190, 207),
            fg_muted: Color::Rgb(98, 114, 164),
            accent: Color::Rgb(189, 147, 249),
            accent_hover: Color::Rgb(163, 120, 224),
            accent_idle: Color::Rgb(80, 250, 123),
            accent_blocked: Color::Rgb(255, 184, 108),
            accent_error: Color::Rgb(255, 85, 85),
            border: Color::Rgb(68, 71, 90),
            border_dim: Color::Rgb(122, 126, 150),
        }
    }

    // "solarized" — Solarized Dark
    pub const fn solarized() -> Self {
        Self {
            bg_primary: Color::Rgb(0, 43, 54),
            bg_secondary: Color::Rgb(7, 54, 66),
            bg_tertiary: Color::Rgb(0, 43, 54),
            bg_card: Color::Rgb(7, 54, 66),
            fg_primary: Color::Rgb(253, 246, 227),
            fg_secondary: Color::Rgb(238, 232, 213),
            fg_muted: Color::Rgb(101, 123, 131),
            accent: Color::Rgb(38, 139, 210),
            accent_hover: Color::Rgb(20, 110, 185),
            accent_idle: Color::Rgb(42, 161, 152),
            accent_blocked: Color::Rgb(181, 137, 0),
            accent_error: Color::Rgb(220, 50, 47),
            border: Color::Rgb(7, 54, 66),
            border_dim: Color::Rgb(70, 130, 145),
        }
    }

    // "onedark" — One Dark Pro
    pub const fn onedark() -> Self {
        Self {
            bg_primary: Color::Rgb(24, 26, 31),
            bg_secondary: Color::Rgb(33, 37, 43),
            bg_tertiary: Color::Rgb(40, 44, 52),
            bg_card: Color::Rgb(33, 37, 43),
            fg_primary: Color::Rgb(171, 178, 191),
            fg_secondary: Color::Rgb(140, 148, 164),
            fg_muted: Color::Rgb(92, 99, 112),
            accent: Color::Rgb(97, 175, 239),
            accent_hover: Color::Rgb(75, 150, 215),
            accent_idle: Color::Rgb(152, 195, 121),
            accent_blocked: Color::Rgb(229, 192, 123),
            accent_error: Color::Rgb(224, 108, 117),
            border: Color::Rgb(58, 64, 74),
            border_dim: Color::Rgb(108, 116, 132),
        }
    }

    // "everforest" — Everforest Dark
    pub const fn everforest() -> Self {
        Self {
            bg_primary: Color::Rgb(29, 32, 33),
            bg_secondary: Color::Rgb(37, 42, 37),
            bg_tertiary: Color::Rgb(45, 53, 45),
            bg_card: Color::Rgb(37, 42, 37),
            fg_primary: Color::Rgb(211, 198, 170),
            fg_secondary: Color::Rgb(184, 170, 142),
            fg_muted: Color::Rgb(127, 143, 111),
            accent: Color::Rgb(167, 192, 128),
            accent_hover: Color::Rgb(140, 168, 100),
            accent_idle: Color::Rgb(131, 192, 169),
            accent_blocked: Color::Rgb(219, 188, 127),
            accent_error: Color::Rgb(230, 126, 128),
            border: Color::Rgb(74, 84, 68),
            border_dim: Color::Rgb(128, 140, 118),
        }
    }

    // "kanagawa" — Kanagawa Wave
    pub const fn kanagawa() -> Self {
        Self {
            bg_primary: Color::Rgb(22, 22, 29),
            bg_secondary: Color::Rgb(31, 31, 40),
            bg_tertiary: Color::Rgb(41, 41, 53),
            bg_card: Color::Rgb(31, 31, 40),
            fg_primary: Color::Rgb(220, 215, 186),
            fg_secondary: Color::Rgb(192, 187, 157),
            fg_muted: Color::Rgb(113, 119, 145),
            accent: Color::Rgb(127, 180, 202),
            accent_hover: Color::Rgb(100, 155, 178),
            accent_idle: Color::Rgb(106, 153, 85),
            accent_blocked: Color::Rgb(196, 169, 125),
            accent_error: Color::Rgb(195, 95, 103),
            border: Color::Rgb(54, 54, 68),
            border_dim: Color::Rgb(102, 102, 122),
        }
    }
}

pub const ALL_THEMES: &[&str] = &[
    "orbt",
    "orange",
    "catppuccin",
    "gruvbox",
    "nord",
    "dracula",
    "solarized",
    "onedark",
    "everforest",
    "kanagawa",
];

thread_local! {
    static CURRENT: std::cell::RefCell<ThemeColors> =
        const { std::cell::RefCell::new(ThemeColors::orbt()) };
}

pub fn set_theme(name: &str) {
    let new = match name {
        "orange" => ThemeColors::orange(),
        "catppuccin" => ThemeColors::catppuccin(),
        "gruvbox" => ThemeColors::gruvbox(),
        "nord" => ThemeColors::nord(),
        "dracula" => ThemeColors::dracula(),
        "solarized" => ThemeColors::solarized(),
        "onedark" => ThemeColors::onedark(),
        "everforest" => ThemeColors::everforest(),
        "kanagawa" => ThemeColors::kanagawa(),
        _ => ThemeColors::orbt(),
    };
    CURRENT.with(|c| *c.borrow_mut() = new);
}

pub fn bg_primary() -> Color {
    CURRENT.with(|c| c.borrow().bg_primary)
}
pub fn bg_secondary() -> Color {
    CURRENT.with(|c| c.borrow().bg_secondary)
}
pub fn bg_tertiary() -> Color {
    CURRENT.with(|c| c.borrow().bg_tertiary)
}
pub fn bg_card() -> Color {
    CURRENT.with(|c| c.borrow().bg_card)
}
pub fn fg_primary() -> Color {
    CURRENT.with(|c| c.borrow().fg_primary)
}
pub fn fg_secondary() -> Color {
    CURRENT.with(|c| c.borrow().fg_secondary)
}
pub fn fg_muted() -> Color {
    CURRENT.with(|c| c.borrow().fg_muted)
}
pub fn accent() -> Color {
    CURRENT.with(|c| c.borrow().accent)
}
pub fn accent_hover() -> Color {
    CURRENT.with(|c| c.borrow().accent_hover)
}
pub fn accent_idle() -> Color {
    CURRENT.with(|c| c.borrow().accent_idle)
}
pub fn accent_blocked() -> Color {
    CURRENT.with(|c| c.borrow().accent_blocked)
}
pub fn accent_error() -> Color {
    CURRENT.with(|c| c.borrow().accent_error)
}
pub fn border() -> Color {
    CURRENT.with(|c| c.borrow().border)
}

pub fn border_dim() -> Color {
    CURRENT.with(|c| c.borrow().border_dim)
}
