//! Settings menu reachable from the unlocked header gear, plus the read-only
//! wallet list. Rendering only — all decisions (what needs an unlock, etc.)
//! live in the main event loop.

use core::fmt::Write as _;

use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_8X13};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
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
    }
}

pub fn draw_menu_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let label = match hit.button {
        Button::Menu(item) => menu_label(item),
        _ => "",
    };
    draw_text_button(display, hit, label, active);
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

pub fn button_from_point_wallets(point: Point) -> Option<ButtonHit> {
    let back = wallets_back_button();
    within(&back, point, 8).then_some(back)
}

/// The scrollable region of the wallet screen (between the slot-count summary and
/// the Back button).
pub fn wallets_viewport() -> Rectangle {
    let top = header_height() + 28;
    let bottom = SCREEN_HEIGHT as i32 - MENU_BUTTON_HEIGHT - 14;
    Rectangle::new(
        Point::new(0, top),
        Size::new(SCREEN_WIDTH as u32, (bottom - top).max(0) as u32),
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
        Point::new(MENU_MARGIN, header_height() + 18),
        subtle,
        Alignment::Left,
    )
    .draw(display);

    draw_text_button(display, wallets_back_button(), "Back", false);
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
        let mut h = 20;
        for row in self.rows {
            h += wallet_row_height(row);
        }
        h
    }

    fn draw_content<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let style = MonoTextStyle::new(&FONT_8X13, COLOR_TEXT);
        let left = MENU_MARGIN;
        let mut y = 13;
        for row in self.rows {
            let mut short_pkh = HString::<16>::new();
            if !row.pkh.is_empty() {
                push_short_pkh(&mut short_pkh, row.pkh.as_str());
            }

            let mut line = HString::<56>::new();
            let name = if row.label.is_empty() {
                short_pkh.as_str()
            } else {
                row.label.as_str()
            };
            let marker = if row.active { "*" } else { " " };
            let _ = write!(line, "{}slot {} {}", marker, row.index, name);
            let _ = Text::with_alignment(line.as_str(), Point::new(left, y), style, Alignment::Left)
                .draw(target);
            y += WALLET_LINE_H;

            if row.pkh.is_empty() && row.label.is_empty() {
                let _ = Text::with_alignment(
                    "pkh unavailable",
                    Point::new(left, y),
                    style,
                    Alignment::Left,
                )
                .draw(target);
                y += WALLET_SUBLINE_H;
            } else if !row.label.is_empty() && !short_pkh.is_empty() {
                let _ = Text::with_alignment(
                    short_pkh.as_str(),
                    Point::new(left, y),
                    style,
                    Alignment::Left,
                )
                .draw(target);
                y += WALLET_SUBLINE_H;
            }
            y += WALLET_ROW_GAP;
        }
    }
}

const WALLET_LINE_H: i32 = 18;
const WALLET_SUBLINE_H: i32 = 16;
const WALLET_ROW_GAP: i32 = 4;

fn wallet_has_second_line(row: &WalletRow) -> bool {
    (row.pkh.is_empty() && row.label.is_empty()) || (!row.label.is_empty() && !row.pkh.is_empty())
}

fn wallet_row_height(row: &WalletRow) -> i32 {
    WALLET_LINE_H + if wallet_has_second_line(row) { WALLET_SUBLINE_H } else { 0 } + WALLET_ROW_GAP
}

pub fn draw_wallets_back(display: &mut GuiDisplay<'_>, active: bool) {
    draw_text_button(display, wallets_back_button(), "Back", active);
}

fn within(hit: &ButtonHit, point: Point, slack: i32) -> bool {
    let left = hit.top_left.x - slack;
    let right = hit.top_left.x + hit.size.width as i32 + slack;
    let top = hit.top_left.y - slack;
    let bottom = hit.top_left.y + hit.size.height as i32 + slack;
    point.x >= left && point.x < right && point.y >= top && point.y < bottom
}

fn push_short_pkh(out: &mut HString<16>, pkh: &str) {
    let bytes = pkh.as_bytes();
    if bytes.len() <= 12 {
        let _ = out.push_str(pkh);
        return;
    }

    let _ = out.push_str(core::str::from_utf8(&bytes[..4]).unwrap_or(""));
    let _ = out.push_str("...");
    let _ = out.push_str(core::str::from_utf8(&bytes[bytes.len() - 4..]).unwrap_or(""));
}
