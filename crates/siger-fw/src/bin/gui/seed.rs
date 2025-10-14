use core::cmp::{max, min};
use core::fmt::Write;

use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use heapless::{String as HString, Vec as HVec};

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
const INFO_AREA_HEIGHT: i32 = 64;
const INFO_PANEL_MARGIN: i32 = 2;
const KEYPAD_MARGIN: i32 = 3;

pub type SeedWord = HString<MAX_WORD_LEN>;
pub type SeedPhrase = HVec<SeedWord, MAX_SEED_WORDS>;

#[derive(Clone, Debug)]
pub enum SeedInteraction {
    EnterSeedRequested,
    WordCommitted(SeedWord),
    WordRemoved(Option<SeedWord>),
    EntryCompleted(SeedPhrase),
    EntryCancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedButton {
    EnterSeed,
    Key(u8),
    Backspace,
    NextSuggestion,
    PrevSuggestion,
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
}

impl SeedEntryState {
    pub const fn new() -> Self {
        Self {
            digits: HVec::new(),
            suggestions: HVec::new(),
            suggestion_index: 0,
            total_matches: 0,
            words: HVec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.digits.clear();
        self.suggestions.clear();
        self.suggestion_index = 0;
        self.total_matches = 0;
        self.words.clear();
    }

    pub fn words(&self) -> &SeedPhrase {
        &self.words
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

    pub fn prev_suggestion(&mut self) -> bool {
        if self.suggestions.is_empty() {
            return false;
        }
        if self.suggestion_index == 0 {
            self.suggestion_index = self.suggestions.len() - 1;
        } else {
            self.suggestion_index -= 1;
        }
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
        if self.words.is_empty() || !self.digits.is_empty() {
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

    pub fn digits_as_string(&self) -> HString<MAX_DIGITS> {
        let mut buf = HString::<MAX_DIGITS>::new();
        for digit in self.digits.iter() {
            let ch = (b'0' + *digit) as char;
            let _ = buf.push(ch);
        }
        buf
    }

    pub fn typed_prefix(&self) -> Option<SeedWord> {
        let suggestion = self.current_suggestion()?;
        let mut prefix = SeedWord::new();
        for (idx, ch) in suggestion.chars().enumerate() {
            if idx >= self.digits.len() {
                break;
            }
            if prefix.push(ch).is_err() {
                break;
            }
        }
        Some(prefix)
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

pub fn render_seed_setup(display: &mut GuiDisplay<'_>) {
    let _ = display.clear(COLOR_BACKGROUND);
    render_header(display, "Seed Required", COLOR_SURFACE_HIGH);

    let mut body = HString::<96>::new();
    let _ = body.push_str("Welcome! This wallet needs your seed words before it can unlock.");

    let text_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
    let mut y = header_height() + 24;
    for line in split_text(body.as_str(), SCREEN_WIDTH as usize - 16) {
        let _ = Text::with_alignment(
            line,
            Point::new((SCREEN_WIDTH / 2) as i32, y),
            text_style,
            Alignment::Center,
        )
        .draw(display);
        y += (FONT_6X10.character_size.height as i32) + 4;
    }

    let button = enter_seed_button_hit();
    draw_seed_button(display, GuiMode::SeedFirstBoot, button, None, false);
}

pub fn render_seed_entry(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let _ = display.clear(COLOR_BACKGROUND);
    draw_info_header(display, state);
    draw_keypad(display, state);
}

pub fn button_from_point_seed_setup(point: Point) -> Option<ButtonHit> {
    let hit = enter_seed_button_hit();
    if within_hit(&hit, point, 0) {
        Some(hit)
    } else {
        None
    }
}

pub fn button_from_point_seed_entry(point: Point) -> Option<ButtonHit> {
    // Check header buttons first
    for button in &header_buttons() {
        if within_hit(button, point, 4) {
            return Some(*button);
        }
    }
    // Then check keypad
    let geo = keypad_geometry();

    // Check the wide ADD button
    let add_button = add_word_button_hit(&geo);
    if within_hit(&add_button, point, 10) {
        return Some(add_button);
    }

    // Check regular keypad buttons
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
        Button::Seed(SeedButton::EnterSeed) => draw_enter_seed_button(display, hit, active),
        Button::Seed(_) if mode == GuiMode::SeedEntry => {
            draw_keypad_button(display, hit, state.expect("seed state required"), active)
        }
        Button::Seed(_) if mode == GuiMode::SeedFirstBoot => {
            draw_enter_seed_button(display, hit, active)
        }
        _ => {}
    }
}

fn enter_seed_button_hit() -> ButtonHit {
    let width = (SCREEN_WIDTH as i32 - 2 * 24).max(80);
    let height = 40;
    let x = ((SCREEN_WIDTH as i32) - width) / 2;
    let y = header_height() + 72;
    ButtonHit {
        button: Button::Seed(SeedButton::EnterSeed),
        top_left: Point::new(x, y),
        size: Size::new(width as u32, height as u32),
    }
}

fn draw_enter_seed_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    draw_button_frame(display, hit, active);
    let label = "Enter Seed";
    let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
    let center = Point::new(
        hit.top_left.x + hit.size.width as i32 / 2,
        hit.top_left.y + hit.size.height as i32 / 2 + 4,
    );
    let _ = Text::with_alignment(label, center, style, Alignment::Center).draw(display);
}

fn draw_info_header(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let header_h = header_height();
    let info_h = INFO_AREA_HEIGHT;

    // Draw background for info area
    let info_rect = Rectangle::new(
        Point::new(0, header_h),
        Size::new(SCREEN_WIDTH.into(), info_h as u32),
    );
    let _ = info_rect
        .into_styled(PrimitiveStyleBuilder::new().fill_color(COLOR_SURFACE_HIGH).build())
        .draw(display);

    // Draw header buttons
    for hit in header_buttons() {
        draw_keypad_button(display, hit, state, false);
    }

    // Draw current word being typed (white text!)
    let text_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);  // White, not subtle!
    let center_x = (SCREEN_WIDTH / 2) as i32;
    let word_y = header_h + info_h / 2 + 6;

    let mut display_text = HString::<32>::new();

    // Show typed prefix if we have digits
    if !state.digits.is_empty() {
        if let Some(suggestion) = state.current_suggestion() {
            // Show typed portion
            for (idx, ch) in suggestion.chars().enumerate() {
                if idx < state.digits.len() {
                    let _ = display_text.push(ch);
                }
            }
            // Show rest in gray
            let remaining: HString<16> = suggestion.chars().skip(state.digits.len()).collect();

            let _ = Text::with_alignment(
                display_text.as_str(),
                Point::new(center_x, word_y),
                text_style,
                Alignment::Center,
            )
            .draw(display);

            // Draw remaining in subtle color
            let subtle_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT_SUBTLE);
            let typed_width = (display_text.len() * 10) as i32;  // Approximate width
            let _ = Text::new(
                remaining.as_str(),
                Point::new(center_x + typed_width / 2, word_y),
                subtle_style,
            )
            .draw(display);

            // Show match count
            if let Some((pos, total)) = state.suggestion_position() {
                let mut count_buf = HString::<16>::new();
                let _ = write!(count_buf, "{}/{}", pos, total);
                let count_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
                let _ = Text::with_alignment(
                    count_buf.as_str(),
                    Point::new(center_x, header_h + info_h - 8),
                    count_style,
                    Alignment::Center,
                )
                .draw(display);
            }
        } else {
            // No match
            let _ = display_text.push_str("???");
            let _ = Text::with_alignment(
                display_text.as_str(),
                Point::new(center_x, word_y),
                text_style,
                Alignment::Center,
            )
            .draw(display);
        }
    } else if state.words.is_empty() {
        let _ = display_text.push_str("--");
        let subtle_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT_SUBTLE);
        let _ = Text::with_alignment(
            display_text.as_str(),
            Point::new(center_x, word_y),
            subtle_style,
            Alignment::Center,
        )
        .draw(display);
    }

    // Draw word count at top
    let mut word_count = HString::<24>::new();
    let _ = write!(word_count, "Word {}/{}", min(state.words.len() + 1, MAX_SEED_WORDS), MAX_SEED_WORDS);
    let small_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);
    let _ = Text::with_alignment(
        word_count.as_str(),
        Point::new(center_x, header_h + 12),
        small_style,
        Alignment::Center,
    )
    .draw(display);
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
    // Draw the wide ADD WORD button at the bottom
    let add_button = add_word_button_hit(&geo);
    draw_keypad_button(display, add_button, state, false);
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
            let letters_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT);  // Bright, not subtle!
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
        SeedButton::PrevSuggestion => draw_label(display, center_x, center_y, "PREV"),
        SeedButton::NextSuggestion => draw_label(display, center_x, center_y, "NEXT"),
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

fn header_buttons() -> [ButtonHit; 4] {
    let header_h = header_height();
    let button_size = 28i32;
    let margin = 4i32;
    let y = header_h + margin;

    [
        // Cancel button (left)
        ButtonHit {
            button: Button::Seed(SeedButton::Cancel),
            top_left: Point::new(margin, y),
            size: Size::new(button_size as u32, button_size as u32),
        },
        // Finish button (right)
        ButtonHit {
            button: Button::Seed(SeedButton::Finish),
            top_left: Point::new((SCREEN_WIDTH as i32 - button_size - margin), y),
            size: Size::new(button_size as u32, button_size as u32),
        },
        // Prev button (left center)
        ButtonHit {
            button: Button::Seed(SeedButton::PrevSuggestion),
            top_left: Point::new(margin, y + button_size + margin),
            size: Size::new(button_size as u32, button_size as u32),
        },
        // Next button (right center)
        ButtonHit {
            button: Button::Seed(SeedButton::NextSuggestion),
            top_left: Point::new((SCREEN_WIDTH as i32 - button_size - margin), y + button_size + margin),
            size: Size::new(button_size as u32, button_size as u32),
        },
    ]
}

fn keypad_layout() -> [[SeedButton; 3]; 3] {
    [
        [
            SeedButton::Key(2),
            SeedButton::Key(3),
            SeedButton::Key(4),
        ],
        [
            SeedButton::Key(5),
            SeedButton::Key(6),
            SeedButton::Key(7),
        ],
        [
            SeedButton::Key(8),
            SeedButton::Key(9),
            SeedButton::Backspace,
        ],
    ]
}

struct KeypadGeometry {
    top: i32,
    button_width: i32,
    button_height: i32,
}

fn keypad_geometry() -> KeypadGeometry {
    let top = header_height() + INFO_AREA_HEIGHT + KEYPAD_MARGIN;
    let available_height = max(0, SCREEN_HEIGHT as i32 - top - KEYPAD_MARGIN);
    let button_width = max(
        32,
        (SCREEN_WIDTH as i32 - KEYPAD_MARGIN * 4) / 3,
    );
    let button_height = max(
        28,
        (available_height - KEYPAD_MARGIN * 4) / 4,  // 4 rows total (3 keypad + 1 ADD button)
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
        size: Size::new(
            geo.button_width as u32,
            geo.button_height as u32,
        ),
    }
}

fn add_word_button_hit(geo: &KeypadGeometry) -> ButtonHit {
    let layout = keypad_layout();
    let bottom_row = layout.len();
    let y = geo.top + bottom_row as i32 * (geo.button_height + KEYPAD_MARGIN);
    let width = (SCREEN_WIDTH as i32 - KEYPAD_MARGIN * 2);
    ButtonHit {
        button: Button::Seed(SeedButton::CommitWord),
        top_left: Point::new(KEYPAD_MARGIN, y),
        size: Size::new(width as u32, geo.button_height as u32),
    }
}

fn within_hit(hit: &ButtonHit, point: Point, slack: i32) -> bool {
    let left = hit.top_left.x - slack;
    let right = hit.top_left.x + hit.size.width as i32 + slack;
    let top = hit.top_left.y - slack;
    let bottom = hit.top_left.y + hit.size.height as i32 + slack;
    point.x >= left && point.x < right && point.y >= top && point.y < bottom
}

fn draw_left_aligned_text(
    display: &mut GuiDisplay<'_>,
    text: &HString<64>,
    style: MonoTextStyle<'_, Rgb565>,
    left: i32,
    base_top: i32,
    line: usize,
) {
    let baseline = base_top + line as i32 * ((FONT_6X10.character_size.height as i32) + 4);
    let _ = Text::new(
        text.as_str(),
        Point::new(left, baseline),
        style,
    )
    .draw(display);
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
