use core::cmp::max;
use core::fmt::Write;

use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10, FONT_8X13};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use heapless::{String as HString, Vec as HVec};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::constants::*;
use super::layout::header_height;
use super::render::render_header;
use super::state::{Button, ButtonHit, GuiMode};
use super::GuiDisplay;

const WORDLIST: &str = include_str!("bip39_english.txt");

pub const MAX_SEED_WORDS: usize = 24;
const MAX_WORD_LEN: usize = 12;
const MAX_DIGITS: usize = 8;
const SUGGESTION_CAP: usize = 6;
const KEYPAD_MARGIN: i32 = 3;

pub type SeedWord = HString<MAX_WORD_LEN>;
pub type SeedPhrase = HVec<SeedWord, MAX_SEED_WORDS>;

#[derive(Clone, Debug)]
pub enum SeedInteraction {
    EnterSeedRequested,
    WordCommitted,
    WordRemoved,
    EntryCompleted(SeedPhrase),
    EntryCancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedButton {
    EnterSeed,
    GenerateSeed,
    Key(u8),
    Backspace,
    NextSuggestion,
    CommitWord,
    Finish,
    Cancel,
}

#[derive(Clone)]
pub struct SeedEntryState {
    digits: HVec<u8, MAX_DIGITS>,
    suggestions: HVec<&'static str, SUGGESTION_CAP>,
    suggestion_index: usize,
    total_matches: usize,
    words: SeedPhrase,
    /// True when `words` was produced on-device (Generate) rather than typed in.
    /// Drives the confirm-screen copy and the Cancel destination.
    generated: bool,
}

impl SeedEntryState {
    pub const fn new() -> Self {
        Self {
            digits: HVec::new(),
            suggestions: HVec::new(),
            suggestion_index: 0,
            total_matches: 0,
            words: HVec::new(),
            generated: false,
        }
    }

    pub fn reset(&mut self) {
        self.digits.clear();
        self.suggestions.clear();
        self.suggestion_index = 0;
        self.total_matches = 0;
        self.words.clear();
        self.generated = false;
    }

    pub fn is_generated(&self) -> bool {
        self.generated
    }

    /// Generate a fresh 24-word BIP-39 mnemonic on-device from the hardware RNG
    /// and load it as the committed word list. Returns false if entropy is
    /// unavailable or word lookup fails (state left reset in that case).
    pub fn load_generated(&mut self) -> bool {
        let mut entropy = [0u8; 32];
        if getrandom::getrandom(&mut entropy).is_err() {
            entropy.zeroize();
            return false;
        }
        let phrase = mnemonic_from_entropy(&entropy);
        entropy.zeroize();
        match phrase {
            Some(words) => {
                self.reset();
                self.words = words;
                self.generated = true;
                true
            }
            None => false,
        }
    }

    pub fn push_digit(&mut self, digit: u8) -> bool {
        if digit < 2 || digit > 9 || self.digits.len() >= MAX_DIGITS {
            return false;
        }
        if self.digits.push(digit).is_ok() {
            self.refresh_suggestions();
            true
        } else {
            false
        }
    }

    pub fn backspace(&mut self) -> Option<SeedWord> {
        if let Some(_) = self.digits.pop() {
            self.refresh_suggestions();
            None
        } else {
            self.words.pop()
        }
    }

    pub fn next_suggestion(&mut self) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        self.suggestion_index = (self.suggestion_index + 1) % self.suggestions.len();
        true
    }

    pub fn commit_current(&mut self) -> Option<SeedWord> {
        let word = self.current_suggestion()?;
        if self.words.len() >= MAX_SEED_WORDS {
            return None;
        }
        let mut committed = SeedWord::new();
        let _ = committed.push_str(word);
        if self.words.push(committed.clone()).is_err() {
            return None;
        }
        self.digits.clear();
        self.suggestions.clear();
        self.suggestion_index = 0;
        self.total_matches = 0;
        Some(committed)
    }

    pub fn finish(&self) -> Option<SeedPhrase> {
        if self.words.len() != MAX_SEED_WORDS || !self.digits.is_empty() {
            None
        } else {
            Some(self.words.clone())
        }
    }

