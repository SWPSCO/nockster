use embedded_graphics::prelude::{Point, Size};
use heapless::{String as HString, Vec as HVec};

use super::constants::PIN_BUFFER_LEN;
use super::time::Instant;
use super::touch::{Coordinates, ScreenPoint};

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
    Diagnostics,
    TouchCalibration,
    Menu,
    Wallets,
    LabelEntry,
}

/// Items on the settings menu reachable from the unlocked header menu icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Wallets,
    AddSeed,
    Calibrate,
    Diagnostics,
    Back,
}

pub const TX_REVIEW_MAX_OUTPUTS: usize = 24;
pub const TX_REVIEW_FLAG_HIGH_FEE: u8 = 1 << 0;
pub const TX_REVIEW_FLAG_NO_REFUND: u8 = 1 << 1;
pub const TX_REVIEW_FLAG_MULTIPLE_RECIPIENTS: u8 = 1 << 2;

#[derive(Clone, Debug)]
pub struct TxReviewOutput {
    pub gift: u64,
    pub recipient_b58: HString<64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxReviewSummary {
    pub input_count: u32,
    pub external_output_count: u32,
    pub external_total: u64,
    pub refund_total: u64,
    pub fee_total: u64,
    pub flags: u8,
}

use super::label::{LabelButton, LabelInteraction};
use super::seed::SeedButton;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Digit(u8),
    Clear,
    Ok,
    Seed(SeedButton),
    Menu(MenuItem),
    WalletRow(u8),
    Label(LabelButton),
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
    TouchCalibrationComplete(nockster_core::TouchCalibration),
    Menu(MenuItem),
    Label(LabelInteraction),
    ExitDiagnostics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchDiagnostics {
    pub touching: bool,
    pub raw: Coordinates,
    pub screen: ScreenPoint,
    pub count: u8,
    pub status: u8,
    pub pressure: u8,
    pub samples: u32,
    pub frames: u32,
    pub errors: u32,
}

impl TouchDiagnostics {
    pub fn new() -> Self {
        Self {
            touching: false,
            raw: Coordinates { x: 0, y: 0 },
            screen: ScreenPoint { x: 0, y: 0 },
            count: 0,
            status: 0,
            pressure: 0,
            samples: 0,
            frames: 0,
            errors: 0,
        }
    }

    pub fn record_sample(
        &mut self,
        raw: Coordinates,
        screen: ScreenPoint,
        count: u8,
        status: u8,
        pressure: u8,
    ) {
        self.touching = true;
        self.raw = raw;
        self.screen = screen;
        self.count = count;
        self.status = status;
        self.pressure = pressure;
        self.samples = self.samples.saturating_add(1);
    }

    pub fn record_no_touch(&mut self) {
        self.touching = false;
        self.count = 0;
        self.status = 0;
        self.pressure = 0;
    }

    pub fn record_error(&mut self) {
        self.touching = false;
        self.count = 0;
        self.errors = self.errors.saturating_add(1);
    }
}

impl Default for TouchDiagnostics {
    fn default() -> Self {
        Self::new()
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
