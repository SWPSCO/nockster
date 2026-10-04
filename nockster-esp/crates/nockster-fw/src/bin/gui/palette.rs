use core::sync::atomic::{AtomicU8, Ordering};

use embedded_graphics::pixelcolor::Rgb565;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Theme {
    Nockster = 0,
    Solarized = 1,
    Nord = 2,
    Dracula = 3,
    Mono = 4,
    Paper = 5,
}

pub const THEMES: [Theme; 6] = [
    Theme::Nockster,
    Theme::Solarized,
    Theme::Nord,
    Theme::Dracula,
    Theme::Mono,
    Theme::Paper,
];

#[derive(Clone, Copy)]
pub struct GuiPalette {
    pub background: Rgb565,
    pub surface_low: Rgb565,
    pub surface_high: Rgb565,
    pub divider: Rgb565,
    pub text: Rgb565,
    pub text_subtle: Rgb565,
    pub accent_primary: Rgb565,
    pub accent_primary_light: Rgb565,
    pub accent_primary_dark: Rgb565,
    pub accent_secondary: Rgb565,
    pub accent_secondary_light: Rgb565,
    pub accent_secondary_dark: Rgb565,
    pub accent_info: Rgb565,
    pub accent_warning: Rgb565,
    pub keypad_border: Rgb565,
    pub panel_highlight: Rgb565,
    pub panel_shadow: Rgb565,
}

static CURRENT_THEME: AtomicU8 = AtomicU8::new(Theme::Nockster as u8);

