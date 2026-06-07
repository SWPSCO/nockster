use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10, FONT_8X13};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use esp_hal::time::{Duration, Instant};
use heapless::String as HString;
use nockster_core::MAX_SEED_LABEL_LEN;

use super::constants::*;
use super::layout::header_height;
use super::render::render_header;
use super::state::{Button, ButtonHit};
use super::GuiDisplay;

const MULTITAP_TIMEOUT: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelEntryContext {
    WalletMenu,
    AddedSeed,
    FirstSeed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelButton {
    Key(u8),
    Cancel,
    Backspace,
    Save,
}

#[derive(Clone, Debug)]
pub enum LabelInteraction {
    Saved {
        slot: u8,
        label: HString<MAX_SEED_LABEL_LEN>,
        context: LabelEntryContext,
    },
    Cancelled {
        context: LabelEntryContext,
    },
}

#[derive(Clone)]
pub struct LabelEntryState {
    slot: u8,
    context: LabelEntryContext,
    label: HString<MAX_SEED_LABEL_LEN>,
    last_key: Option<u8>,
    last_key_at: Option<Instant>,
    last_key_index: usize,
}

impl LabelEntryState {
    pub fn new() -> Self {
        Self {
            slot: 0,
            context: LabelEntryContext::WalletMenu,
            label: HString::new(),
            last_key: None,
            last_key_at: None,
            last_key_index: 0,
        }
    }

    pub fn begin(&mut self, slot: u8, current: &str, context: LabelEntryContext) {
        self.slot = slot;
        self.context = context;
        self.label.clear();
        let take = current.len().min(MAX_SEED_LABEL_LEN);
        let _ = self.label.push_str(&current[..take]);
        self.clear_multitap();
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    pub fn context(&self) -> LabelEntryContext {
        self.context
    }

    pub fn label(&self) -> &HString<MAX_SEED_LABEL_LEN> {
        &self.label
    }

    pub fn push_key(&mut self, digit: u8, now: Instant) -> bool {
        let Some(chars) = key_chars(digit) else {
            return false;
        };
        if chars.is_empty() {
            return false;
        }

        let cycling = self.last_key == Some(digit)
            && self
                .last_key_at
                .map(|last| now - last <= MULTITAP_TIMEOUT)
                .unwrap_or(false)
            && !self.label.is_empty();

        if cycling {
            let _ = self.label.pop();
            self.last_key_index = (self.last_key_index + 1) % chars.len();
        } else {
            if self.label.len() >= MAX_SEED_LABEL_LEN {
                self.clear_multitap();
                return false;
            }
            self.last_key_index = 0;
        }

        let ch = chars[self.last_key_index] as char;
        if self.label.push(ch).is_err() {
            self.clear_multitap();
            return false;
        }
        self.last_key = Some(digit);
        self.last_key_at = Some(now);
        true
    }

    pub fn backspace(&mut self) -> bool {
        self.clear_multitap();
        self.label.pop().is_some()
    }

    pub fn clear_multitap(&mut self) {
        self.last_key = None;
        self.last_key_at = None;
        self.last_key_index = 0;
    }
}

impl Default for LabelEntryState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_label_entry(display: &mut GuiDisplay<'_>, state: &LabelEntryState) {
    let _ = display.clear(COLOR_BACKGROUND);
    render_header(display, label_header(state.context), COLOR_SURFACE_HIGH);
    draw_label_preview(display, state);
    for hit in label_buttons() {
        draw_label_button(display, hit, state, false);
    }
}

pub fn button_from_point_label_entry(point: Point) -> Option<ButtonHit> {
    label_buttons()
        .into_iter()
        .find(|hit| within_hit(hit, point, 5))
}

pub fn draw_label_button(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    state: &LabelEntryState,
    active: bool,
) {
    draw_button_frame(display, hit, active);

    let Button::Label(button) = hit.button else {
        return;
    };

    let center_x = hit.top_left.x + hit.size.width as i32 / 2;
    let center_y = hit.top_left.y + hit.size.height as i32 / 2;
    match button {
        LabelButton::Key(digit) => draw_key_label(display, center_x, center_y, digit),
        LabelButton::Cancel => {
            let label = match state.context {
                LabelEntryContext::WalletMenu => "BACK",
                LabelEntryContext::AddedSeed | LabelEntryContext::FirstSeed => "SKIP",
            };
            draw_action_label(display, center_x, center_y, label);
        }
        LabelButton::Backspace => draw_action_label(display, center_x, center_y, "DEL"),
        LabelButton::Save => draw_action_label(display, center_x, center_y, "SAVE"),
    }
}

fn draw_label_preview(display: &mut GuiDisplay<'_>, state: &LabelEntryState) {
    let top = header_height() + 10;
    let shadow = Rectangle::new(
        Point::new(9, top + 1),
        Size::new((SCREEN_WIDTH - 17) as u32, 42),
    );
    let _ = shadow
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_PANEL_SHADOW)
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let panel = Rectangle::new(
        Point::new(8, top),
        Size::new((SCREEN_WIDTH - 16) as u32, 42),
    );
    let _ = panel
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_SURFACE_LOW)
                .stroke_color(COLOR_DIVIDER)
                .stroke_width(1)
                .build(),
        )
        .draw(display);

    let _ = Line::new(
        Point::new(10, top + 2),
        Point::new(SCREEN_WIDTH as i32 - 11, top + 2),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(COLOR_PANEL_HIGHLIGHT)
            .stroke_width(1)
            .build(),
    )
    .draw(display);
    let _ = Line::new(
        Point::new(10, top + 40),
        Point::new(SCREEN_WIDTH as i32 - 11, top + 40),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(COLOR_PANEL_SHADOW)
            .stroke_width(1)
            .build(),
    )
    .draw(display);
    let accent = Rectangle::new(Point::new(11, top + 5), Size::new(3, 26));
    let _ = accent
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_PANEL_HIGHLIGHT)
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let meta_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
    let mut slot_line = HString::<24>::new();
    let _ = core::fmt::write(&mut slot_line, format_args!("slot {}", state.slot()));
    let _ = Text::with_alignment(
        slot_line.as_str(),
        Point::new(18, top + 12),
        meta_style,
        Alignment::Left,
    )
    .draw(display);

    let value = preview_label(state.label().as_str());
    let value_style = MonoTextStyle::new(
        &FONT_10X20,
        if state.label().is_empty() {
            COLOR_TEXT_SUBTLE
        } else {
            COLOR_TEXT
        },
    );
    let value_x = 18;
    let _ = Text::with_alignment(
        value.as_str(),
        Point::new(value_x, top + 38),
        value_style,
        Alignment::Left,
    )
    .draw(display);

    if !state.label().is_empty() {
        let cursor_x = value_x
            + (value.len() as i32 * FONT_10X20.character_size.width as i32)
                .min(SCREEN_WIDTH as i32 - value_x - 22);
        let cursor = Rectangle::new(Point::new(cursor_x + 2, top + 21), Size::new(2, 17));
        let _ = cursor
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_TEXT)
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
    }
}

