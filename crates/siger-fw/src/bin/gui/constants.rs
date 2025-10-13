use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
use esp_hal::time::Duration;

pub const SCREEN_WIDTH: u16 = 172;
pub const SCREEN_HEIGHT: u16 = 320;
pub const DISPLAY_X_OFFSET: u16 = 34;
pub const BOOT_LOGO_WIDTH: u16 = SCREEN_WIDTH;
pub const BOOT_LOGO_HEIGHT: u16 = SCREEN_HEIGHT;
pub const RAW_X_MIN: u16 = 0;
pub const RAW_X_MAX: u16 = 0x0075;
pub const RAW_Y_MIN: u16 = 0;
pub const RAW_Y_MAX: u16 = 0x0120;
pub const MIRROR_X: bool = true;

pub const COLOR_BACKGROUND: Rgb565 = Rgb565::new(3, 6, 10);
pub const COLOR_BUTTON: Rgb565 = Rgb565::new(12, 24, 18);
pub const COLOR_BUTTON_BORDER: Rgb565 = Rgb565::new(6, 12, 10);
pub const COLOR_BUTTON_ACTIVE: Rgb565 = Rgb565::new(18, 40, 28);
pub const COLOR_TEXT: Rgb565 = Rgb565::WHITE;
pub const COLOR_UNLOCK_BG: Rgb565 = Rgb565::new(2, 28, 18);
pub const COLOR_CONFIRM_APPROVE: Rgb565 = Rgb565::new(5, 50, 15);
pub const COLOR_CONFIRM_APPROVE_ACTIVE: Rgb565 = Rgb565::new(4, 60, 18);
pub const COLOR_CONFIRM_REJECT: Rgb565 = Rgb565::new(24, 8, 8);
pub const COLOR_CONFIRM_REJECT_ACTIVE: Rgb565 = Rgb565::new(29, 8, 9);

pub const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];
pub const BUTTON_STABLE_DURATION: Duration = Duration::from_millis(30);
pub const BUTTON_INACTIVE_GRACE: Duration = Duration::from_millis(80);
pub const MIN_PRESS_DURATION: Duration = Duration::from_millis(40);
pub const RELEASE_DEBOUNCE: Duration = Duration::from_millis(40);
pub const PRESS_COOLDOWN: Duration = Duration::from_millis(120);
pub const PIN_BUFFER_LEN: usize = 16;
pub const MAX_PIN_DIGITS: usize = 12;