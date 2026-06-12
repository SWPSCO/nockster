//! Settings menu reachable from the unlocked header menu icon, plus the wallet list.
//! Rendering only — all decisions (what needs an unlock, etc.) live in the main
//! event loop.

use core::fmt::Write as _;

use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10, FONT_8X13};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use heapless::{String as HString, Vec as HVec};
use nockster_core::{BuildInfo, UpdateTrust, MAX_SEED_SLOTS};

use super::constants::*;
use super::layout::header_height;
use super::palette::{self, Theme};
use super::render::{draw_hold_hint_on_background, render_header_with_back, HOLD_DELETE_HINT};
use super::scroll::{self, ScrollContent, ScrollState};
use super::seed::draw_text_button;
use super::state::{Button, ButtonHit, MenuItem};
use super::GuiDisplay;

const MENU_MARGIN: i32 = 16;
const MENU_BUTTON_HEIGHT: i32 = 40;
const MENU_BUTTON_GAP: i32 = 10;
const MENU_CONTENT_TOP: i32 = 4;
const MENU_CONTENT_BOTTOM: i32 = 8;
const THEME_BUTTON_HEIGHT: i32 = 34;
const THEME_BUTTON_GAP: i32 = 2;

/// One row in the wallet list (a seed slot the user can examine).
#[derive(Clone)]
pub struct WalletRow {
    pub index: u8,
    pub active: bool,
    pub label: HString<32>,
    pub pkh: HString<64>,
}

pub type WalletRows = HVec<WalletRow, MAX_SEED_SLOTS>;

pub struct AboutInfo {
    pub fw_major: u16,
    pub fw_minor: u16,
    pub release_version: u32,
    pub build: BuildInfo,
    pub trust: UpdateTrust,
}

#[derive(Clone, Copy)]
struct MenuDefinition {
    item: MenuItem,
    label: &'static str,
}

const SETTINGS_MENU: [MenuDefinition; 7] = [
    MenuDefinition {
        item: MenuItem::Wallets,
        label: "Wallets",
    },
    MenuDefinition {
        item: MenuItem::AddSeed,
        label: "Add Seed",
    },
    MenuDefinition {
        item: MenuItem::Vault,
        label: "Vault",
    },
    MenuDefinition {
        item: MenuItem::Theme,
        label: "Theme",
    },
    MenuDefinition {
        item: MenuItem::Calibrate,
        label: "Calibrate",
    },
    MenuDefinition {
        item: MenuItem::Diagnostics,
        label: "Diagnostics",
    },
    MenuDefinition {
        item: MenuItem::About,
        label: "About",
    },
];

fn menu_label(item: MenuItem) -> &'static str {
    SETTINGS_MENU
        .iter()
        .find(|definition| definition.item == item)
        .map(|definition| definition.label)
        .unwrap_or("")
}

fn menu_button(index: usize, definition: MenuDefinition) -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = MENU_CONTENT_TOP + index as i32 * (MENU_BUTTON_HEIGHT + MENU_BUTTON_GAP);
    ButtonHit {
        button: Button::Menu(definition.item),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, MENU_BUTTON_HEIGHT as u32),
    }
}

fn menu_buttons() -> [ButtonHit; 7] {
    [
        menu_button(0, SETTINGS_MENU[0]),
        menu_button(1, SETTINGS_MENU[1]),
        menu_button(2, SETTINGS_MENU[2]),
        menu_button(3, SETTINGS_MENU[3]),
        menu_button(4, SETTINGS_MENU[4]),
        menu_button(5, SETTINGS_MENU[5]),
        menu_button(6, SETTINGS_MENU[6]),
    ]
}

pub fn menu_viewport() -> Rectangle {
    let top = header_height() + 4;
    Rectangle::new(
        Point::new(0, top),
        Size::new(
            SCREEN_WIDTH.into(),
            (SCREEN_HEIGHT as i32 - top).max(0) as u32,
        ),
    )
}

pub fn render_menu(display: &mut GuiDisplay<'_>, scroll: &mut ScrollState) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "Settings", palette::surface_high(), false);
    render_menu_viewport(display, scroll);
}

pub fn render_menu_viewport(display: &mut GuiDisplay<'_>, scroll: &mut ScrollState) {
    scroll::render(display, scroll, &MenuList);
}

struct MenuList;

