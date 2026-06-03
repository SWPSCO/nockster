use core::fmt::Write as _;

use embedded_graphics::mono_font::{
    ascii::{FONT_10X20, FONT_6X10},
    MonoTextStyle,
};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use nockster_core::TouchCalibration;

use super::constants::*;
use super::layout::{
    confirm_buttons, header_height, keypad_button_hit, keypad_grid, row_height, tx_review_buttons,
    tx_review_detail_rect, tx_review_list_rect, tx_review_output_item_height,
    tx_review_summary_height, TX_REVIEW_LINE_GAP, TX_REVIEW_PADDING,
};
use super::state::{
    Button, ButtonHit, GuiMode, TouchDiagnostics, TxReviewOutput, TxReviewSummary,
    TX_REVIEW_FLAG_HIGH_FEE, TX_REVIEW_FLAG_MULTIPLE_RECIPIENTS, TX_REVIEW_FLAG_NO_REFUND,
};
use super::GuiDisplay;

include!(concat!(env!("OUT_DIR"), "/boot_logo.rs"));

pub fn blit_boot_logo(display: &mut GuiDisplay<'_>) {
    let expected_len = (BOOT_LOGO_WIDTH as usize) * (BOOT_LOGO_HEIGHT as usize) * 2;
    debug_assert_eq!(BOOT_LOGO.len(), expected_len);
    let logo_iter = BOOT_LOGO.chunks_exact(2).map(|chunk| {
        let raw = u16::from_be_bytes([chunk[0], chunk[1]]);
        Rgb565::from(RawU16::new(raw))
    });
    let _ = display.set_pixels(0, 0, BOOT_LOGO_WIDTH - 1, BOOT_LOGO_HEIGHT - 1, logo_iter);
}

pub fn draw_keypad(display: &mut GuiDisplay<'_>) {
    let _ = display.clear(COLOR_BACKGROUND);

    let frame = Rectangle::new(
        Point::new(4, header_height()),
        Size::new(
            (BOOT_LOGO_WIDTH - 8) as u32,
            (BOOT_LOGO_HEIGHT as i32 - header_height() - 8).max(0) as u32,
        ),
    );
    let _ = frame
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_SURFACE_LOW)
                .stroke_color(COLOR_DIVIDER)
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    for (row_idx, row) in keypad_grid().iter().enumerate() {
        for col_idx in 0..row.len() {
            let hit = keypad_button_hit(row_idx, col_idx);
            draw_button(display, GuiMode::Locked, hit, false);
        }
    }
}

