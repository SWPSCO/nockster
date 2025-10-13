use embedded_graphics::mono_font::{ascii::FONT_10X20, MonoTextStyle};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};

use super::constants::*;
use super::layout::{confirm_buttons, header_height, keypad_button_hit, keypad_grid, row_height};
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

    let frame = Rectangle::new(
        Point::new(4, header_height()),
        Size::new(
            (BOOT_LOGO_WIDTH - 8) as u32,
            (BOOT_LOGO_HEIGHT as i32 - header_height() - 8)
                .max(0) as u32,
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
        let shadow_top =
            hit.top_left.y + hit.size.height as i32 - shadow_height as i32 - 1;
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
        let highlight_height = (size.height / 3).max(4);
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

        let shadow_height = (size.height / 3).max(4);
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

pub fn render_confirm_overlay(
    display: &mut GuiDisplay<'_>,
    prompt: &str,
    subtitle: Option<&str>,
    active_button: Option<Button>,
) {
    let margin = 8;
    let panel_height = 72;
    let panel_width = SCREEN_WIDTH as i32 - margin * 2;
    let panel_top_left = Point::new(margin, margin);
    let panel_size = Size::new(panel_width as u32, panel_height as u32);
    draw_panel(display, panel_top_left, panel_size);

    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let center_x = (SCREEN_WIDTH / 2) as i32;
    let prompt_baseline = margin + FONT_10X20.character_size.height as i32 + 2;
    let _ = Text::with_alignment(
        prompt,
        Point::new(center_x, prompt_baseline),
        style,
        Alignment::Center,
    )
    .draw(display);

    if let Some(line) = subtitle.filter(|s| !s.is_empty()) {
        let subtitle_baseline = margin + panel_height - 12;
        let _ = Text::with_alignment(
            line,
            Point::new(center_x, subtitle_baseline),
            style,
            Alignment::Center,
        )
        .draw(display);
    }

    for hit in confirm_buttons() {
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
        Point::new(hit.top_left.x + shadow_offset, hit.top_left.y + shadow_offset),
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
        let shadow_top =
            hit.top_left.y + hit.size.height as i32 - shadow_height as i32 - 1;
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
    }
}

fn confirm_button_label(button: Button) -> &'static str {
    match button {
        Button::Ok => "Approve",
        Button::Clear => "Reject",
        Button::Digit(_) => "",
    }
}

struct Palette {
    base: Rgb565,
    light: Rgb565,
    dark: Rgb565,
    border: Rgb565,
}

fn button_palette(mode: GuiMode, button: Button, active: bool) -> Palette {
    let (base, light, dark) = match (mode, button) {
        (GuiMode::Confirm, Button::Ok) => (
            COLOR_BTN_PRIMARY_BASE,
            COLOR_BTN_PRIMARY_LIGHT,
            COLOR_BTN_PRIMARY_DARK,
        ),
        (GuiMode::Confirm, Button::Clear) => (
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

    let adjusted_base = if active { light } else { base };
    let adjusted_light = if active { light } else { light };
    let adjusted_dark = if active { base } else { dark };

    Palette {
        base: adjusted_base,
        light: adjusted_light,
        dark: adjusted_dark,
        border: COLOR_DIVIDER,
    }
}