impl ScrollContent for MenuList {
    fn content_height(&self) -> i32 {
        let count = menu_buttons().len() as i32;
        MENU_CONTENT_TOP
            + count * MENU_BUTTON_HEIGHT
            + count.saturating_sub(1) * MENU_BUTTON_GAP
            + MENU_CONTENT_BOTTOM
    }

    fn draw_content<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        for hit in menu_buttons() {
            let label = match hit.button {
                Button::Menu(item) => menu_label(item),
                _ => "",
            };
            draw_menu_text_button(target, hit, label, false);
            draw_menu_affordance(target, hit, false);
        }
    }
}

pub fn draw_menu_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let label = match hit.button {
        Button::Menu(item) => menu_label(item),
        _ => "",
    };
    draw_menu_text_button(display, hit, label, active);
    draw_menu_affordance(display, hit, active);
}

pub fn button_from_point_menu(point: Point, scroll: &ScrollState) -> Option<ButtonHit> {
    let viewport = scroll.viewport();
    if !viewport.contains(point) {
        return None;
    }
    let content_point = Point::new(
        point.x - viewport.top_left.x,
        point.y - viewport.top_left.y + scroll.offset_y(),
    );
    menu_buttons()
        .into_iter()
        .find(|hit| within(hit, content_point, 6))
        .map(|hit| menu_hit_to_screen(hit, viewport, scroll.offset_y()))
}

fn menu_hit_to_screen(hit: ButtonHit, viewport: Rectangle, offset_y: i32) -> ButtonHit {
    ButtonHit {
        button: hit.button,
        top_left: Point::new(
            viewport.top_left.x + hit.top_left.x,
            viewport.top_left.y + hit.top_left.y - offset_y,
        ),
        size: hit.size,
    }
}

fn theme_button(index: usize, theme: Theme) -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = header_height() + 3 + index as i32 * (THEME_BUTTON_HEIGHT + THEME_BUTTON_GAP);
    ButtonHit {
        button: Button::Theme(theme),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, THEME_BUTTON_HEIGHT as u32),
    }
}

pub fn theme_buttons() -> [ButtonHit; 6] {
    [
        theme_button(0, palette::THEMES[0]),
        theme_button(1, palette::THEMES[1]),
        theme_button(2, palette::THEMES[2]),
        theme_button(3, palette::THEMES[3]),
        theme_button(4, palette::THEMES[4]),
        theme_button(5, palette::THEMES[5]),
    ]
}

pub fn render_themes(display: &mut GuiDisplay<'_>) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "Theme", palette::surface_high(), false);
    for hit in theme_buttons() {
        draw_theme_button(display, hit, false);
    }
}

pub fn draw_theme_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    match hit.button {
        Button::Theme(theme) => {
            draw_text_button(display, hit, theme.name(), active);
            draw_theme_marker(display, hit, theme, active);
        }
        _ => {}
    }
}

pub fn button_from_point_themes(point: Point) -> Option<ButtonHit> {
    theme_buttons()
        .into_iter()
        .find(|hit| within(hit, point, 6))
}

pub fn render_about(display: &mut GuiDisplay<'_>, info: &AboutInfo) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "About", palette::surface_high(), false);

    let mut y = draw_about_intro(display, header_height() + 7);

    let mut fw = HString::<40>::new();
    let _ = write!(
        fw,
        "fw {}.{} release {}",
        info.fw_major, info.fw_minor, info.release_version
    );

    let mut profile = HString::<40>::new();
    let _ = write!(
        profile,
        "{} proto {}",
        info.build.build_profile.as_str(),
        info.build.protocol_v
    );

    let mut theme = HString::<40>::new();
    let _ = write!(theme, "theme {}", palette::current_theme().name());
    y = draw_about_card(
        display,
        y,
        "Firmware",
        &[fw.as_str(), profile.as_str(), theme.as_str()],
    );

    let mut commit = HString::<40>::new();
    let _ = commit.push_str("commit ");
    push_short_ascii(&mut commit, info.build.git_commit.as_str(), 10);
    if info.build.git_dirty {
        let _ = commit.push('*');
    }

    let mut tx = HString::<40>::new();
    let _ = tx.push_str("tx ");
    push_short_ascii(&mut tx, info.build.tx_types_rev.as_str(), 14);
    y = draw_about_card(display, y, "Build", &[commit.as_str(), tx.as_str()]);

    if info.trust.configured {
        let mut first = HString::<40>::new();
        let _ = first.push_str("sha256 ");
        push_hex_bytes(&mut first, &info.trust.pubkey_sha256[..8]);

        let mut last = HString::<40>::new();
        let _ = last.push_str("       ");
        push_hex_bytes(&mut last, &info.trust.pubkey_sha256[24..]);
        let _ = draw_about_card(display, y, "Trust Root", &[first.as_str(), last.as_str()]);
    } else {
        let _ = draw_about_card(display, y, "Trust Root", &["not configured"]);
    }
}