pub fn draw_button(display: &mut GuiDisplay<'_>, mode: GuiMode, hit: ButtonHit, active: bool) {
    let Palette {
        base,
        light,
        dark,
        border,
    } = button_palette(mode, hit.button, active);

    let rect = Rectangle::new(hit.top_left, hit.size);
    let _ = rect
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(base)
                .stroke_color(border)
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    if hit.size.height > 6 && hit.size.width > 6 {
        // Top highlight
        let highlight = Rectangle::new(
            Point::new(hit.top_left.x + 1, hit.top_left.y + 1),
            Size::new(hit.size.width.saturating_sub(2), hit.size.height / 3),
        );
        let _ = highlight
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(light)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);

        // Bottom shadow
        let shadow_height = hit.size.height / 3;
        let shadow_top = hit.top_left.y + hit.size.height as i32 - shadow_height as i32 - 1;
        let shadow = Rectangle::new(
            Point::new(hit.top_left.x + 1, shadow_top),
            Size::new(hit.size.width.saturating_sub(2), shadow_height),
        );
        let _ = shadow
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(dark)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
    }

    let label = match mode {
        GuiMode::Confirm => confirm_button_label(hit.button),
        _ => button_label(hit.button),
    };

    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let center = Point::new(
        hit.top_left.x + hit.size.width as i32 / 2,
        hit.top_left.y + hit.size.height as i32 / 2,
    );
    let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
    let _ = Text::with_alignment(
        label,
        Point::new(center.x, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

pub fn render_header(display: &mut GuiDisplay<'_>, text: &str, bg: Rgb565) {
    let header_h = header_height();
    let header_rect = Rectangle::new(
        Point::new(0, 0),
        Size::new(BOOT_LOGO_WIDTH.into(), header_h as u32),
    );
    let _ = header_rect
        .into_styled(PrimitiveStyleBuilder::new().fill_color(bg).build())
        .draw(display);
    let underline = Rectangle::new(
        Point::new(0, header_h - 2),
        Size::new(BOOT_LOGO_WIDTH.into(), 2),
    );
    let _ = underline
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_DIVIDER)
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let baseline = header_h / 2 + FONT_10X20.character_size.height as i32 / 3;
    let _ = Text::with_alignment(
        text,
        Point::new((BOOT_LOGO_WIDTH / 2) as i32, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

/// Centre of the settings gear, in the top-right of the header.
pub fn settings_gear_center() -> Point {
    Point::new(SCREEN_WIDTH as i32 - 28, header_height() / 2)
}

/// Draws a small cog icon used as the settings entry point.
/// `header_bg` must match the colour the header was filled with so the hub
/// "hole" blends in.
pub fn draw_settings_gear(display: &mut GuiDisplay<'_>, active: bool, header_bg: Rgb565) {
    let color = if active {
        COLOR_KEYPAD_ACTIVE_LIGHT
    } else {
        COLOR_TEXT
    };
    let center = settings_gear_center();

    let teeth = [
        (0, -16),
        (0, 16),
        (-16, 0),
        (16, 0),
        (-11, -11),
        (11, -11),
        (-11, 11),
        (11, 11),
    ];
    for (dx, dy) in teeth {
        let tooth =
            Rectangle::with_center(Point::new(center.x + dx, center.y + dy), Size::new(6, 6));
        let _ = tooth
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(display);
    }
    let _ = Circle::with_center(center, 28)
        .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
        .draw(display);
    let _ = Circle::with_center(center, 12)
        .into_styled(PrimitiveStyleBuilder::new().fill_color(header_bg).build())
        .draw(display);
}

fn draw_header_lock_icon(display: &mut GuiDisplay<'_>, active: bool, header_bg: Rgb565) {
    let color = if active {
        COLOR_KEYPAD_ACTIVE_LIGHT
    } else {
        COLOR_TEXT
    };
    let center = Point::new(28, header_height() / 2);
    let stroke = PrimitiveStyleBuilder::new()
        .stroke_color(color)
        .stroke_width(4)
        .build();
    let fill = PrimitiveStyleBuilder::new()
        .fill_color(color)
        .stroke_width(0)
        .build();
    let cutout = PrimitiveStyleBuilder::new()
        .fill_color(header_bg)
        .stroke_width(0)
        .build();

    let shackle_top = center.y - 20;
    let shackle_bottom = center.y - 4;
    let _ = Line::new(
        Point::new(center.x - 10, shackle_top),
        Point::new(center.x - 10, shackle_bottom),
    )
    .into_styled(stroke)
    .draw(display);
    let _ = Line::new(
        Point::new(center.x + 10, shackle_top),
        Point::new(center.x + 10, shackle_bottom),
    )
    .into_styled(stroke)
    .draw(display);
    let _ = Line::new(
        Point::new(center.x - 10, shackle_top),
        Point::new(center.x + 10, shackle_top),
    )
    .into_styled(stroke)
    .draw(display);

    let body = Rectangle::new(Point::new(center.x - 15, center.y - 2), Size::new(30, 22));
    let _ = body.into_styled(fill).draw(display);
    let _ = Circle::with_center(Point::new(center.x, center.y + 7), 7)
        .into_styled(cutout)
        .draw(display);
    let key_slot = Rectangle::new(Point::new(center.x - 2, center.y + 7), Size::new(5, 10));
    let _ = key_slot.into_styled(cutout).draw(display);
}

pub fn draw_centered_message(display: &mut GuiDisplay<'_>, text: &str) {
    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let baseline = (BOOT_LOGO_HEIGHT / 2) as i32;
    let _ = Text::with_alignment(
        text,
        Point::new((BOOT_LOGO_WIDTH / 2) as i32, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

pub fn render_touch_diagnostics(
    display: &mut GuiDisplay<'_>,
    diagnostics: &TouchDiagnostics,
    calibration: TouchCalibration,
    build: &str,
) {
    let top = header_height();
    let body = Rectangle::new(
        Point::new(0, top),
        Size::new(
            SCREEN_WIDTH.into(),
            (SCREEN_HEIGHT as i32 - top).max(0) as u32,
        ),
    );
    let _ = body
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_BACKGROUND)
                .build(),
        )
        .draw(display);

    let style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
    let subtle = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
    let left = 6;
    let mut y = top + 14;

    let mut line = heapless::String::<64>::new();
    let _ = write!(
        line,
        "touch: {} count {}",
        if diagnostics.touching { "down" } else { "none" },
        diagnostics.count
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), style).draw(display);
    y += 14;

    line.clear();
    let _ = write!(line, "raw: {},{}", diagnostics.raw.x, diagnostics.raw.y);
    let _ = Text::new(line.as_str(), Point::new(left, y), style).draw(display);
    y += 14;

    line.clear();
    let _ = write!(
        line,
        "screen: {},{}",
        diagnostics.screen.x, diagnostics.screen.y
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), style).draw(display);
    y += 14;

    line.clear();
    let _ = write!(
        line,
        "status: 0x{:02x} pressure {}",
        diagnostics.status, diagnostics.pressure
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), style).draw(display);
    y += 18;

    line.clear();
    let _ = write!(
        line,
        "cal x: {}..{}",
        calibration.raw_x_min, calibration.raw_x_max
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), subtle).draw(display);
    y += 12;

    line.clear();
    let _ = write!(
        line,
        "cal y: {}..{}",
        calibration.raw_y_min, calibration.raw_y_max
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), subtle).draw(display);
    y += 12;

    line.clear();
    let _ = write!(
        line,
        "mirror x:{} y:{}",
        calibration.mirror_x, calibration.mirror_y
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), subtle).draw(display);
    y += 18;

    line.clear();
    let _ = write!(
        line,
        "samples {} frames {}",
        diagnostics.samples, diagnostics.frames
    );
    let _ = Text::new(line.as_str(), Point::new(left, y), subtle).draw(display);
    y += 12;

    line.clear();
    let _ = write!(line, "i2c errors {}", diagnostics.errors);
    let _ = Text::new(line.as_str(), Point::new(left, y), subtle).draw(display);
    y += 18;

    let _ = Text::new(build, Point::new(left, y), subtle).draw(display);
}