    pub fn suggestion_position(&self) -> Option<(usize, usize)> {
        if self.suggestions.is_empty() || self.total_matches == 0 {
            None
        } else {
            Some((self.suggestion_index + 1, self.total_matches))
        }
    }

    pub fn current_suggestion(&self) -> Option<&'static str> {
        if self.suggestions.is_empty() {
            None
        } else {
            self.suggestions.get(self.suggestion_index).copied()
        }
    }

    fn refresh_suggestions(&mut self) {
        self.suggestions.clear();
        self.total_matches = 0;
        if self.digits.is_empty() {
            self.suggestion_index = 0;
            return;
        }
        for word in WORDLIST.lines() {
            if word_matches_digits(word, &self.digits) {
                self.total_matches = self.total_matches.saturating_add(1);
                if self.suggestions.len() < SUGGESTION_CAP {
                    let _ = self.suggestions.push(word);
                }
            }
        }
        if self.suggestions.is_empty() {
            self.suggestion_index = 0;
        } else if self.suggestion_index >= self.suggestions.len() {
            self.suggestion_index = 0;
        }
    }
}

pub fn render_seed_setup(display: &mut GuiDisplay<'_>, title: &str, show_back: bool) {
    let _ = display.clear(COLOR_BACKGROUND);
    render_header(display, title, COLOR_SURFACE_HIGH);

    let mut body = HString::<96>::new();
    let _ = body
        .push_str("Generate a new seed on-device, or enter an existing one (here, web, or CLI).");
    let text_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
    let mut y = header_height() + 24;
    // Estimate character width for FONT_6X10 is 6 pixels
    let max_chars = (SCREEN_WIDTH as usize - 32) / 6;
    for line in split_text(body.as_str(), max_chars) {
        let _ = Text::with_alignment(
            line,
            Point::new((SCREEN_WIDTH / 2) as i32, y),
            text_style,
            Alignment::Center,
        )
        .draw(display);
        y += (FONT_6X10.character_size.height as i32) + 4;
    }

    for button in setup_buttons(show_back) {
        draw_seed_button(display, GuiMode::SeedFirstBoot, button, None, false);
    }
}

pub fn render_seed_entry(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let _ = display.clear(COLOR_BACKGROUND);

    // Build header text showing current word
    let mut header_text = HString::<48>::new();
    if !state.digits.is_empty() {
        if let Some(suggestion) = state.current_suggestion() {
            // Show typed prefix
            for (idx, ch) in suggestion.chars().enumerate() {
                if idx < state.digits.len() {
                    let _ = header_text.push(ch);
                }
            }
            // Add suggestion count if available
            if let Some((pos, total)) = state.suggestion_position() {
                let _ = write!(header_text, " ({}/{})", pos, total);
            }
        } else {
            let _ = header_text.push_str("No match");
        }
    } else if state.words.is_empty() {
        let _ = write!(header_text, "Word 1/{}", MAX_SEED_WORDS);
    } else if state.words.len() >= MAX_SEED_WORDS {
        let _ = write!(header_text, "Seed {}/{}", MAX_SEED_WORDS, MAX_SEED_WORDS);
    } else {
        let _ = write!(
            header_text,
            "Word {}/{}",
            state.words.len() + 1,
            MAX_SEED_WORDS
        );
    }

    render_header(display, header_text.as_str(), COLOR_SURFACE_HIGH);
    draw_keypad(display, state);
    draw_corner_buttons(display, state);
}

pub fn button_from_point_seed_setup(point: Point, show_back: bool) -> Option<ButtonHit> {
    setup_buttons(show_back)
        .into_iter()
        .find(|hit| within_hit(hit, point, 4))
}

pub fn button_from_point_seed_entry(point: Point) -> Option<ButtonHit> {
    // Check corner buttons first
    for button in &corner_buttons() {
        if within_hit(button, point, 4) {
            return Some(*button);
        }
    }

    // Check keypad
    let geo = keypad_geometry();
    for row in 0..keypad_layout().len() {
        for col in 0..keypad_layout()[row].len() {
            let hit = keypad_button_hit(row, col, &geo);
            if within_hit(&hit, point, 10) {
                return Some(hit);
            }
        }
    }
    None
}

