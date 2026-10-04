use super::time::Duration;

pub const SCREEN_WIDTH: u16 = 172;
pub const SCREEN_HEIGHT: u16 = 320;
pub const DISPLAY_X_OFFSET: u16 = 34;
pub const RAW_X_MIN: u16 = 0;
pub const RAW_X_MAX: u16 = 151;
pub const RAW_Y_MIN: u16 = 0;
pub const RAW_Y_MAX: u16 = 331;
pub const MIRROR_X: bool = true;

pub const IDLE_OVERLAY_MARGIN: i32 = 8;
pub const IDLE_OVERLAY_HEIGHT: i32 = 44;

pub const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];
pub const BUTTON_STABLE_DURATION: Duration = Duration::from_millis(30);
pub const BUTTON_INACTIVE_GRACE: Duration = Duration::from_millis(80);
pub const MIN_PRESS_DURATION: Duration = Duration::from_millis(40);
pub const RELEASE_DEBOUNCE: Duration = Duration::from_millis(40);
pub const PRESS_COOLDOWN: Duration = Duration::from_millis(120);
/// How often to ask the touch controller "is the finger still there?" while
/// the INT line is silent mid-touch (stationary fingers stop reporting).
pub const PRESENCE_POLL_INTERVAL: Duration = Duration::from_millis(15);
/// Presence polling gives up if the controller produced no INT report for
/// this long; a real touch keeps reporting at least occasionally.
pub const TOUCH_REPORT_SILENCE_LIMIT: Duration = Duration::from_millis(1500);
pub const PIN_BUFFER_LEN: usize = 16;
pub const MAX_PIN_DIGITS: usize = 12;

pub const AUTO_LOCK_TIMEOUT: Duration = Duration::from_secs(120);

/// Hold-to-confirm: how long Approve must be held before a confirmation
/// fires (the progress ring sweeps 360 degrees over this window).
pub const HOLD_CONFIRM_DURATION: Duration = Duration::from_millis(3_000);
/// After a hold completes the finger is still on the glass and the host may
/// swap screens under it; block arming for longer than a tap's cooldown.
pub const HOLD_CONFIRM_RELEASE_COOLDOWN: Duration = Duration::from_millis(600);