fn draw_about_intro(display: &mut GuiDisplay<'_>, top: i32) -> i32 {
    let x = 8;
    let width = SCREEN_WIDTH as i32 - x * 2;
    let height = 54;
    let panel = Rectangle::new(Point::new(x, top), Size::new(width as u32, height as u32));
    let _ = panel
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::surface_low())
                .stroke_color(palette::divider())
                .stroke_width(1)
                .build(),
        )
        .draw(display);
    let _ = Line::new(
        Point::new(x + 2, top + 2),
        Point::new(x + width - 3, top + 2),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(palette::panel_highlight())
            .stroke_width(1)
            .build(),
    )
    .draw(display);
    let _ = Rectangle::new(Point::new(x + 5, top + 7), Size::new(3, 38))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::keypad_active_light())
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let style = MonoTextStyle::new(&FONT_6X10, palette::text());
    let subtle = MonoTextStyle::new(&FONT_6X10, palette::text_subtle());
    let lines = [
        ("Nockster hardware wallet", style),
        ("for Nockchain by", subtle),
        ("Southwestern Pool", subtle),
        ("Supply Co - swps.io", subtle),
    ];
    let mut y = top + 13;
    for (line, text_style) in lines {
        let _ = Text::new(line, Point::new(x + 13, y), text_style).draw(display);
        y += 11;
    }

    top + height + 5
}

fn draw_about_card(display: &mut GuiDisplay<'_>, top: i32, title: &str, lines: &[&str]) -> i32 {
    let x = 8;
    let width = SCREEN_WIDTH as i32 - x * 2;
    let height = 26 + lines.len() as i32 * 12;
    let shadow = Rectangle::new(
        Point::new(x + 2, top + 2),
        Size::new(width as u32, height as u32),
    );
    let _ = shadow
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::panel_shadow())
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let card = Rectangle::new(Point::new(x, top), Size::new(width as u32, height as u32));
    let _ = card
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::surface_low())
                .stroke_color(palette::divider())
                .stroke_width(1)
                .build(),
        )
        .draw(display);
    let _ = Line::new(
        Point::new(x + 2, top + 2),
        Point::new(x + width - 3, top + 2),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(palette::panel_highlight())
            .stroke_width(1)
            .build(),
    )
    .draw(display);
    let _ = Rectangle::new(Point::new(x + 5, top + 8), Size::new(3, 14))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::keypad_active_light())
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let title_style = MonoTextStyle::new(&FONT_6X10, palette::text());
    let line_style = MonoTextStyle::new(&FONT_6X10, palette::text_subtle());
    let _ = Text::new(title, Point::new(x + 13, top + 16), title_style).draw(display);

    let mut y = top + 29;
    for line in lines {
        let _ = Text::new(line, Point::new(x + 13, y), line_style).draw(display);
        y += 12;
    }

    top + height + 5
}

fn push_short_ascii<const N: usize>(out: &mut HString<N>, value: &str, max: usize) {
    for byte in value.bytes().take(max) {
        let ch = if byte.is_ascii_graphic() || byte == b' ' {
            byte as char
        } else {
            '?'
        };
        let _ = out.push(ch);
    }
}

fn push_hex_bytes<const N: usize>(out: &mut HString<N>, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        let _ = out.push(HEX[(byte >> 4) as usize] as char);
        let _ = out.push(HEX[(byte & 0x0f) as usize] as char);
    }
}

pub fn button_from_point_about(_point: Point) -> Option<ButtonHit> {
    None
}

pub fn button_from_point_wallets(
    point: Point,
    rows: &[WalletRow],
    scroll: &ScrollState,
) -> Option<ButtonHit> {
    wallet_row_hit(point, rows, scroll)
}

