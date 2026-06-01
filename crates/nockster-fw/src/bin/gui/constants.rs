use embedded_graphics::pixelcolor::Rgb565;
use esp_hal::time::Duration;

pub const SCREEN_WIDTH: u16 = 172;
pub const SCREEN_HEIGHT: u16 = 320;
pub const DISPLAY_X_OFFSET: u16 = 34;
pub const RAW_X_MIN: u16 = 0;
pub const RAW_X_MAX: u16 = 0x0075;
pub const RAW_Y_MIN: u16 = 0;
pub const RAW_Y_MAX: u16 = 0x0120;
pub const MIRROR_X: bool = true;

pub const COLOR_BACKGROUND: Rgb565 = Rgb565::new(2, 4, 8);
pub const COLOR_SURFACE_LOW: Rgb565 = Rgb565::new(5, 7, 12);
pub const COLOR_SURFACE_HIGH: Rgb565 = Rgb565::new(8, 11, 18);
pub const COLOR_DIVIDER: Rgb565 = Rgb565::new(3, 5, 8);

pub const COLOR_TEXT: Rgb565 = Rgb565::new(29, 58, 31);
pub const COLOR_TEXT_SUBTLE: Rgb565 = Rgb565::new(18, 32, 21);

pub const COLOR_ACCENT_PRIMARY: Rgb565 = Rgb565::new(18, 44, 24);
pub const COLOR_ACCENT_PRIMARY_LIGHT: Rgb565 = Rgb565::new(24, 54, 28);
pub const COLOR_ACCENT_PRIMARY_DARK: Rgb565 = Rgb565::new(10, 24, 15);

pub const COLOR_ACCENT_SECONDARY: Rgb565 = Rgb565::new(26, 20, 30);
pub const COLOR_ACCENT_SECONDARY_LIGHT: Rgb565 = Rgb565::new(29, 26, 31);
pub const COLOR_ACCENT_SECONDARY_DARK: Rgb565 = Rgb565::new(14, 11, 18);

pub const COLOR_ACCENT_INFO: Rgb565 = Rgb565::new(31, 8, 6);
pub const COLOR_ACCENT_WARNING: Rgb565 = Rgb565::new(8, 42, 31);

pub const COLOR_KEYPAD_IDLE: Rgb565 = COLOR_SURFACE_LOW;
pub const COLOR_KEYPAD_ACTIVE: Rgb565 = Rgb565::new(22, 52, 30);
pub const COLOR_KEYPAD_ACTIVE_LIGHT: Rgb565 = Rgb565::new(26, 60, 36);
pub const COLOR_KEYPAD_ACTIVE_DARK: Rgb565 = Rgb565::new(10, 26, 16);
pub const COLOR_KEYPAD_BORDER: Rgb565 = COLOR_DIVIDER;

pub const COLOR_PANEL_BASE: Rgb565 = COLOR_SURFACE_HIGH;
pub const COLOR_PANEL_HIGHLIGHT: Rgb565 = Rgb565::new(12, 18, 24);
pub const COLOR_PANEL_SHADOW: Rgb565 = Rgb565::new(3, 4, 6);
pub const COLOR_PANEL_BORDER: Rgb565 = COLOR_DIVIDER;

pub const COLOR_BTN_PRIMARY_BASE: Rgb565 = COLOR_ACCENT_PRIMARY;
pub const COLOR_BTN_PRIMARY_LIGHT: Rgb565 = COLOR_ACCENT_PRIMARY_LIGHT;
pub const COLOR_BTN_PRIMARY_DARK: Rgb565 = COLOR_ACCENT_PRIMARY_DARK;

pub const COLOR_BTN_SECONDARY_BASE: Rgb565 = COLOR_ACCENT_SECONDARY;
pub const COLOR_BTN_SECONDARY_LIGHT: Rgb565 = COLOR_ACCENT_SECONDARY_LIGHT;
pub const COLOR_BTN_SECONDARY_DARK: Rgb565 = COLOR_ACCENT_SECONDARY_DARK;

pub const COLOR_BTN_DISABLED_BASE: Rgb565 = Rgb565::new(4, 5, 7);
pub const COLOR_BTN_DISABLED_LIGHT: Rgb565 = Rgb565::new(7, 8, 11);
pub const COLOR_BTN_DISABLED_DARK: Rgb565 = Rgb565::new(2, 3, 4);

pub const IDLE_OVERLAY_MARGIN: i32 = 8;
pub const IDLE_OVERLAY_HEIGHT: i32 = 44;

pub const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];
pub const BUTTON_STABLE_DURATION: Duration = Duration::from_millis(30);
pub const BUTTON_INACTIVE_GRACE: Duration = Duration::from_millis(80);
pub const MIN_PRESS_DURATION: Duration = Duration::from_millis(40);
pub const RELEASE_DEBOUNCE: Duration = Duration::from_millis(40);
pub const PRESS_COOLDOWN: Duration = Duration::from_millis(120);
pub const PIN_BUFFER_LEN: usize = 16;
pub const MAX_PIN_DIGITS: usize = 12;

pub const AUTO_LOCK_TIMEOUT: Duration = Duration::from_secs(120);