fn label_header(context: LabelEntryContext) -> &'static str {
    match context {
        LabelEntryContext::WalletMenu => "Rename",
        LabelEntryContext::AddedSeed | LabelEntryContext::FirstSeed => "Name Wallet",
    }
}

fn preview_label(label: &str) -> HString<18> {
    let mut out = HString::<18>::new();
    if label.is_empty() {
        let _ = out.push_str("(unnamed)");
        return out;
    }
    push_truncated(&mut out, label, 14);
    out
}

fn label_buttons() -> [ButtonHit; 12] {
    let margin = 6i32;
    let gap = 5i32;
    let button_w = ((SCREEN_WIDTH as i32 - margin * 2 - gap * 2) / 3).max(36);
    let button_h = 35i32;
    let top = header_height() + 66;
    let row_gap = 6i32;
    let layout = [
        [
            LabelButton::Key(2),
            LabelButton::Key(3),
            LabelButton::Key(4),
        ],
        [
            LabelButton::Key(5),
            LabelButton::Key(6),
            LabelButton::Key(7),
        ],
        [
            LabelButton::Key(8),
            LabelButton::Key(9),
            LabelButton::Key(0),
        ],
        [
            LabelButton::Cancel,
            LabelButton::Backspace,
            LabelButton::Save,
        ],
    ];

    let mut out = [ButtonHit {
        button: Button::Label(LabelButton::Cancel),
        top_left: Point::zero(),
        size: Size::zero(),
    }; 12];
    let mut idx = 0;
    for row in 0..4 {
        for col in 0..3 {
            out[idx] = ButtonHit {
                button: Button::Label(layout[row][col]),
                top_left: Point::new(
                    margin + col as i32 * (button_w + gap),
                    top + row as i32 * (button_h + row_gap),
                ),
                size: Size::new(button_w as u32, button_h as u32),
            };
            idx += 1;
        }
    }
    out
}

fn draw_key_label(display: &mut GuiDisplay<'_>, x: i32, y: i32, digit: u8) {
    let digit_style = MonoTextStyle::new(&FONT_8X13, COLOR_TEXT);
    let letters_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
    let mut digit_buf = [0u8; 4];
    let digit_ch = char::from(b'0' + digit);
    let digit_str = digit_ch.encode_utf8(&mut digit_buf);
    let _ = Text::with_alignment(
        digit_str,
        Point::new(x, y - 2),
        digit_style,
        Alignment::Center,
    )
    .draw(display);
    let _ = Text::with_alignment(
        key_label(digit),
        Point::new(x, y + 12),
        letters_style,
        Alignment::Center,
    )
    .draw(display);
}