/// The scrollable region of the wallet screen, below the slot-count summary.
pub fn wallets_viewport() -> Rectangle {
    let top = header_height() + 48;
    let bottom = SCREEN_HEIGHT as i32 - 7;
    Rectangle::new(
        Point::new(6, top),
        Size::new((SCREEN_WIDTH - 12) as u32, (bottom - top).max(0) as u32),
    )
}

/// Full wallet screen: header, slot count, and the scrollable slot list.
pub fn render_wallets(display: &mut GuiDisplay<'_>, rows: &[WalletRow], scroll: &mut ScrollState) {
    let mut summary = HString::<32>::new();
    let _ = write!(summary, "{} of {} slots", rows.len(), MAX_SEED_SLOTS);
    render_row_list(display, "Wallets", summary.as_str(), rows, scroll);
}

/// Vault screen: same list chrome as Wallets, rows are preimage entries
/// (label + commitment base58).
pub fn render_vault(display: &mut GuiDisplay<'_>, rows: &[WalletRow], scroll: &mut ScrollState) {
    let mut summary = HString::<32>::new();
    let _ = write!(
        summary,
        "{} of {} secrets",
        rows.len(),
        nockster_core::MAX_VAULT_ENTRIES
    );
    render_row_list(display, "Vault", summary.as_str(), rows, scroll);
}

fn render_row_list(
    display: &mut GuiDisplay<'_>,
    title: &str,
    summary: &str,
    rows: &[WalletRow],
    scroll: &mut ScrollState,
) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, title, palette::surface_high(), false);

    let subtle = MonoTextStyle::new(&FONT_6X10, palette::text());
    let _ = Text::with_alignment(
        summary,
        Point::new(8, header_height() + 16),
        subtle,
        Alignment::Left,
    )
    .draw(display);

    draw_wallet_table_header(display);
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

pub fn render_wallet_detail(display: &mut GuiDisplay<'_>, row: Option<&WalletRow>) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "Wallet", palette::surface_high(), false);

    let Some(row) = row else {
        let _ = draw_about_card(display, header_height() + 18, "Wallet", &["not found"]);
        return;
    };

    let mut slot_line = HString::<32>::new();
    let _ = write!(slot_line, "slot {}", row.index);
    let mut name_line = HString::<40>::new();
    let _ = name_line.push_str("nick ");
    if row.label.is_empty() {
        let _ = name_line.push_str("(unnamed)");
    } else {
        push_truncated(&mut name_line, row.label.as_str(), 26);
    }
    let status = if row.active {
        "status active"
    } else {
        "status standby"
    };
    let y = draw_about_card(
        display,
        header_height() + 8,
        "Slot",
        &[slot_line.as_str(), name_line.as_str(), status, "path m"],
    );
    let _ = draw_wallet_address_card(display, y, row.pkh.as_str());

    for hit in wallet_detail_buttons(row.index) {
        draw_wallet_detail_button(display, hit, false);
    }
}

pub fn render_wallet_delete_confirm(display: &mut GuiDisplay<'_>, row: Option<&WalletRow>) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "Delete", palette::surface_high(), false);

    let Some(row) = row else {
        let _ = draw_about_card(
            display,
            header_height() + 18,
            "Delete",
            &["wallet not found"],
        );
        return;
    };

    let mut slot_line = HString::<32>::new();
    let _ = write!(slot_line, "delete slot {}?", row.index);
    let name = if row.label.is_empty() {
        "(unnamed)"
    } else {
        row.label.as_str()
    };
    let _ = draw_about_card(
        display,
        header_height() + 24,
        "Confirm",
        &[
            slot_line.as_str(),
            name,
            "seed will be removed",
            "cannot be undone",
        ],
    );

    draw_hold_hint_on_background(display, HOLD_DELETE_HINT);
    for hit in wallet_delete_buttons(row.index) {
        draw_wallet_detail_button(display, hit, false);
    }
}

