//! Settings menu reachable from the unlocked header menu icon, plus the wallet list.
//! Rendering only — all decisions (what needs an unlock, etc.) live in the main
//! event loop.

use core::fmt::Write as _;

use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_8X13};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use heapless::{String as HString, Vec as HVec};
use nockster_core::MAX_SEED_SLOTS;

use super::constants::*;
use super::layout::header_height;
use super::render::render_header;
use super::scroll::{self, ScrollContent, ScrollState};
use super::seed::draw_text_button;
use super::state::{Button, ButtonHit, MenuItem};
use super::GuiDisplay;

const MENU_MARGIN: i32 = 16;
const MENU_BUTTON_HEIGHT: i32 = 34;
const MENU_BUTTON_GAP: i32 = 10;

/// One row in the wallet list (a seed slot the user can examine).
#[derive(Clone)]
pub struct WalletRow {
    pub index: u8,
    pub active: bool,
    pub label: HString<32>,
    pub pkh: HString<64>,
}

pub type WalletRows = HVec<WalletRow, MAX_SEED_SLOTS>;

fn menu_order() -> [MenuItem; 5] {
    [
        MenuItem::Wallets,
        MenuItem::AddSeed,
        MenuItem::Calibrate,
        MenuItem::Diagnostics,
        MenuItem::Back,
    ]
}

fn menu_label(item: MenuItem) -> &'static str {
    match item {
        MenuItem::Wallets => "Wallets",
        MenuItem::AddSeed => "Add Seed",
        MenuItem::Calibrate => "Calibrate",
        MenuItem::Diagnostics => "Diagnostics",
        MenuItem::Back => "Back",
    }
}

fn menu_button(index: usize, item: MenuItem) -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = header_height() + 14 + index as i32 * (MENU_BUTTON_HEIGHT + MENU_BUTTON_GAP);
    ButtonHit {
        button: Button::Menu(item),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, MENU_BUTTON_HEIGHT as u32),
    }
}

pub fn menu_buttons() -> [ButtonHit; 5] {
    let order = menu_order();
    [
        menu_button(0, order[0]),
        menu_button(1, order[1]),
        menu_button(2, order[2]),
        menu_button(3, order[3]),
        menu_button(4, order[4]),
    ]
}

pub fn render_menu(display: &mut GuiDisplay<'_>) {
    let _ = display.clear(COLOR_BACKGROUND);
    render_header(display, "Settings", COLOR_SURFACE_HIGH);
    for hit in menu_buttons() {
        let label = match hit.button {
            Button::Menu(item) => menu_label(item),
            _ => "",
        };
        draw_text_button(display, hit, label, false);
        draw_menu_affordance(display, hit, false);
    }
}

pub fn draw_menu_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let label = match hit.button {
        Button::Menu(item) => menu_label(item),
        _ => "",
    };
    draw_text_button(display, hit, label, active);
    draw_menu_affordance(display, hit, active);
}

pub fn button_from_point_menu(point: Point) -> Option<ButtonHit> {
    menu_buttons().into_iter().find(|hit| within(hit, point, 6))
}

/// Bottom "Back" button shared by the wallet view.
fn wallets_back_button() -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = SCREEN_HEIGHT as i32 - MENU_BUTTON_HEIGHT - 8;
    ButtonHit {
        button: Button::Menu(MenuItem::Back),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, MENU_BUTTON_HEIGHT as u32),
    }
}

pub fn button_from_point_wallets(
    point: Point,
    rows: &[WalletRow],
    scroll: &ScrollState,
) -> Option<ButtonHit> {
    let back = wallets_back_button();
    if within(&back, point, 8) {
        return Some(back);
    }
    wallet_row_hit(point, rows, scroll)
}

/// The scrollable region of the wallet screen (between the slot-count summary and
/// the Back button).
pub fn wallets_viewport() -> Rectangle {
    let top = header_height() + 48;
    let bottom = SCREEN_HEIGHT as i32 - MENU_BUTTON_HEIGHT - 14;
    Rectangle::new(
        Point::new(6, top),
        Size::new((SCREEN_WIDTH - 12) as u32, (bottom - top).max(0) as u32),
    )
}

/// Full wallet screen: header, slot count, Back, and the scrollable slot list.
pub fn render_wallets(display: &mut GuiDisplay<'_>, rows: &[WalletRow], scroll: &mut ScrollState) {
    let _ = display.clear(COLOR_BACKGROUND);
    render_header(display, "Wallets", COLOR_SURFACE_HIGH);

    let subtle = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
    let mut summary = HString::<32>::new();
    let _ = write!(summary, "{} of {} slots", rows.len(), MAX_SEED_SLOTS);
    let _ = Text::with_alignment(
        summary.as_str(),
        Point::new(8, header_height() + 16),
        subtle,
        Alignment::Left,
    )
    .draw(display);

    draw_wallet_table_header(display);
    let back = wallets_back_button();
    draw_text_button(display, back, "Back", false);
    draw_menu_affordance(display, back, false);
    render_wallets_viewport(display, rows, scroll);
}

