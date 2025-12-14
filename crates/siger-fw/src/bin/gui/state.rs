use embedded_graphics::prelude::{Point, Size};
use esp_hal::time::Instant;
use heapless::{String as HString, Vec as HVec};

use super::constants::PIN_BUFFER_LEN;
use super::touch::ScreenPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiMode {
    Splash,
    Locked,
    Confirm,
    TxReview,
    Unlocking,
    Unlocked,
    Error,
    SeedFirstBoot,
    SeedEntry,
    SeedConfirm,
}

pub const TX_REVIEW_MAX_OUTPUTS: usize = 24;

#[derive(Clone, Debug)]
pub struct TxReviewOutput {
    pub gift: u64,
    pub recipient_b58: HString<64>,
}

use super::seed::SeedButton;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Digit(u8),
    Clear,
    Ok,
    Seed(SeedButton),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonHit {
    pub button: Button,
    pub top_left: Point,
    pub size: Size,
}

use super::seed::SeedInteraction;

#[derive(Clone, Debug)]
pub enum GuiInteraction {
    PinComplete(HVec<u8, PIN_BUFFER_LEN>),
    ConfirmAccepted,
    ConfirmRejected,
    RawTouch(ScreenPoint),
    LockRequested,
    Seed(SeedInteraction),
}

pub struct TextBuffers {
    pub status: HString<64>,
    pub info: HString<64>,
}

impl TextBuffers {
    pub fn new() -> Self {
        Self {
            status: HString::new(),
            info: HString::new(),
        }
    }
}

pub struct InteractionState {
    pub active_button: Option<ButtonHit>,
    pub finger_down: bool,
    pub pending_hit: Option<ButtonHit>,
    pub pending_since: Option<Instant>,
    pub active_seen_at: Option<Instant>,
    pub press_started_at: Option<Instant>,
    pub last_touch_sample_at: Option<Instant>,
    pub cooldown_until: Option<Instant>,
}

impl InteractionState {
    pub fn new() -> Self {
        Self {
            active_button: None,
            finger_down: false,
            pending_hit: None,
            pending_since: None,
            active_seen_at: None,
            press_started_at: None,
            last_touch_sample_at: None,
            cooldown_until: None,
        }
    }
}

impl Default for InteractionState {
    fn default() -> Self {
        Self::new()
    }
}
