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
use super::render::render_header;
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

fn menu_order() -> [MenuItem; 7] {
    [
        MenuItem::Wallets,
        MenuItem::AddSeed,
        MenuItem::Theme,
        MenuItem::Calibrate,
        MenuItem::Diagnostics,
        MenuItem::About,
        MenuItem::Back,
    ]
}

fn menu_label(item: MenuItem) -> &'static str {
    match item {
        MenuItem::Wallets => "Wallets",
        MenuItem::AddSeed => "Add Seed",
        MenuItem::Theme => "Theme",
        MenuItem::About => "About",
        MenuItem::Calibrate => "Calibrate",
        MenuItem::Diagnostics => "Diagnostics",
        MenuItem::Back => "Back",
    }
}

fn menu_button(index: usize, item: MenuItem) -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = MENU_CONTENT_TOP + index as i32 * (MENU_BUTTON_HEIGHT + MENU_BUTTON_GAP);
    ButtonHit {
        button: Button::Menu(item),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, MENU_BUTTON_HEIGHT as u32),
    }
}

fn menu_buttons() -> [ButtonHit; 7] {
    let order = menu_order();
    [
        menu_button(0, order[0]),
        menu_button(1, order[1]),
        menu_button(2, order[2]),
        menu_button(3, order[3]),
        menu_button(4, order[4]),
        menu_button(5, order[5]),
        menu_button(6, order[6]),
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
    render_header(display, "Settings", palette::surface_high());
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

fn theme_back_button(index: usize) -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = header_height() + 3 + index as i32 * (THEME_BUTTON_HEIGHT + THEME_BUTTON_GAP);
    ButtonHit {
        button: Button::Menu(MenuItem::Back),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, THEME_BUTTON_HEIGHT as u32),
    }
}

pub fn theme_buttons() -> [ButtonHit; 7] {
    [
        theme_button(0, palette::THEMES[0]),
        theme_button(1, palette::THEMES[1]),
        theme_button(2, palette::THEMES[2]),
        theme_button(3, palette::THEMES[3]),
        theme_button(4, palette::THEMES[4]),
        theme_button(5, palette::THEMES[5]),
        theme_back_button(6),
    ]
}

pub fn render_themes(display: &mut GuiDisplay<'_>) {
    let _ = display.clear(palette::background());
    render_header(display, "Theme", palette::surface_high());
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
        Button::Menu(MenuItem::Back) => {
            draw_text_button(display, hit, "Back", active);
            draw_menu_affordance(display, hit, active);
        }
        _ => {}
    }
}

pub fn button_from_point_themes(point: Point) -> Option<ButtonHit> {
    theme_buttons()
        .into_iter()
        .find(|hit| within(hit, point, 6))
}

fn about_back_button() -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * MENU_MARGIN).max(80);
    let y = SCREEN_HEIGHT as i32 - MENU_BUTTON_HEIGHT - 8;
    ButtonHit {
        button: Button::Menu(MenuItem::Back),
        top_left: Point::new(MENU_MARGIN, y),
        size: Size::new(width as u32, MENU_BUTTON_HEIGHT as u32),
    }
}

pub fn render_about(display: &mut GuiDisplay<'_>, info: &AboutInfo) {
    let _ = display.clear(palette::background());
    render_header(display, "About", palette::surface_high());

    let text = MonoTextStyle::new(&FONT_8X13, palette::text());
    let subtle = MonoTextStyle::new(&FONT_6X10, palette::text_subtle());
    let left = 10;
    let mut y = header_height() + 24;
    let _ = Text::new("Nockster FW", Point::new(left, y), text).draw(display);
    y += 17;

    let mut line = HString::<40>::new();
    let _ = write!(
        line,
        "firmware {}.{} r{}",
        info.fw_major, info.fw_minor, info.release_version
    );
    draw_about_line(display, left, &mut y, line.as_str(), subtle);

    line.clear();
    let _ = write!(
        line,
        "profile {} proto {}",
        info.build.build_profile.as_str(),
        info.build.protocol_v
    );
    draw_about_line(display, left, &mut y, line.as_str(), subtle);

    line.clear();
    let _ = line.push_str("commit ");
    push_short_ascii(&mut line, info.build.git_commit.as_str(), 10);
    if info.build.git_dirty {
        let _ = line.push('*');
    }
    draw_about_line(display, left, &mut y, line.as_str(), subtle);

    line.clear();
    let _ = line.push_str("tx-types ");
    push_short_ascii(&mut line, info.build.tx_types_rev.as_str(), 10);
    draw_about_line(display, left, &mut y, line.as_str(), subtle);

    line.clear();
    let _ = write!(line, "theme {}", palette::current_theme().name());
    draw_about_line(display, left, &mut y, line.as_str(), subtle);

    y += 5;
    draw_about_line(display, left, &mut y, "trust root", subtle);
    if info.trust.configured {
        line.clear();
        let _ = line.push_str("sha256 ");
        push_hex_bytes(&mut line, &info.trust.pubkey_sha256[..8]);
        draw_about_line(display, left, &mut y, line.as_str(), subtle);

        line.clear();
        let _ = line.push_str("       ");
        push_hex_bytes(&mut line, &info.trust.pubkey_sha256[24..]);
        draw_about_line(display, left, &mut y, line.as_str(), subtle);
    } else {
        draw_about_line(display, left, &mut y, "not configured", subtle);
    }

    draw_about_button(display, false);
}

fn draw_about_line(
    display: &mut GuiDisplay<'_>,
    left: i32,
    y: &mut i32,
    line: &str,
    style: MonoTextStyle<'_, Rgb565>,
) {
    let _ = Text::new(line, Point::new(left, *y), style).draw(display);
    *y += 13;
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

pub fn draw_about_button(display: &mut GuiDisplay<'_>, active: bool) {
    let back = about_back_button();
    draw_text_button(display, back, "Back", active);
    draw_menu_affordance(display, back, active);
}

pub fn button_from_point_about(point: Point) -> Option<ButtonHit> {
    let back = about_back_button();
    within(&back, point, 8).then_some(back)
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
    let _ = display.clear(palette::background());
    render_header(display, "Wallets", palette::surface_high());

    let subtle = MonoTextStyle::new(&FONT_6X10, palette::text());
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
    let shadow = Rectangle::new(
        Point::new(hit.top_left.x + 2, hit.top_left.y + 2),
        Size::new(hit.size.width, hit.size.height),
    );
    let _ = shadow
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::panel_shadow())
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let face = Rectangle::new(hit.top_left, hit.size);
    let _ = face
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(if active {
                    palette::keypad_active()
                } else {
                    palette::surface_low()
                })
                .stroke_color(if active {
                    palette::keypad_active_light()
                } else {
                    palette::divider()
                })
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    let right = hit.top_left.x + hit.size.width as i32 - 2;
    let bottom = hit.top_left.y + hit.size.height as i32 - 2;
    let _ = Line::new(
        Point::new(hit.top_left.x + 1, hit.top_left.y + 1),
        Point::new(right, hit.top_left.y + 1),
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
        Point::new(hit.top_left.x + 1, bottom),
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

    if !label.is_empty() {
        let style = MonoTextStyle::new(&FONT_10X20, palette::text());
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

    if matches!(hit.button, Button::Menu(MenuItem::Back)) {
        return;
    }

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