pub fn render_touch_calibration_target(
    display: &mut GuiDisplay<'_>,
    step: usize,
    total: usize,
    target: super::ScreenPoint,
) {
    let top = header_height();
    let body = Rectangle::new(
        Point::new(0, top),
        Size::new(
            SCREEN_WIDTH.into(),
            (SCREEN_HEIGHT as i32 - top).max(0) as u32,
        ),
    );
    let _ = body
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_BACKGROUND)
                .build(),
        )
        .draw(display);

    let style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
    let mut line = heapless::String::<48>::new();
    let _ = write!(line, "target {}/{}", step.saturating_add(1), total);
    let _ = Text::with_alignment(
        line.as_str(),
        Point::new((SCREEN_WIDTH / 2) as i32, top + 18),
        style,
        Alignment::Center,
    )
    .draw(display);

    let center = Point::new(target.x as i32, target.y as i32);
    let cross = PrimitiveStyleBuilder::new()
        .stroke_color(COLOR_ACCENT_PRIMARY_LIGHT)
        .stroke_width(2)
        .build();
    let ring = PrimitiveStyleBuilder::new()
        .stroke_color(COLOR_TEXT)
        .stroke_width(1)
        .build();
    let _ = Circle::with_center(center, 24)
        .into_styled(ring)
        .draw(display);
    let _ = Line::new(
        Point::new(center.x - 18, center.y),
        Point::new(center.x + 18, center.y),
    )
    .into_styled(cross)
    .draw(display);
    let _ = Line::new(
        Point::new(center.x, center.y - 18),
        Point::new(center.x, center.y + 18),
    )
    .into_styled(cross)
    .draw(display);
}