pub fn draw_seed_button(
    display: &mut GuiDisplay<'_>,
    mode: GuiMode,
    hit: ButtonHit,
    state: Option<&SeedEntryState>,
    active: bool,
) {
    match hit.button {
        Button::Seed(_) if mode == GuiMode::SeedEntry => {
            if let Some(state) = state {
                draw_keypad_button(display, hit, state, active);
            }
        }
        Button::Seed(_) if mode == GuiMode::SeedConfirm => {
            draw_confirm_button(display, hit, active)
        }
        Button::Seed(sb) if mode == GuiMode::SeedFirstBoot => {
            draw_text_button(display, hit, setup_button_label(sb), active)
        }
        Button::Seed(SeedButton::EnterSeed) => draw_text_button(display, hit, "Enter Seed", active),
        _ => {}
    }
}

/// The actions on the setup screen: generate on-device, enter an existing phrase,
/// and (only when adding to an already-set-up device) a Back to the settings menu.
fn setup_buttons(show_back: bool) -> HVec<ButtonHit, 3> {
    let width = (SCREEN_WIDTH as i32 - 2 * 20).max(80);
    let height = 42;
    let x = ((SCREEN_WIDTH as i32) - width) / 2;
    let gap = 14;
    let y0 = header_height() + 84;
    let mut out: HVec<ButtonHit, 3> = HVec::new();
    let _ = out.push(ButtonHit {
        button: Button::Seed(SeedButton::GenerateSeed),
        top_left: Point::new(x, y0),
        size: Size::new(width as u32, height as u32),
    });
    let _ = out.push(ButtonHit {
        button: Button::Seed(SeedButton::EnterSeed),
        top_left: Point::new(x, y0 + height + gap),
        size: Size::new(width as u32, height as u32),
    });
    if show_back {
        let _ = out.push(ButtonHit {
            button: Button::Seed(SeedButton::Cancel),
            top_left: Point::new(x, y0 + 2 * (height + gap)),
            size: Size::new(width as u32, height as u32),
        });
    }
    out
}

fn setup_button_label(button: SeedButton) -> &'static str {
    match button {
        SeedButton::GenerateSeed => "Generate New",
        SeedButton::Cancel => "Back to menu",
        _ => "Enter Seed",
    }
}

pub(crate) fn draw_text_button(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    label: &str,
    active: bool,
) {
    draw_button_frame(display, hit, active);
    draw_fitted_button_label(display, hit, label);
}

fn draw_keypad(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let layout = keypad_layout();
    let geo = keypad_geometry();
    for row in 0..layout.len() {
        for col in 0..layout[row].len() {
            let hit = keypad_button_hit(row, col, &geo);
            draw_keypad_button(display, hit, state, false);
        }
    }
}

fn draw_keypad_button(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    state: &SeedEntryState,
    active: bool,
) {
    draw_button_frame(display, hit, active);

    let button = match hit.button {
        Button::Seed(button) => button,
        _ => return,
    };

    let center_x = hit.top_left.x + hit.size.width as i32 / 2;
    let center_y = hit.top_left.y + hit.size.height as i32 / 2;

    match button {
        SeedButton::Key(digit) => {
            let digit_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
            let letters_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT); // Bright, not subtle!
            let digit_label = char::from(b'0' + digit);
            let baseline_digit = center_y - 2;
            let mut digit_buf = [0u8; 4];
            let digit_str = digit_label.encode_utf8(&mut digit_buf);
            let _ = Text::with_alignment(
                digit_str,
                Point::new(center_x, baseline_digit),
                digit_style,
                Alignment::Center,
            )
            .draw(display);

            let letters = t9_letters(digit);
            let baseline_letters = center_y + 14;
            let _ = Text::with_alignment(
                letters,
                Point::new(center_x, baseline_letters),
                letters_style,
                Alignment::Center,
            )
            .draw(display);
        }
        SeedButton::NextSuggestion => draw_label(display, center_x, center_y, ">"),
        SeedButton::Backspace => draw_label(display, center_x, center_y, "DEL"),
        SeedButton::CommitWord => {
            let label = if state.current_suggestion().is_some() {
                "ADD"
            } else {
                "ADD"
            };
            draw_label(display, center_x, center_y, label);
        }
        SeedButton::Finish => draw_label(display, center_x, center_y, "DONE"),
        SeedButton::Cancel => draw_label(display, center_x, center_y, "BACK"),
        SeedButton::EnterSeed => draw_label(display, center_x, center_y, "Start"),
        SeedButton::GenerateSeed => {}
    }
}

