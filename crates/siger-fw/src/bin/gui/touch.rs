use siger_fw::axs5106l::Coordinates;

use super::constants::{
    MIRROR_X, RAW_X_MAX, RAW_X_MIN, RAW_Y_MAX, RAW_Y_MIN, SCREEN_HEIGHT, SCREEN_WIDTH,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: u16,
    pub y: u16,
}

pub fn transform_raw_touch(raw: Coordinates) -> ScreenPoint {
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

    let mut x = scale_axis(raw.x, RAW_X_MIN, RAW_X_MAX, span_x);
    let y = scale_axis(raw.y, RAW_Y_MIN, RAW_Y_MAX, span_y);

    if MIRROR_X {
        x = span_x.saturating_sub(x);
    }

    ScreenPoint { x, y }
}
