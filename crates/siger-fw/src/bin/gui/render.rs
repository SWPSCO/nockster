use embedded_graphics::mono_font::{ascii::FONT_10X20, MonoTextStyle};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};

use super::constants::*;
use super::layout::{header_height, keypad_button_hit, keypad_grid, row_height};
use super::state::{Button, ButtonHit, GuiMode};
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
    for (row_idx, row) in keypad_grid().iter().enumerate() {
        for col_idx in 0..row.len() {
            let hit = keypad_button_hit(row_idx, col_idx);
            draw_button(display, GuiMode::Locked, hit, false);
        }
    }
}

pub fn draw_button(display: &mut GuiDisplay<'_>, mode: GuiMode, hit: ButtonHit, active: bool) {
    let (fill_color, border_color) = match (mode, hit.button, active) {
        (GuiMode::Confirm, Button::Ok, true) => (COLOR_CONFIRM_APPROVE_ACTIVE, COLOR_BUTTON_BORDER),
        (GuiMode::Confirm, Button::Ok, false) => (COLOR_CONFIRM_APPROVE, COLOR_BUTTON_BORDER),
        (GuiMode::Confirm, Button::Clear, true) => {
            (COLOR_CONFIRM_REJECT_ACTIVE, COLOR_BUTTON_BORDER)
        }
        (GuiMode::Confirm, Button::Clear, false) => (COLOR_CONFIRM_REJECT, COLOR_BUTTON_BORDER),
        (_, _, true) => (COLOR_BUTTON_ACTIVE, COLOR_BUTTON_BORDER),
        (_, _, false) => (COLOR_BUTTON, COLOR_BUTTON_BORDER),
    };

    let button_style = PrimitiveStyleBuilder::new()
        .fill_color(fill_color)
        .stroke_color(border_color)
        .stroke_width(2)
        .build();
    let rect = Rectangle::new(hit.top_left, hit.size);
    let _ = rect.into_styled(button_style).draw(display);

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
    }
}

fn confirm_button_label(button: Button) -> &'static str {
    match button {
        Button::Ok => "Approve",
        Button::Clear => "Reject",
        Button::Digit(_) => "",
    }
}