/// Re-renders only the scrollable list region (used on each drag step).
pub fn render_wallets_viewport(
    display: &mut GuiDisplay<'_>,
    rows: &[WalletRow],
    scroll: &mut ScrollState,
) {
    scroll::render(display, scroll, &WalletList { rows });
}

struct WalletList<'a> {
    rows: &'a [WalletRow],
}

impl ScrollContent for WalletList<'_> {
    fn content_height(&self) -> i32 {
        if self.rows.is_empty() {
            wallets_viewport().size.height as i32
        } else {
            self.rows.len() as i32 * WALLET_ROW_HEIGHT
        }
    }

    fn draw_content<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let primary = MonoTextStyle::new(&FONT_8X13, COLOR_TEXT);
        let subtle = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
        let active_style = MonoTextStyle::new(&FONT_6X10, COLOR_ACCENT_WARNING);
        if self.rows.is_empty() {
            let empty = Rectangle::new(
                Point::new(2, 10),
                Size::new(wallets_viewport().size.width.saturating_sub(4), 56),
            );
            let _ = empty
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLOR_SURFACE_LOW)
                        .stroke_color(COLOR_DIVIDER)
                        .stroke_width(1)
                        .build(),
                )
                .draw(target);
            let _ = Line::new(
                Point::new(4, 12),
                Point::new(wallets_viewport().size.width as i32 - 6, 12),
            )
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(COLOR_PANEL_HIGHLIGHT)
                    .stroke_width(1)
                    .build(),
            )
            .draw(target);
            let _ = Rectangle::new(Point::new(9, 24), Size::new(3, 28))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLOR_PANEL_HIGHLIGHT)
                        .stroke_width(0)
                        .build(),
                )
                .draw(target);
            let _ = Text::with_alignment(
                "No wallets",
                Point::new((wallets_viewport().size.width / 2) as i32, 44),
                primary,
                Alignment::Center,
            )
            .draw(target);
            return;
        }

        let mut y = 0;
        for row in self.rows {
            let face = Rectangle::new(
                Point::new(2, y + 2),
                Size::new(
                    wallets_viewport().size.width.saturating_sub(4),
                    (WALLET_ROW_HEIGHT - 4) as u32,
                ),
            );
            let _ = face
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(if row.active {
                            COLOR_SURFACE_LOW
                        } else {
                            COLOR_BACKGROUND
                        })
                        .stroke_color(COLOR_DIVIDER)
                        .stroke_width(1)
                        .build(),
                )
                .draw(target);
            let _ = Line::new(
                Point::new(4, y + 4),
                Point::new(wallets_viewport().size.width as i32 - 6, y + 4),
            )
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(COLOR_PANEL_HIGHLIGHT)
                    .stroke_width(1)
                    .build(),
            )
            .draw(target);
            if row.active {
                let _ = Rectangle::new(Point::new(3, y + 7), Size::new(3, 26))
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(COLOR_KEYPAD_ACTIVE_LIGHT)
                            .stroke_width(0)
                            .build(),
                    )
                    .draw(target);
            }

            let slot_badge = Rectangle::new(Point::new(8, y + 7), Size::new(18, 18));
            let _ = slot_badge
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLOR_PANEL_SHADOW)
                        .stroke_color(COLOR_DIVIDER)
                        .stroke_width(1)
                        .build(),
                )
                .draw(target);
            let mut slot = HString::<6>::new();
            let _ = write!(slot, "{}", row.index);
            let _ = Text::with_alignment(
                slot.as_str(),
                Point::new(17, y + 20),
                primary,
                Alignment::Center,
            )
            .draw(target);

            let mut name = HString::<18>::new();
            if row.label.is_empty() {
                let _ = name.push_str("Wallet");
            } else {
                push_truncated(
                    &mut name,
                    row.label.as_str(),
                    if row.active { 10 } else { 15 },
                );
            }
            let _ = Text::with_alignment(
                name.as_str(),
                Point::new(38, y + 15),
                primary,
                Alignment::Left,
            )
            .draw(target);

            if row.active {
                let _ = Text::with_alignment(
                    "ACTIVE",
                    Point::new(wallets_viewport().size.width as i32 - 4, y + 13),
                    active_style,
                    Alignment::Right,
                )
                .draw(target);
            }

            let mut short_pkh = HString::<24>::new();
            if !row.pkh.is_empty() {
                push_short_pkh(&mut short_pkh, row.pkh.as_str());
            }
            let address = if short_pkh.is_empty() {
                "address unavailable"
            } else {
                short_pkh.as_str()
            };
            let _ = Text::with_alignment(address, Point::new(38, y + 31), subtle, Alignment::Left)
                .draw(target);

            let _ = Line::new(
                Point::new(0, y + WALLET_ROW_HEIGHT - 1),
                Point::new(
                    wallets_viewport().size.width as i32,
                    y + WALLET_ROW_HEIGHT - 1,
                ),
            )
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(COLOR_DIVIDER)
                    .stroke_width(1)
                    .build(),
            )
            .draw(target);
            y += WALLET_ROW_HEIGHT;
        }
    }
}