/// Vault entry detail: label, slot, and the full Tip5 commitment (the `%hax`
/// lock value), with the same Edit/Delete buttons as the wallet detail.
pub fn render_vault_detail(display: &mut GuiDisplay<'_>, row: Option<&WalletRow>) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "Secret", palette::surface_high(), false);

    let Some(row) = row else {
        let _ = draw_about_card(display, header_height() + 18, "Secret", &["not found"]);
        return;
    };

    let mut slot_line = HString::<32>::new();
    let _ = write!(slot_line, "slot {}", row.index);
    let mut name_line = HString::<40>::new();
    let _ = name_line.push_str("nick ");
    if row.label.is_empty() {
        let _ = name_line.push_str("(unnamed)");
    } else {
        push_truncated(&mut name_line, row.label.as_str(), 26);
    }
    let y = draw_about_card(
        display,
        header_height() + 8,
        "Entry",
        &[slot_line.as_str(), name_line.as_str(), "hax commitment:"],
    );
    let _ = draw_wallet_address_card(display, y, row.pkh.as_str());

    for hit in wallet_detail_buttons(row.index) {
        draw_wallet_detail_button(display, hit, false);
    }
}

pub fn render_vault_delete_confirm(display: &mut GuiDisplay<'_>, row: Option<&WalletRow>) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "Delete", palette::surface_high(), false);

    let Some(row) = row else {
        let _ = draw_about_card(
            display,
            header_height() + 18,
            "Delete",
            &["secret not found"],
        );
        return;
    };

    let mut slot_line = HString::<32>::new();
    let _ = write!(slot_line, "delete secret {}?", row.index);
    let name = if row.label.is_empty() {
        "(unnamed)"
    } else {
        row.label.as_str()
    };
    let _ = draw_about_card(
        display,
        header_height() + 24,
        "Confirm",
        &[
            slot_line.as_str(),
            name,
            "preimage will be erased",
            "cannot be undone",
        ],
    );

    draw_hold_hint_on_background(display, HOLD_DELETE_HINT);
    for hit in wallet_delete_buttons(row.index) {
        draw_wallet_detail_button(display, hit, false);
    }
}

pub fn button_from_point_wallet_detail(point: Point, slot: Option<u8>) -> Option<ButtonHit> {
    let slot = slot?;
    wallet_detail_buttons(slot)
        .into_iter()
        .find(|hit| within(hit, point, 6))
}

pub fn button_from_point_wallet_delete(point: Point, slot: Option<u8>) -> Option<ButtonHit> {
    let slot = slot?;
    wallet_delete_buttons(slot)
        .into_iter()
        .find(|hit| within(hit, point, 6))
}

pub fn draw_wallet_detail_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let label = match hit.button {
        Button::WalletEdit(_) => "Edit Nick",
        Button::WalletDelete(_) => "Delete",
        Button::WalletDeleteCancel(_) => "Cancel",
        Button::WalletDeleteConfirm(_) => "Delete",
        _ => "",
    };
    draw_text_button(display, hit, label, active);
}

pub fn draw_wallet_row_press(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    rows: &[WalletRow],
    scroll: &mut ScrollState,
    active: bool,
) {
    if active {
        let rect = Rectangle::new(hit.top_left, hit.size);
        let _ = rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(palette::keypad_active_light())
                    .stroke_width(2)
                    .build(),
            )
            .draw(display);
    } else {
        render_wallets_viewport(display, rows, scroll);
    }
}

fn wallet_detail_buttons(slot: u8) -> [ButtonHit; 2] {
    let margin = 8;
    let gap = 8;
    let top = SCREEN_HEIGHT as i32 - 50;
    let height = 42;
    let width = (SCREEN_WIDTH as i32 - margin * 2 - gap) / 2;
    [
        ButtonHit {
            button: Button::WalletEdit(slot),
            top_left: Point::new(margin, top),
            size: Size::new(width as u32, height as u32),
        },
        ButtonHit {
            button: Button::WalletDelete(slot),
            top_left: Point::new(margin + width + gap, top),
            size: Size::new(width as u32, height as u32),
        },
    ]
}

fn wallet_delete_buttons(slot: u8) -> [ButtonHit; 2] {
    let margin = 8;
    let gap = 8;
    let top = SCREEN_HEIGHT as i32 - 50;
    let height = 42;
    let width = (SCREEN_WIDTH as i32 - margin * 2 - gap) / 2;
    [
        ButtonHit {
            button: Button::WalletDeleteCancel(slot),
            top_left: Point::new(margin, top),
            size: Size::new(width as u32, height as u32),
        },
        ButtonHit {
            button: Button::WalletDeleteConfirm(slot),
            top_left: Point::new(margin + width + gap, top),
            size: Size::new(width as u32, height as u32),
        },
    ]
}