pub fn draw_unlock_spinner_frame(display: &mut GuiDisplay<'_>, frame: u8) {
    let center = Point::new(
        (BOOT_LOGO_WIDTH / 2) as i32,
        header_height() + row_height() * 2,
    );
    let erase = Rectangle::new(Point::new(center.x - 10, center.y - 12), Size::new(20, 24));
    let _ = erase
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_BACKGROUND)
                .build(),
        )
        .draw(display);
    let mut buf = [0u8; 4];
    let spinner_str = SPINNER_FRAMES[frame as usize].encode_utf8(&mut buf);
    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
    let _ = Text::with_alignment(
        spinner_str,
        Point::new(center.x, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

fn draw_panel(display: &mut GuiDisplay<'_>, top_left: Point, size: Size) {
    let panel = Rectangle::new(top_left, size);
    let _ = panel
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_PANEL_BASE)
                .stroke_color(COLOR_PANEL_BORDER)
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    if size.width > 4 && size.height > 4 {
        let highlight_height = (size.height / 5).clamp(4, 12);
        let highlight = Rectangle::new(
            Point::new(top_left.x + 1, top_left.y + 1),
            Size::new(size.width.saturating_sub(2), highlight_height),
        );
        let _ = highlight
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_PANEL_HIGHLIGHT)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);

        let shadow_height = highlight_height;
        let shadow_top = top_left.y + size.height as i32 - shadow_height as i32 - 1;
        let shadow = Rectangle::new(
            Point::new(top_left.x + 1, shadow_top),
            Size::new(size.width.saturating_sub(2), shadow_height),
        );
        let _ = shadow
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_PANEL_SHADOW)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
    }
}

pub fn render_idle_overlay(display: &mut GuiDisplay<'_>, message: &str) {
    if message.is_empty() {
        return;
    }
    let margin = IDLE_OVERLAY_MARGIN;
    let height = IDLE_OVERLAY_HEIGHT;
    if height <= 0 {
        return;
    }
    let width = SCREEN_WIDTH as i32 - margin * 2;
    if width <= 0 {
        return;
    }
    let top = SCREEN_HEIGHT as i32 - height - margin;
    if top < 0 {
        return;
    }
    let top_left = Point::new(margin, top);
    let size = Size::new(width as u32, height as u32);
    draw_panel(display, top_left, size);

    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let baseline = top + height / 2 + FONT_10X20.character_size.height as i32 / 3;
    let _ = Text::with_alignment(
        message,
        Point::new((SCREEN_WIDTH / 2) as i32, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

pub fn clear_idle_overlay(display: &mut GuiDisplay<'_>) {
    let margin = IDLE_OVERLAY_MARGIN;
    let height = IDLE_OVERLAY_HEIGHT;
    if height <= 0 {
        return;
    }
    let width = SCREEN_WIDTH as i32 - margin * 2;
    if width <= 0 {
        return;
    }
    let top = SCREEN_HEIGHT as i32 - height - margin;
    if top < 0 {
        return;
    }
    let rect = Rectangle::new(
        Point::new(margin, top),
        Size::new(width as u32, height as u32),
    );
    let _ = rect
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_BACKGROUND)
                .stroke_width(0)
                .build(),
        )
        .draw(display);
}

pub fn render_confirm_overlay(
    display: &mut GuiDisplay<'_>,
    prompt: &str,
    subtitle: Option<&str>,
    active_button: Option<Button>,
) {
    let header_h = header_height();
    if header_h < SCREEN_HEIGHT as i32 {
        let body = Rectangle::new(
            Point::new(0, header_h),
            Size::new(
                SCREEN_WIDTH.into(),
                (SCREEN_HEIGHT as i32 - header_h) as u32,
            ),
        );
        let _ = body
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_BACKGROUND)
                    .build(),
            )
            .draw(display);
    }

    let margin = 6;
    let buttons = confirm_buttons();
    let buttons_top = buttons[0].top_left.y;
    let panel_top = header_h + margin;
    let panel_bottom = (buttons_top - margin).max(panel_top + 40);
    let panel_height = (panel_bottom - panel_top).max(0);
    let panel_width = SCREEN_WIDTH as i32 - margin * 2;
    let panel_top_left = Point::new(margin, panel_top);
    let panel_size = Size::new(panel_width as u32, panel_height as u32);
    draw_panel(display, panel_top_left, panel_size);

    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let center_x = (SCREEN_WIDTH / 2) as i32;
    let prompt_baseline = panel_top + FONT_10X20.character_size.height as i32 + 2;
    let _ = Text::with_alignment(
        prompt,
        Point::new(center_x, prompt_baseline),
        style,
        Alignment::Center,
    )
    .draw(display);

    if let Some(details) = subtitle.filter(|s| !s.is_empty()) {
        let subtle = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT_SUBTLE);
        let line_gap: i32 = 2;
        let line_h: i32 = FONT_10X20.character_size.height as i32 + line_gap;
        let mut baseline = prompt_baseline + line_h + 6;

        for (idx, line) in details
            .lines()
            .filter(|l| !l.is_empty())
            .take(3)
            .enumerate()
        {
            let line_style = if idx == 0 { style } else { subtle };
            let _ = Text::with_alignment(
                line,
                Point::new(center_x, baseline),
                line_style,
                Alignment::Center,
            )
            .draw(display);
            baseline += line_h;
        }
    }

    for hit in buttons {
        let (base, light, dark) = match hit.button {
            Button::Ok => (
                COLOR_BTN_PRIMARY_BASE,
                COLOR_BTN_PRIMARY_LIGHT,
                COLOR_BTN_PRIMARY_DARK,
            ),
            Button::Clear => (
                COLOR_BTN_SECONDARY_BASE,
                COLOR_BTN_SECONDARY_LIGHT,
                COLOR_BTN_SECONDARY_DARK,
            ),
            _ => (
                COLOR_BTN_DISABLED_BASE,
                COLOR_BTN_DISABLED_LIGHT,
                COLOR_BTN_DISABLED_DARK,
            ),
        };
        let is_active = active_button == Some(hit.button);
        draw_button_skeuo(display, hit, base, light, dark, is_active);

        let label = confirm_button_label(hit.button);
        if !label.is_empty() {
            let label_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
            let center = Point::new(
                hit.top_left.x + hit.size.width as i32 / 2,
                hit.top_left.y + hit.size.height as i32 / 2,
            );
            let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
            let _ = Text::with_alignment(
                label,
                Point::new(center.x, baseline),
                label_style,
                Alignment::Center,
            )
            .draw(display);
        }
    }
}