const WALLET_ROW_HEIGHT: i32 = 42;

pub fn draw_wallets_back(display: &mut GuiDisplay<'_>, active: bool) {
    let back = wallets_back_button();
    draw_text_button(display, back, "Back", active);
    draw_menu_affordance(display, back, active);
}

fn within(hit: &ButtonHit, point: Point, slack: i32) -> bool {
    let left = hit.top_left.x - slack;
    let right = hit.top_left.x + hit.size.width as i32 + slack;
    let top = hit.top_left.y - slack;
    let bottom = hit.top_left.y + hit.size.height as i32 + slack;
    point.x >= left && point.x < right && point.y >= top && point.y < bottom
}

fn draw_wallet_table_header(display: &mut GuiDisplay<'_>) {
    let style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
    let y = header_height() + 38;
    let _ = Text::with_alignment("Slot", Point::new(16, y), style, Alignment::Left).draw(display);
    let _ = Text::with_alignment("Nick", Point::new(44, y), style, Alignment::Left).draw(display);
    let _ = Text::with_alignment(
        "Address",
        Point::new(SCREEN_WIDTH as i32 - 8, y),
        style,
        Alignment::Right,
    )
    .draw(display);
    let _ = Line::new(
        Point::new(6, y + 6),
        Point::new(SCREEN_WIDTH as i32 - 7, y + 6),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(COLOR_DIVIDER)
            .stroke_width(1)
            .build(),
    )
    .draw(display);
}

fn draw_menu_affordance(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let rail_color = if active {
        COLOR_TEXT
    } else {
        COLOR_PANEL_HIGHLIGHT
    };
    let rail = Rectangle::new(
        Point::new(hit.top_left.x + 6, hit.top_left.y + 8),
        Size::new(2, hit.size.height.saturating_sub(16)),
    );
    let _ = rail
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(rail_color)
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    if matches!(hit.button, Button::Menu(MenuItem::Back)) {
        return;
    }

    let x = hit.top_left.x + hit.size.width as i32 - 18;
    let y = hit.top_left.y + hit.size.height as i32 / 2;
    let style = PrimitiveStyleBuilder::new()
        .stroke_color(if active {
            COLOR_TEXT
        } else {
            COLOR_TEXT_SUBTLE
        })
        .stroke_width(1)
        .build();
    let _ = Line::new(Point::new(x, y - 5), Point::new(x + 5, y))
        .into_styled(style)
        .draw(display);
    let _ = Line::new(Point::new(x + 5, y), Point::new(x, y + 5))
        .into_styled(style)
        .draw(display);
}

fn wallet_row_hit(point: Point, rows: &[WalletRow], scroll: &ScrollState) -> Option<ButtonHit> {
    let viewport = scroll.viewport();
    if !viewport.contains(point) || rows.is_empty() {
        return None;
    }
    let content_y = point.y - viewport.top_left.y + scroll.offset_y();
    if content_y < 0 {
        return None;
    }
    let index = (content_y / WALLET_ROW_HEIGHT) as usize;
    let row = rows.get(index)?;
    let row_top = viewport.top_left.y + index as i32 * WALLET_ROW_HEIGHT - scroll.offset_y();
    Some(ButtonHit {
        button: Button::WalletRow(row.index),
        top_left: Point::new(viewport.top_left.x, row_top),
        size: Size::new(viewport.size.width, WALLET_ROW_HEIGHT as u32),
    })
}

fn push_short_pkh(out: &mut HString<24>, pkh: &str) {
    let bytes = pkh.as_bytes();
    if bytes.len() <= 20 {
        let _ = out.push_str(pkh);
        return;
    }

    let _ = out.push_str(core::str::from_utf8(&bytes[..8]).unwrap_or(""));
    let _ = out.push_str("...");
    let _ = out.push_str(core::str::from_utf8(&bytes[bytes.len() - 8..]).unwrap_or(""));
}

fn push_truncated<const N: usize>(out: &mut HString<N>, value: &str, max_chars: usize) {
    let bytes = value.as_bytes();
    if bytes.len() <= max_chars {
        let _ = out.push_str(value);
        return;
    }
    let take = max_chars.saturating_sub(2);
    let _ = out.push_str(core::str::from_utf8(&bytes[..take]).unwrap_or(""));
    let _ = out.push_str("..");
}