fn draw_wallet_address_card(display: &mut GuiDisplay<'_>, top: i32, pkh: &str) -> i32 {
    if pkh.is_empty() {
        return draw_about_card(display, top, "Address", &["address unavailable"]);
    }

    let mut line0 = HString::<32>::new();
    let mut line1 = HString::<32>::new();
    let mut line2 = HString::<32>::new();
    let count = wrapped_address_lines(pkh, &mut line0, &mut line1, &mut line2);
    match count {
        0 => draw_about_card(display, top, "Address", &["address unavailable"]),
        1 => draw_about_card(display, top, "Address", &[line0.as_str()]),
        2 => draw_about_card(display, top, "Address", &[line0.as_str(), line1.as_str()]),
        _ => draw_about_card(
            display,
            top,
            "Address",
            &[line0.as_str(), line1.as_str(), line2.as_str()],
        ),
    }
}

fn wrapped_address_lines(
    value: &str,
    line0: &mut HString<32>,
    line1: &mut HString<32>,
    line2: &mut HString<32>,
) -> usize {
    let bytes = value.as_bytes();
    let mut start = 0usize;
    let mut count = 0usize;
    for line in [line0, line1, line2] {
        if start >= bytes.len() {
            break;
        }
        let end = (start + 23).min(bytes.len());
        let _ = line.push_str(core::str::from_utf8(&bytes[start..end]).unwrap_or(""));
        start = end;
        count += 1;
    }
    count
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
        let primary = MonoTextStyle::new(&FONT_8X13, palette::text());
        let subtle = MonoTextStyle::new(&FONT_6X10, palette::text_subtle());
        let active_style = MonoTextStyle::new(&FONT_6X10, palette::accent_warning());
        if self.rows.is_empty() {
            let empty = Rectangle::new(
                Point::new(2, 10),
                Size::new(wallets_viewport().size.width.saturating_sub(4), 56),
            );
            let _ = empty
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(palette::surface_low())
                        .stroke_color(palette::divider())
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
                    .stroke_color(palette::panel_highlight())
                    .stroke_width(1)
                    .build(),
            )
            .draw(target);
            let _ = Rectangle::new(Point::new(9, 24), Size::new(3, 28))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(palette::panel_highlight())
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
                            palette::surface_low()
                        } else {
                            palette::background()
                        })
                        .stroke_color(palette::divider())
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
                    .stroke_color(palette::panel_highlight())
                    .stroke_width(1)
                    .build(),
            )
            .draw(target);
            if row.active {
                let _ = Rectangle::new(Point::new(3, y + 7), Size::new(3, 26))
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(palette::keypad_active_light())
                            .stroke_width(0)
                            .build(),
                    )
                    .draw(target);
            }

            let slot_badge = Rectangle::new(Point::new(8, y + 7), Size::new(18, 18));
            let _ = slot_badge
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(palette::panel_shadow())
                        .stroke_color(palette::divider())
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
                    .stroke_color(palette::divider())
                    .stroke_width(1)
                    .build(),
            )
            .draw(target);
            y += WALLET_ROW_HEIGHT;
        }
    }
}

const WALLET_ROW_HEIGHT: i32 = 42;

fn within(hit: &ButtonHit, point: Point, slack: i32) -> bool {
    let left = hit.top_left.x - slack;
    let right = hit.top_left.x + hit.size.width as i32 + slack;
    let top = hit.top_left.y - slack;
    let bottom = hit.top_left.y + hit.size.height as i32 + slack;
    point.x >= left && point.x < right && point.y >= top && point.y < bottom
}

fn draw_wallet_table_header(display: &mut GuiDisplay<'_>) {
    let style = MonoTextStyle::new(&FONT_6X10, palette::text());
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
            .stroke_color(palette::divider())
            .stroke_width(1)
            .build(),
    )
    .draw(display);
}