pub fn render_tx_review_overlay(
    display: &mut GuiDisplay<'_>,
    outputs: &[TxReviewOutput],
    summary: Option<TxReviewSummary>,
    scroll_y: i32,
    expanded_index: Option<usize>,
    active_button: Option<Button>,
) {
    let header_h = header_height();
    if header_h < SCREEN_HEIGHT as i32 {
        let body = Rectangle::new(
            Point::new(0, header_h),
            Size::new(
                SCREEN_WIDTH.into(),
                (SCREEN_HEIGHT as i32 - header_h) as u32,
            ),
        );
        let _ = body
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_BACKGROUND)
                    .build(),
            )
            .draw(display);
    }

    let list_rect = tx_review_list_rect();
    draw_panel(display, list_rect.top_left, list_rect.size);

    let inner_left = list_rect.top_left.x + TX_REVIEW_PADDING;
    let inner_top = list_rect.top_left.y + TX_REVIEW_PADDING;
    let inner_bottom = list_rect.top_left.y + list_rect.size.height as i32 - TX_REVIEW_PADDING;
    let summary_h = tx_review_summary_height(summary.is_some());
    let output_top = (inner_top + summary_h).min(inner_bottom);

    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let subtle = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let summary_style = MonoTextStyle::new(&FONT_10X20, COLOR_ACCENT_INFO);
    let warning_style = MonoTextStyle::new(&FONT_10X20, COLOR_ACCENT_WARNING);

    fn write_amount(buf: &mut heapless::String<32>, gift_nicks: u64) {
        const NICKS_PER_NOCK: u64 = 1 << 16; // 65536
        buf.clear();

        if gift_nicks >= NICKS_PER_NOCK {
            let whole = gift_nicks / NICKS_PER_NOCK;
            let rem = gift_nicks % NICKS_PER_NOCK;
            let mut frac = (rem.saturating_mul(100) + (NICKS_PER_NOCK / 2)) / NICKS_PER_NOCK;
            let mut whole = whole;
            if frac >= 100 {
                whole = whole.saturating_add(1);
                frac = 0;
            }
            let _ = write!(buf, "{}.{:02} N", whole, frac);
        } else {
            let _ = write!(buf, "{} n", gift_nicks);
        }
    }

    fn write_amount_short(buf: &mut heapless::String<32>, gift_nicks: u64) {
        const NICKS_PER_NOCK: u64 = 1 << 16; // 65536
        buf.clear();

        if gift_nicks >= NICKS_PER_NOCK {
            let whole = gift_nicks / NICKS_PER_NOCK;
            let rem = gift_nicks % NICKS_PER_NOCK;
            let mut frac = (rem.saturating_mul(100) + (NICKS_PER_NOCK / 2)) / NICKS_PER_NOCK;
            let mut whole = whole;
            if frac >= 100 {
                whole = whole.saturating_add(1);
                frac = 0;
            }
            if whole >= 1_000_000 {
                let _ = write!(buf, "{}N", whole);
            } else {
                let _ = write!(buf, "{}.{:02}N", whole, frac);
            }
        } else {
            let _ = write!(buf, "{}n", gift_nicks);
        }
    }

    fn write_truncated_recipient(buf: &mut heapless::String<32>, recipient: &str) {
        buf.clear();
        let bytes = recipient.as_bytes();
        const HEAD: usize = 4;
        const TAIL: usize = 4;
        if bytes.len() <= HEAD + 3 + TAIL {
            let _ = buf.push_str(recipient);
            return;
        }
        let head = core::str::from_utf8(&bytes[..HEAD]).unwrap_or("");
        let tail = core::str::from_utf8(&bytes[bytes.len() - TAIL..]).unwrap_or("");
        let _ = write!(buf, "{}...{}", head, tail);
    }

    fn write_warning(buf: &mut heapless::String<32>, flags: u8) -> bool {
        buf.clear();
        if flags & TX_REVIEW_FLAG_HIGH_FEE != 0 {
            let _ = buf.push_str("WARN high fee");
            return true;
        }
        if flags & TX_REVIEW_FLAG_NO_REFUND != 0 {
            let _ = buf.push_str("WARN no chg");
            return true;
        }
        if flags & TX_REVIEW_FLAG_MULTIPLE_RECIPIENTS != 0 {
            let _ = buf.push_str("WARN multi out");
            return true;
        }
        false
    }

    let line_h: i32 = FONT_10X20.character_size.height as i32 + TX_REVIEW_LINE_GAP;
    let item_h = tx_review_output_item_height();

    if let Some(summary) = summary {
        let mut baseline = inner_top + FONT_10X20.character_size.height as i32;
        let mut amount = heapless::String::<32>::new();
        let mut line = heapless::String::<32>::new();

        write_amount_short(&mut amount, summary.external_total);
        let _ = write!(line, "OUT {}", amount.as_str());
        let _ = Text::new(
            line.as_str(),
            Point::new(inner_left, baseline),
            summary_style,
        )
        .draw(display);
        baseline += line_h;
        line.clear();

        write_amount_short(&mut amount, summary.fee_total);
        let _ = write!(line, "FEE {}", amount.as_str());
        let _ = Text::new(
            line.as_str(),
            Point::new(inner_left, baseline),
            summary_style,
        )
        .draw(display);
        baseline += line_h;
        line.clear();

        write_amount_short(&mut amount, summary.refund_total);
        let _ = write!(line, "CHG {}", amount.as_str());
        let _ = Text::new(
            line.as_str(),
            Point::new(inner_left, baseline),
            summary_style,
        )
        .draw(display);
        baseline += line_h;
        line.clear();

        if write_warning(&mut line, summary.flags) {
            let _ = Text::new(
                line.as_str(),
                Point::new(inner_left, baseline),
                warning_style,
            )
            .draw(display);
        } else {
            let _ = write!(
                line,
                "IN {} OUT {}",
                summary.input_count, summary.external_output_count
            );
            let _ = Text::new(
                line.as_str(),
                Point::new(inner_left, baseline),
                summary_style,
            )
            .draw(display);
        }
    }

    if outputs.is_empty() {
        let center_x = (SCREEN_WIDTH / 2) as i32;
        let available_h = inner_bottom.saturating_sub(output_top);
        let baseline = output_top + available_h / 2;
        let _ = Text::with_alignment(
            "No external",
            Point::new(center_x, baseline),
            style,
            Alignment::Center,
        )
        .draw(display);
    } else {
        let mut y = output_top - scroll_y;
        for out in outputs {
            let item_top = y;
            let item_bottom = item_top + item_h;

            if item_bottom < output_top {
                y = y.saturating_add(item_h);
                continue;
            }
            if item_top > inner_bottom {
                break;
            }

            let mut baseline = y + FONT_10X20.character_size.height as i32;
            let mut line1 = heapless::String::<32>::new();
            write_amount(&mut line1, out.gift);
            let _ =
                Text::new(line1.as_str(), Point::new(inner_left, baseline), style).draw(display);

            baseline += line_h;
            let mut line2 = heapless::String::<32>::new();
            write_truncated_recipient(&mut line2, out.recipient_b58.as_str());
            let _ =
                Text::new(line2.as_str(), Point::new(inner_left, baseline), subtle).draw(display);

            y = y.saturating_add(item_h);
        }
    }

    if let Some(idx) = expanded_index {
        if let Some(out) = outputs.get(idx) {
            let detail_rect = tx_review_detail_rect();
            draw_panel(display, detail_rect.top_left, detail_rect.size);

            let padding: i32 = 6;
            let left = detail_rect.top_left.x + padding;
            let top = detail_rect.top_left.y + padding;
            let bottom = detail_rect.top_left.y + detail_rect.size.height as i32 - padding;

            let font_w: i32 = FONT_10X20.character_size.width as i32;
            let inner_w = (detail_rect.size.width as i32 - padding * 2).max(font_w);
            let max_chars = (inner_w / font_w).max(1) as usize;

            let mut baseline = top + FONT_10X20.character_size.height as i32;

            let mut amount = heapless::String::<32>::new();
            write_amount(&mut amount, out.gift);
            let _ = Text::new(amount.as_str(), Point::new(left, baseline), style).draw(display);
            baseline += line_h;

            let recipient = out.recipient_b58.as_str();
            let bytes = recipient.as_bytes();
            let mut pos: usize = 0;
            while pos < bytes.len() && baseline <= bottom {
                let end = (pos + max_chars).min(bytes.len());
                let part = core::str::from_utf8(&bytes[pos..end]).unwrap_or("");
                let _ = Text::new(part, Point::new(left, baseline), subtle).draw(display);
                pos = end;
                baseline += line_h;
            }
        }
    }

    for hit in tx_review_buttons() {
        let (base, light, dark, label) = match hit.button {
            Button::Ok => (
                COLOR_BTN_PRIMARY_BASE,
                COLOR_BTN_PRIMARY_LIGHT,
                COLOR_BTN_PRIMARY_DARK,
                "Approve",
            ),
            Button::Clear => (
                COLOR_BTN_SECONDARY_BASE,
                COLOR_BTN_SECONDARY_LIGHT,
                COLOR_BTN_SECONDARY_DARK,
                "Deny",
            ),
            _ => (
                COLOR_BTN_DISABLED_BASE,
                COLOR_BTN_DISABLED_LIGHT,
                COLOR_BTN_DISABLED_DARK,
                "",
            ),
        };

        let is_active = active_button == Some(hit.button);
        draw_button_skeuo(display, hit, base, light, dark, is_active);

        if !label.is_empty() {
            let label_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
            let center = Point::new(
                hit.top_left.x + hit.size.width as i32 / 2,
                hit.top_left.y + hit.size.height as i32 / 2,
            );
            let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
            let _ = Text::with_alignment(
                label,
                Point::new(center.x, baseline),
                label_style,
                Alignment::Center,
            )
            .draw(display);
        }
    }
}