fn draw_action_label(display: &mut GuiDisplay<'_>, x: i32, y: i32, label: &str) {
    let style = MonoTextStyle::new(&FONT_8X13, COLOR_TEXT);
    let _ =
        Text::with_alignment(label, Point::new(x, y + 5), style, Alignment::Center).draw(display);
}

fn draw_button_frame(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let (base, border): (Rgb565, Rgb565) = if active {
        (COLOR_KEYPAD_ACTIVE, COLOR_KEYPAD_ACTIVE_LIGHT)
    } else {
        (COLOR_SURFACE_LOW, COLOR_DIVIDER)
    };
    if hit.size.width > 8 && hit.size.height > 8 {
        let shadow = Rectangle::new(
            Point::new(hit.top_left.x + 1, hit.top_left.y + 1),
            Size::new(
                hit.size.width.saturating_sub(1),
                hit.size.height.saturating_sub(1),
            ),
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
    if hit.size.width > 8 && hit.size.height > 8 {
        let right = hit.top_left.x + hit.size.width as i32 - 2;
        let bottom = hit.top_left.y + hit.size.height as i32 - 2;
        let inset_left = hit.top_left.x + 3;
        let inset_right = hit.top_left.x + hit.size.width as i32 - 4;
        let inset_top = hit.top_left.y + 3;
        let inset_bottom = hit.top_left.y + hit.size.height as i32 - 4;
        let _ = Line::new(
            Point::new(hit.top_left.x + 1, hit.top_left.y + 1),
            Point::new(right, hit.top_left.y + 1),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(if active {
                    COLOR_KEYPAD_ACTIVE_LIGHT
                } else {
                    COLOR_SURFACE_HIGH
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
                    COLOR_KEYPAD_ACTIVE_DARK
                } else {
                    COLOR_BACKGROUND
                })
                .stroke_width(1)
                .build(),
        )
        .draw(display);
        let _ = Line::new(
            Point::new(inset_left, inset_top),
            Point::new(inset_right, inset_top),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(if active {
                    COLOR_TEXT
                } else {
                    COLOR_PANEL_HIGHLIGHT
                })
                .stroke_width(1)
                .build(),
        )
        .draw(display);
        let _ = Line::new(
            Point::new(inset_left, inset_bottom),
            Point::new(inset_right, inset_bottom),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(if active {
                    COLOR_KEYPAD_ACTIVE_DARK
                } else {
                    COLOR_BACKGROUND
                })
                .stroke_width(1)
                .build(),
        )
        .draw(display);
        let notch = Rectangle::new(
            Point::new(hit.top_left.x + 3, hit.top_left.y + 4),
            Size::new(2, 5),
        );
        let _ = notch
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(if active {
                        COLOR_TEXT
                    } else {
                        COLOR_PANEL_HIGHLIGHT
                    })
                    .stroke_width(0)
                    .build(),
            )
            .draw(display);
        if active && hit.size.width > 8 {
            let bar = Rectangle::new(
                Point::new(
                    hit.top_left.x + 4,
                    hit.top_left.y + hit.size.height as i32 - 4,
                ),
                Size::new(hit.size.width.saturating_sub(8), 2),
            );
            let _ = bar
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLOR_TEXT)
                        .stroke_width(0)
                        .build(),
                )
                .draw(display);
        }
    }
}

fn key_chars(digit: u8) -> Option<&'static [u8]> {
    match digit {
        0 => Some(b" 0"),
        2 => Some(b"abc2"),
        3 => Some(b"def3"),
        4 => Some(b"ghi4"),
        5 => Some(b"jkl5"),
        6 => Some(b"mno6"),
        7 => Some(b"pqrs7"),
        8 => Some(b"tuv8"),
        9 => Some(b"wxyz9"),
        _ => None,
    }
}

fn key_label(digit: u8) -> &'static str {
    match digit {
        0 => "space",
        2 => "ABC",
        3 => "DEF",
        4 => "GHI",
        5 => "JKL",
        6 => "MNO",
        7 => "PQRS",
        8 => "TUV",
        9 => "WXYZ",
        _ => "",
    }
}

fn within_hit(hit: &ButtonHit, point: Point, slack: i32) -> bool {
    let left = hit.top_left.x - slack;
    let right = hit.top_left.x + hit.size.width as i32 + slack;
    let top = hit.top_left.y - slack;
    let bottom = hit.top_left.y + hit.size.height as i32 + slack;
    point.x >= left && point.x < right && point.y >= top && point.y < bottom
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