fn draw_theme_marker(display: &mut GuiDisplay<'_>, hit: ButtonHit, theme: Theme, active: bool) {
    let selected = palette::current_theme() == theme;
    let marker_color = if selected || active {
        palette::keypad_active_light()
    } else {
        palette::panel_highlight()
    };
    let marker = Rectangle::new(
        Point::new(hit.top_left.x + 6, hit.top_left.y + 8),
        Size::new(2, hit.size.height.saturating_sub(16)),
    );
    let _ = marker
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(marker_color)
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let colors = theme.palette();
    let swatch_x = hit.top_left.x + hit.size.width as i32 - 36;
    let swatch_y = hit.top_left.y + 9;
    let swatch = Rectangle::new(Point::new(swatch_x, swatch_y), Size::new(24, 16));
    let _ = swatch
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(colors.background)
                .stroke_color(if selected {
                    colors.text
                } else {
                    colors.divider
                })
                .stroke_width(1)
                .build(),
        )
        .draw(display);
    let _ = Rectangle::new(Point::new(swatch_x + 3, swatch_y + 3), Size::new(6, 10))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(colors.accent_primary_light)
                .stroke_width(0)
                .build(),
        )
        .draw(display);
    let _ = Rectangle::new(Point::new(swatch_x + 10, swatch_y + 3), Size::new(5, 10))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(colors.accent_secondary_light)
                .stroke_width(0)
                .build(),
        )
        .draw(display);
    let _ = Rectangle::new(Point::new(swatch_x + 16, swatch_y + 3), Size::new(5, 10))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(colors.text)
                .stroke_width(0)
                .build(),
        )
        .draw(display);
}

fn draw_menu_text_button<D>(display: &mut D, hit: ButtonHit, label: &str, active: bool)
where
    D: DrawTarget<Color = Rgb565>,
{
    let press_offset = if active { 1 } else { 0 };
    let shadow = Rectangle::new(
        Point::new(hit.top_left.x + 2, hit.top_left.y + 2),
        Size::new(
            hit.size.width.saturating_sub(press_offset as u32),
            hit.size.height.saturating_sub(press_offset as u32),
        ),
    );
    let _ = shadow
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(if active {
                    palette::background()
                } else {
                    palette::panel_shadow()
                })
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let face_top_left = Point::new(hit.top_left.x + press_offset, hit.top_left.y + press_offset);
    let face_size = Size::new(
        hit.size.width.saturating_sub(press_offset as u32),
        hit.size.height.saturating_sub(press_offset as u32),
    );
    let face = Rectangle::new(face_top_left, face_size);
    let _ = face
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(if active {
                    palette::keypad_active()
                } else {
                    palette::surface_low()
                })
                .stroke_color(if active {
                    palette::text()
                } else {
                    palette::divider()
                })
                .stroke_width(if active { 2 } else { 1 })
                .build(),
        )
        .draw(display);

    let right = face_top_left.x + face_size.width as i32 - 2;
    let bottom = face_top_left.y + face_size.height as i32 - 2;
    let _ = Line::new(
        Point::new(face_top_left.x + 1, face_top_left.y + 1),
        Point::new(right, face_top_left.y + 1),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(if active {
                palette::keypad_active_light()
            } else {
                palette::panel_highlight()
            })
            .stroke_width(1)
            .build(),
    )
    .draw(display);
    let _ = Line::new(
        Point::new(face_top_left.x + 1, bottom),
        Point::new(right, bottom),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(if active {
                palette::keypad_active_dark()
            } else {
                palette::panel_shadow()
            })
            .stroke_width(1)
            .build(),
    )
    .draw(display);

    if active {
        let accent = Rectangle::new(
            Point::new(face_top_left.x + 4, face_top_left.y + 5),
            Size::new(4, face_size.height.saturating_sub(10)),
        );
        let _ = accent
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(palette::text())
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
    }

    if !label.is_empty() {
        let style = MonoTextStyle::new(&FONT_10X20, palette::text());
        let center = Point::new(
            face_top_left.x + face_size.width as i32 / 2,
            face_top_left.y + face_size.height as i32 / 2,
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
}

fn draw_menu_affordance<D>(display: &mut D, hit: ButtonHit, active: bool)
where
    D: DrawTarget<Color = Rgb565>,
{
    let rail_color = if active {
        palette::text()
    } else {
        palette::panel_highlight()
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

    let x = hit.top_left.x + hit.size.width as i32 - 18;
    let y = hit.top_left.y + hit.size.height as i32 / 2;
    let style = PrimitiveStyleBuilder::new()
        .stroke_color(if active {
            palette::text()
        } else {
            palette::text_subtle()
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