pub fn draw_unlock_header(display: &mut GuiDisplay<'_>, active: bool) {
    let header_h = header_height();
    let width = SCREEN_WIDTH as i32;
    let base = COLOR_SURFACE_HIGH;

    let header_rect = Rectangle::new(Point::new(0, 0), Size::new(width as u32, header_h as u32));
    let _ = header_rect
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(base)
                .stroke_color(COLOR_DIVIDER)
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    if header_h > 4 {
        let highlight = Rectangle::new(
            Point::new(1, 1),
            Size::new((width - 2) as u32, (header_h / 3).max(2) as u32),
        );
        let _ = highlight
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_PANEL_HIGHLIGHT)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);

        let shadow_height = (header_h / 3).max(2);
        let shadow_top = header_h - shadow_height - 1;
        let shadow = Rectangle::new(
            Point::new(1, shadow_top),
            Size::new((width - 2) as u32, shadow_height as u32),
        );
        let _ = shadow
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_PANEL_SHADOW)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
    }

    draw_header_lock_icon(display, active, base);
    draw_settings_gear(display, false, base);
}

fn draw_button_skeuo(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    base: Rgb565,
    light: Rgb565,
    dark: Rgb565,
    active: bool,
) {
    let shadow_offset = 2;
    let shadow_rect = Rectangle::new(
        Point::new(
            hit.top_left.x + shadow_offset,
            hit.top_left.y + shadow_offset,
        ),
        hit.size,
    );
    let _ = shadow_rect
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_PANEL_SHADOW)
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let main_rect = Rectangle::new(hit.top_left, hit.size);
    let base_color = if active { light } else { base };
    let _ = main_rect
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(base_color)
                .stroke_color(COLOR_PANEL_BORDER)
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    if hit.size.height > 6 {
        let highlight = Rectangle::new(
            Point::new(hit.top_left.x + 1, hit.top_left.y + 1),
            Size::new(hit.size.width.saturating_sub(2), hit.size.height / 3),
        );
        let _ = highlight
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(light)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);

        let shadow_height = hit.size.height / 4;
        let shadow_top = hit.top_left.y + hit.size.height as i32 - shadow_height as i32 - 1;
        let shadow = Rectangle::new(
            Point::new(hit.top_left.x + 1, shadow_top),
            Size::new(hit.size.width.saturating_sub(2), shadow_height),
        );
        let shadow_color = if active { base } else { dark };
        let _ = shadow
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(shadow_color)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
    }
}