impl Theme {
    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(Self::Nockster),
            1 => Some(Self::Solarized),
            2 => Some(Self::Nord),
            3 => Some(Self::Dracula),
            4 => Some(Self::Mono),
            5 => Some(Self::Paper),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Nockster => "Nockster",
            Self::Solarized => "Solarized",
            Self::Nord => "Nord",
            Self::Dracula => "Dracula",
            Self::Mono => "Mono",
            Self::Paper => "Paper",
        }
    }

    pub fn palette(self) -> GuiPalette {
        match self {
            Self::Nockster => GuiPalette {
                background: Rgb565::new(1, 3, 5),
                surface_low: Rgb565::new(4, 7, 9),
                surface_high: Rgb565::new(6, 10, 13),
                divider: Rgb565::new(12, 20, 20),
                text: Rgb565::new(30, 61, 29),
                text_subtle: Rgb565::new(24, 48, 24),
                accent_primary: Rgb565::new(0, 41, 28),
                accent_primary_light: Rgb565::new(7, 55, 31),
                accent_primary_dark: Rgb565::new(0, 24, 17),
                accent_secondary: Rgb565::new(24, 18, 10),
                accent_secondary_light: Rgb565::new(30, 25, 14),
                accent_secondary_dark: Rgb565::new(12, 8, 5),
                accent_info: Rgb565::new(7, 55, 31),
                accent_warning: Rgb565::new(31, 44, 7),
                keypad_border: Rgb565::new(14, 25, 25),
                panel_highlight: Rgb565::new(10, 16, 18),
                panel_shadow: Rgb565::new(0, 2, 3),
            },
            Self::Solarized => GuiPalette {
                background: Rgb565::new(0, 10, 6),
                surface_low: Rgb565::new(0, 13, 8),
                surface_high: Rgb565::new(11, 27, 14),
                divider: Rgb565::new(18, 40, 19),
                text: Rgb565::new(29, 58, 26),
                text_subtle: Rgb565::new(18, 40, 19),
                accent_primary: Rgb565::new(5, 40, 19),
                accent_primary_light: Rgb565::new(13, 51, 24),
                accent_primary_dark: Rgb565::new(0, 26, 14),
                accent_secondary: Rgb565::new(25, 18, 2),
                accent_secondary_light: Rgb565::new(31, 34, 8),
                accent_secondary_dark: Rgb565::new(15, 9, 1),
                accent_info: Rgb565::new(4, 34, 26),
                accent_warning: Rgb565::new(22, 34, 0),
                keypad_border: Rgb565::new(7, 24, 14),
                panel_highlight: Rgb565::new(15, 37, 20),
                panel_shadow: Rgb565::new(0, 7, 5),
            },
            Self::Nord => GuiPalette {
                background: Rgb565::new(5, 13, 8),
                surface_low: Rgb565::new(7, 16, 10),
                surface_high: Rgb565::new(8, 19, 11),
                divider: Rgb565::new(9, 21, 13),
                text: Rgb565::new(29, 59, 30),
                text_subtle: Rgb565::new(27, 55, 29),
                accent_primary: Rgb565::new(17, 48, 26),
                accent_primary_light: Rgb565::new(20, 54, 30),
                accent_primary_dark: Rgb565::new(9, 28, 16),
                accent_secondary: Rgb565::new(22, 35, 21),
                accent_secondary_light: Rgb565::new(28, 43, 26),
                accent_secondary_dark: Rgb565::new(12, 19, 12),
                accent_info: Rgb565::new(15, 42, 28),
                accent_warning: Rgb565::new(29, 50, 17),
                keypad_border: Rgb565::new(12, 25, 16),
                panel_highlight: Rgb565::new(13, 26, 17),
                panel_shadow: Rgb565::new(3, 8, 5),
            },
            Self::Dracula => GuiPalette {
                background: Rgb565::new(5, 10, 6),
                surface_low: Rgb565::new(6, 13, 8),
                surface_high: Rgb565::new(8, 17, 11),
                divider: Rgb565::new(12, 28, 20),
                text: Rgb565::new(31, 62, 30),
                text_subtle: Rgb565::new(24, 49, 26),
                accent_primary: Rgb565::new(23, 36, 31),
                accent_primary_light: Rgb565::new(28, 44, 31),
                accent_primary_dark: Rgb565::new(13, 20, 19),
                accent_secondary: Rgb565::new(31, 30, 24),
                accent_secondary_light: Rgb565::new(31, 42, 28),
                accent_secondary_dark: Rgb565::new(18, 15, 14),
                accent_info: Rgb565::new(10, 54, 31),
                accent_warning: Rgb565::new(30, 62, 17),
                keypad_border: Rgb565::new(16, 30, 22),
                panel_highlight: Rgb565::new(13, 24, 16),
                panel_shadow: Rgb565::new(3, 6, 4),
            },
            Self::Mono => GuiPalette {
                background: Rgb565::new(0, 0, 0),
                surface_low: Rgb565::new(2, 4, 2),
                surface_high: Rgb565::new(4, 8, 4),
                divider: Rgb565::new(11, 23, 11),
                text: Rgb565::new(31, 63, 31),
                text_subtle: Rgb565::new(22, 44, 22),
                accent_primary: Rgb565::new(22, 44, 22),
                accent_primary_light: Rgb565::new(31, 63, 31),
                accent_primary_dark: Rgb565::new(11, 23, 11),
                accent_secondary: Rgb565::new(16, 32, 16),
                accent_secondary_light: Rgb565::new(25, 50, 25),
                accent_secondary_dark: Rgb565::new(8, 16, 8),
                accent_info: Rgb565::new(31, 63, 31),
                accent_warning: Rgb565::new(28, 56, 28),
                keypad_border: Rgb565::new(14, 28, 14),
                panel_highlight: Rgb565::new(20, 40, 20),
                panel_shadow: Rgb565::new(0, 0, 0),
            },
            Self::Paper => GuiPalette {
                background: Rgb565::new(30, 61, 30),
                surface_low: Rgb565::new(31, 63, 31),
                surface_high: Rgb565::new(31, 63, 31),
                divider: Rgb565::new(26, 53, 26),
                text: Rgb565::new(1, 3, 1),
                text_subtle: Rgb565::new(13, 26, 13),
                accent_primary: Rgb565::new(26, 53, 26),
                accent_primary_light: Rgb565::new(31, 63, 31),
                accent_primary_dark: Rgb565::new(13, 26, 13),
                accent_secondary: Rgb565::new(20, 40, 20),
                accent_secondary_light: Rgb565::new(28, 56, 28),
                accent_secondary_dark: Rgb565::new(8, 16, 8),
                accent_info: Rgb565::new(1, 3, 1),
                accent_warning: Rgb565::new(6, 12, 6),
                keypad_border: Rgb565::new(1, 3, 1),
                panel_highlight: Rgb565::new(31, 63, 31),
                panel_shadow: Rgb565::new(26, 53, 26),
            },
        }
    }
}