fn draw_label(display: &mut GuiDisplay<'_>, x: i32, y: i32, label: &str) {
    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let baseline = y + FONT_10X20.character_size.height as i32 / 3;
    let _ = Text::with_alignment(label, Point::new(x, baseline), style, Alignment::Center)
        .draw(display);
}

fn draw_button_frame(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let Palette {
        base,
        light,
        dark,
        border,
    } = palette(active);

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

    if hit.size.height > 8 && hit.size.width > 8 {
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
                .stroke_color(light)
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
                .stroke_color(dark)
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
                .stroke_color(dark)
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

#[derive(Clone, Copy)]
struct Palette {
    base: Rgb565,
    light: Rgb565,
    dark: Rgb565,
    border: Rgb565,
}

fn palette(active: bool) -> Palette {
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

fn corner_buttons() -> [ButtonHit; 2] {
    let button_width = 50i32;
    let button_height = 36i32;
    let margin = 4i32;
    let y = SCREEN_HEIGHT as i32 - button_height - margin;

    [
        // DEL button (bottom left)
        ButtonHit {
            button: Button::Seed(SeedButton::Backspace),
            top_left: Point::new(margin, y),
            size: Size::new(button_width as u32, button_height as u32),
        },
        // ADD button (bottom right)
        ButtonHit {
            button: Button::Seed(SeedButton::CommitWord),
            top_left: Point::new(SCREEN_WIDTH as i32 - button_width - margin, y),
            size: Size::new(button_width as u32, button_height as u32),
        },
    ]
}

fn keypad_layout() -> [[SeedButton; 3]; 3] {
    [
        [SeedButton::Key(2), SeedButton::Key(3), SeedButton::Key(4)],
        [SeedButton::Key(5), SeedButton::Key(6), SeedButton::Key(7)],
        [
            SeedButton::Key(8),
            SeedButton::Key(9),
            SeedButton::NextSuggestion, // Use > for cycling through suggestions
        ],
    ]
}

struct KeypadGeometry {
    top: i32,
    button_width: i32,
    button_height: i32,
}

fn keypad_geometry() -> KeypadGeometry {
    let top = header_height() + KEYPAD_MARGIN * 2;
    // Reserve space at bottom for corner buttons
    let bottom_button_height = 36i32;
    let available_height = max(
        0,
        SCREEN_HEIGHT as i32 - top - bottom_button_height - KEYPAD_MARGIN * 2,
    );
    let button_width = max(32, (SCREEN_WIDTH as i32 - KEYPAD_MARGIN * 4) / 3);
    let button_height = max(
        28,
        (available_height - KEYPAD_MARGIN * 2) / 3, // 3 rows
    );
    KeypadGeometry {
        top,
        button_width,
        button_height,
    }
}

fn keypad_button_hit(row: usize, col: usize, geo: &KeypadGeometry) -> ButtonHit {
    let layout = keypad_layout();
    let button = layout[row][col];
    let x = KEYPAD_MARGIN + col as i32 * (geo.button_width + KEYPAD_MARGIN);
    let y = geo.top + row as i32 * (geo.button_height + KEYPAD_MARGIN);
    ButtonHit {
        button: Button::Seed(button),
        top_left: Point::new(x, y),
        size: Size::new(geo.button_width as u32, geo.button_height as u32),
    }
}

fn draw_corner_buttons(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    for hit in corner_buttons() {
        draw_keypad_button(display, hit, state, false);
    }
}

fn within_hit(hit: &ButtonHit, point: Point, slack: i32) -> bool {
    let left = hit.top_left.x - slack;
    let right = hit.top_left.x + hit.size.width as i32 + slack;
    let top = hit.top_left.y - slack;
    let bottom = hit.top_left.y + hit.size.height as i32 + slack;
    point.x >= left && point.x < right && point.y >= top && point.y < bottom
}

fn split_text<'a>(text: &'a str, max_width: usize) -> impl Iterator<Item = &'a str> {
    // Very small helper: split on spaces without allocation when the line would exceed max width.
    struct SplitLines<'a> {
        text: &'a str,
        max_width: usize,
        pos: usize,
    }

    impl<'a> Iterator for SplitLines<'a> {
        type Item = &'a str;

        fn next(&mut self) -> Option<Self::Item> {
            if self.pos >= self.text.len() {
                return None;
            }
            let remaining = &self.text[self.pos..];
            if remaining.len() <= self.max_width {
                self.pos = self.text.len();
                return Some(remaining);
            }
            let mut end = self.pos + self.max_width;
            while end > self.pos && !self.text.as_bytes()[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            if end == self.pos {
                end = self.pos + self.max_width;
            }
            let slice = self.text[self.pos..end].trim();
            self.pos = end;
            while self.pos < self.text.len() && self.text.as_bytes()[self.pos].is_ascii_whitespace()
            {
                self.pos += 1;
            }
            Some(slice)
        }
    }

    SplitLines {
        text,
        max_width,
        pos: 0,
    }
}

fn word_matches_digits(word: &str, digits: &[u8]) -> bool {
    if digits.len() > word.len() {
        return false;
    }
    for (ch, digit) in word.chars().zip(digits.iter()) {
        if digit_for_char(ch) != Some(*digit) {
            return false;
        }
    }
    true
}

fn digit_for_char(ch: char) -> Option<u8> {
    match ch {
        'a' | 'b' | 'c' => Some(2),
        'd' | 'e' | 'f' => Some(3),
        'g' | 'h' | 'i' => Some(4),
        'j' | 'k' | 'l' => Some(5),
        'm' | 'n' | 'o' => Some(6),
        'p' | 'q' | 'r' | 's' => Some(7),
        't' | 'u' | 'v' => Some(8),
        'w' | 'x' | 'y' | 'z' => Some(9),
        _ => None,
    }
}

fn t9_letters(digit: u8) -> &'static str {
    match digit {
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

/// Build a standard 24-word BIP-39 mnemonic from 256 bits of entropy.
///
/// CS = ENT/32 = 8 checksum bits = the first byte of SHA-256(entropy). The
/// 256 entropy bits followed by the 8 checksum bits make 264 bits = 24 × 11,
/// and each 11-bit group (MSB-first) indexes the 2048-word English list.
fn mnemonic_from_entropy(entropy: &[u8; 32]) -> Option<SeedPhrase> {
    let checksum = Sha256::digest(entropy)[0];
    let mut bits = [0u8; 33];
    bits[..32].copy_from_slice(entropy);
    bits[32] = checksum;

    let mut phrase = SeedPhrase::new();
    for i in 0..MAX_SEED_WORDS {
        let index = extract_11_bits(&bits, i * 11);
        let word = word_at_index(index)?;
        let mut committed = SeedWord::new();
        if committed.push_str(word).is_err() || phrase.push(committed).is_err() {
            bits.zeroize();
            return None;
        }
    }
    bits.zeroize();
    Some(phrase)
}

/// Read 11 bits (MSB-first) starting at bit offset `start` from `bits`.
fn extract_11_bits(bits: &[u8], start: usize) -> usize {
    let mut value = 0usize;
    for i in 0..11 {
        let bit = start + i;
        let byte = bits[bit / 8];
        let set = (byte >> (7 - (bit % 8))) & 1;
        value = (value << 1) | set as usize;
    }
    value
}

fn word_at_index(index: usize) -> Option<&'static str> {
    WORDLIST.lines().nth(index)
}

pub fn render_seed_confirm(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let _ = display.clear(COLOR_BACKGROUND);
    // For a generated seed the user is seeing it for the first time and must
    // copy it down; for a typed-in seed this is a read-back confirmation.
    let title = if state.generated {
        "Write It Down"
    } else {
        "Confirm Seed"
    };
    render_header(display, title, COLOR_SURFACE_HIGH);

    // Two columns (1-12 | 13-24) so the full 24-word phrase fits on screen.
    let text_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
    let header_h = header_height();
    let top = header_h + 14;
    let line_height = (FONT_6X10.character_size.height as i32) + 4;
    let left_x = 8;
    let right_x = (SCREEN_WIDTH as i32) / 2 + 5;
    let per_col = MAX_SEED_WORDS / 2;
    let panel_top = top - 8;
    let panel_height = (line_height * per_col as i32 + 18) as u32;
    let panel_shadow = Rectangle::new(
        Point::new(5, panel_top + 1),
        Size::new((SCREEN_WIDTH - 9) as u32, panel_height),
    );
    let _ = panel_shadow
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLOR_PANEL_SHADOW)
                .stroke_width(0)
                .build(),
        )
        .draw(display);
    let panel = Rectangle::new(
        Point::new(4, panel_top),
        Size::new((SCREEN_WIDTH - 8) as u32, panel_height),
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
        Point::new(6, panel_top + 2),
        Point::new(SCREEN_WIDTH as i32 - 7, panel_top + 2),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(COLOR_PANEL_HIGHLIGHT)
            .stroke_width(1)
            .build(),
    )
    .draw(display);
    let center_x = (SCREEN_WIDTH as i32) / 2;
    let _ = Line::new(
        Point::new(center_x, panel_top + 8),
        Point::new(center_x, panel_top + panel_height as i32 - 8),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(COLOR_DIVIDER)
            .stroke_width(1)
            .build(),
    )
    .draw(display);

    for (idx, word) in state.words.iter().enumerate() {
        let (x, row) = if idx < per_col {
            (left_x, idx as i32)
        } else {
            (right_x, (idx - per_col) as i32)
        };
        let y = top + row * line_height;
        let mut line_buf = HString::<20>::new();
        let _ = write!(line_buf, "{:2}.{}", idx + 1, word.as_str());
        let _ = Text::new(line_buf.as_str(), Point::new(x, y), text_style).draw(display);
    }

    // Draw confirm/cancel buttons at bottom
    for hit in confirm_buttons() {
        draw_confirm_button(display, hit, false);
    }
}