fn button_label(button: Button) -> &'static str {
    match button {
        Button::Digit(0) => "0",
        Button::Digit(1) => "1",
        Button::Digit(2) => "2",
        Button::Digit(3) => "3",
        Button::Digit(4) => "4",
        Button::Digit(5) => "5",
        Button::Digit(6) => "6",
        Button::Digit(7) => "7",
        Button::Digit(8) => "8",
        Button::Digit(9) => "9",
        Button::Digit(_) => "?",
        Button::Clear => "X",
        Button::Ok => "OK",
        Button::Seed(_) => "",
        Button::Menu(_) => "",
    }
}

fn confirm_button_label(button: Button) -> &'static str {
    match button {
        Button::Ok => "Approve",
        Button::Clear => "Deny",
        Button::Digit(_) => "",
        Button::Seed(_) => "",
        Button::Menu(_) => "",
    }
}

struct Palette {
    base: Rgb565,
    light: Rgb565,
    dark: Rgb565,
    border: Rgb565,
}

fn button_palette(mode: GuiMode, button: Button, active: bool) -> Palette {
    match mode {
        GuiMode::Confirm => {
            let (base, light, dark) = match button {
                Button::Ok => (
                    COLOR_BTN_PRIMARY_BASE,
                    COLOR_BTN_PRIMARY_LIGHT,
                    COLOR_BTN_PRIMARY_DARK,
                ),
                Button::Clear => (
                    COLOR_BTN_SECONDARY_BASE,
                    COLOR_BTN_SECONDARY_LIGHT,
                    COLOR_BTN_SECONDARY_DARK,
                ),
                _ => (
                    COLOR_KEYPAD_IDLE,
                    COLOR_BTN_DISABLED_LIGHT,
                    COLOR_BTN_DISABLED_DARK,
                ),
            };
            let base_color = if active { light } else { base };
            let dark_color = if active {
                match button {
                    Button::Ok => COLOR_BTN_PRIMARY_DARK,
                    Button::Clear => COLOR_BTN_SECONDARY_DARK,
                    _ => dark,
                }
            } else {
                dark
            };
            Palette {
                base: base_color,
                light,
                dark: dark_color,
                border: COLOR_DIVIDER,
            }
        }
        _ => {
            if active {
                Palette {
                    base: COLOR_KEYPAD_ACTIVE,
                    light: COLOR_KEYPAD_ACTIVE_LIGHT,
                    dark: COLOR_KEYPAD_ACTIVE_DARK,
                    border: COLOR_KEYPAD_BORDER,
                }
            } else {
                Palette {
                    base: COLOR_KEYPAD_IDLE,
                    light: COLOR_BTN_DISABLED_LIGHT,
                    dark: COLOR_BTN_DISABLED_DARK,
                    border: COLOR_KEYPAD_BORDER,
                }
            }
        }
    }
}