pub fn current_theme() -> Theme {
    Theme::from_id(CURRENT_THEME.load(Ordering::Relaxed)).unwrap_or(Theme::Nockster)
}

pub fn set_theme(theme: Theme) {
    CURRENT_THEME.store(theme.id(), Ordering::Relaxed);
}

pub fn colors() -> GuiPalette {
    current_theme().palette()
}

pub fn background() -> Rgb565 {
    colors().background
}

pub fn surface_low() -> Rgb565 {
    colors().surface_low
}

pub fn surface_high() -> Rgb565 {
    colors().surface_high
}

pub fn divider() -> Rgb565 {
    colors().divider
}

pub fn text() -> Rgb565 {
    colors().text
}

pub fn text_subtle() -> Rgb565 {
    colors().text_subtle
}

#[allow(dead_code)]
pub fn accent_primary() -> Rgb565 {
    colors().accent_primary
}

pub fn accent_primary_light() -> Rgb565 {
    colors().accent_primary_light
}

#[allow(dead_code)]
pub fn accent_primary_dark() -> Rgb565 {
    colors().accent_primary_dark
}

#[allow(dead_code)]
pub fn accent_secondary() -> Rgb565 {
    colors().accent_secondary
}

#[allow(dead_code)]
pub fn accent_secondary_light() -> Rgb565 {
    colors().accent_secondary_light
}

#[allow(dead_code)]
pub fn accent_secondary_dark() -> Rgb565 {
    colors().accent_secondary_dark
}

pub fn accent_info() -> Rgb565 {
    colors().accent_info
}

pub fn accent_warning() -> Rgb565 {
    colors().accent_warning
}

pub fn keypad_idle() -> Rgb565 {
    colors().surface_low
}

pub fn keypad_active() -> Rgb565 {
    colors().accent_primary
}

pub fn keypad_active_light() -> Rgb565 {
    colors().accent_primary_light
}

pub fn keypad_active_dark() -> Rgb565 {
    colors().accent_primary_dark
}

pub fn keypad_border() -> Rgb565 {
    colors().keypad_border
}

pub fn panel_base() -> Rgb565 {
    colors().surface_high
}

pub fn panel_highlight() -> Rgb565 {
    colors().panel_highlight
}

pub fn panel_shadow() -> Rgb565 {
    colors().panel_shadow
}

pub fn panel_border() -> Rgb565 {
    colors().divider
}

pub fn btn_primary_base() -> Rgb565 {
    colors().accent_primary
}

pub fn btn_primary_light() -> Rgb565 {
    colors().accent_primary_light
}

pub fn btn_primary_dark() -> Rgb565 {
    colors().accent_primary_dark
}

pub fn btn_secondary_base() -> Rgb565 {
    colors().accent_secondary
}

pub fn btn_secondary_light() -> Rgb565 {
    colors().accent_secondary_light
}

pub fn btn_secondary_dark() -> Rgb565 {
    colors().accent_secondary_dark
}

pub fn btn_disabled_base() -> Rgb565 {
    colors().surface_low
}

pub fn btn_disabled_light() -> Rgb565 {
    colors().surface_high
}

pub fn btn_disabled_dark() -> Rgb565 {
    colors().background
}