pub fn button_from_point_seed_confirm(point: Point) -> Option<ButtonHit> {
    for button in &confirm_buttons() {
        if within_hit(button, point, 4) {
            return Some(*button);
        }
    }
    None
}

fn confirm_buttons() -> [ButtonHit; 2] {
    let button_width = 70i32;
    let button_height = 40i32;
    let margin = 8i32;
    let y = SCREEN_HEIGHT as i32 - button_height - margin;

    [
        // Cancel button (bottom left)
        ButtonHit {
            button: Button::Seed(SeedButton::Cancel),
            top_left: Point::new(margin, y),
            size: Size::new(button_width as u32, button_height as u32),
        },
        // Finish button (bottom right)
        ButtonHit {
            button: Button::Seed(SeedButton::Finish),
            top_left: Point::new(SCREEN_WIDTH as i32 - button_width - margin, y),
            size: Size::new(button_width as u32, button_height as u32),
        },
    ]
}

fn draw_confirm_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    draw_button_frame(display, hit, active);

    let button = match hit.button {
        Button::Seed(button) => button,
        _ => return,
    };

    let label = match button {
        SeedButton::Cancel => "BACK",
        SeedButton::Finish => "CONFIRM",
        _ => "",
    };

    draw_fitted_button_label(display, hit, label);
}

fn draw_fitted_button_label(display: &mut GuiDisplay<'_>, hit: ButtonHit, label: &str) {
    let available = hit.size.width.saturating_sub(8) as usize;
    let large_width = label.len() * FONT_10X20.character_size.width as usize;
    let center = Point::new(
        hit.top_left.x + hit.size.width as i32 / 2,
        hit.top_left.y + hit.size.height as i32 / 2,
    );
    if large_width <= available {
        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
        let _ = Text::with_alignment(
            label,
            Point::new(center.x, baseline),
            style,
            Alignment::Center,
        )
        .draw(display);
    } else {
        let style = MonoTextStyle::new(&FONT_8X13, COLOR_TEXT);
        let baseline = center.y + FONT_8X13.character_size.height as i32 / 3;
        let _ = Text::with_alignment(
            label,
            Point::new(center.x, baseline),
            style,
            Alignment::Center,
        )
        .draw(display);
    }
}
