use nockster_core::TouchCalibration;

#[cfg(not(target_arch = "wasm32"))]
pub use nockster_fw::axs5106l::Coordinates;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coordinates {
    pub x: u16,
    pub y: u16,
}

use super::constants::{
    MIRROR_X, RAW_X_MAX, RAW_X_MIN, RAW_Y_MAX, RAW_Y_MIN, SCREEN_HEIGHT, SCREEN_WIDTH,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchSample {
    pub raw: Coordinates,
    pub screen: ScreenPoint,
    pub count: u8,
    pub status: u8,
    pub pressure: u8,
}

pub fn default_touch_calibration() -> TouchCalibration {
    TouchCalibration {
        raw_x_min: RAW_X_MIN,
        raw_x_max: RAW_X_MAX,
        raw_y_min: RAW_Y_MIN,
        raw_y_max: RAW_Y_MAX,
        mirror_x: MIRROR_X,
        mirror_y: false,
    }
}

pub fn touch_calibration_valid(calibration: &TouchCalibration) -> bool {
    calibration.raw_x_min < calibration.raw_x_max && calibration.raw_y_min < calibration.raw_y_max
}

pub fn transform_raw_touch(raw: Coordinates, calibration: TouchCalibration) -> ScreenPoint {
    fn scale_axis(input: u16, min: u16, max: u16, span: u16) -> u16 {
        let clamped = input.clamp(min, max);
        let range = u32::from(max.saturating_sub(min)).max(1);
        let shifted = u32::from(clamped.saturating_sub(min));
        let scaled = shifted
            .saturating_mul(u32::from(span))
            .saturating_div(range)
            .min(u32::from(span));
        scaled.try_into().unwrap_or(span)
    }

    let span_x = SCREEN_WIDTH.saturating_sub(1);
    let span_y = SCREEN_HEIGHT.saturating_sub(1);

    let mut x = scale_axis(raw.x, calibration.raw_x_min, calibration.raw_x_max, span_x);
    let mut y = scale_axis(raw.y, calibration.raw_y_min, calibration.raw_y_max, span_y);

    if calibration.mirror_x {
        x = span_x.saturating_sub(x);
    }
    if calibration.mirror_y {
        y = span_y.saturating_sub(y);
    }

    ScreenPoint { x, y }
}
