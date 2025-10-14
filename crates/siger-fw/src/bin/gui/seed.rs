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
const INFO_AREA_HEIGHT: i32 = 96;
const INFO_PANEL_MARGIN: i32 = 4;
const INFO_BUTTON_MARGIN: i32 = 12;
const INFO_BUTTON_HEIGHT: u32 = 28;
const KEYPAD_MARGIN: i32 = 6;

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
    render_header(display, "Seed Entry", COLOR_SURFACE_HIGH);
    draw_info_panel(display, state);
    draw_info_buttons(display, state);
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
    for button in &info_buttons() {
        if within_hit(button, point, 4) {
            return Some(*button);
        }
    }
    let geo = keypad_geometry();
    for row in 0..keypad_layout().len() {
        for col in 0..keypad_layout()[row].len() {
            let hit = keypad_button_hit(row, col, &geo);
            if within_hit(&hit, point, 8) {
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

fn draw_info_panel(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let panel_top = header_height() + INFO_PANEL_MARGIN;
    let panel_height = INFO_AREA_HEIGHT - 2 * INFO_PANEL_MARGIN;
    let panel_rect = Rectangle::new(
        Point::new(INFO_PANEL_MARGIN, panel_top),
        Size::new(
            (SCREEN_WIDTH as i32 - 2 * INFO_PANEL_MARGIN) as u32,
            panel_height as u32,
        ),
    );
    let panel_style = PrimitiveStyleBuilder::new()
        .fill_color(COLOR_SURFACE_LOW)
        .stroke_color(COLOR_DIVIDER)
        .stroke_width(1)
        .build();
    let _ = panel_rect.into_styled(panel_style).draw(display);

    let text_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
    let text_left = INFO_PANEL_MARGIN + INFO_BUTTON_MARGIN;
    let text_base =
        panel_top + INFO_BUTTON_MARGIN + INFO_BUTTON_HEIGHT as i32 + FONT_6X10.character_size.height as i32;

    let mut buf = HString::<64>::new();
    let word_number = min(state.words.len() + 1, MAX_SEED_WORDS);
    let _ = write!(buf, "Word {} of {}", word_number, MAX_SEED_WORDS);
    draw_left_aligned_text(display, &buf, text_style, text_left, text_base, 0);

    buf.clear();
    if state.digits.is_empty() {
        let _ = buf.push_str("Digits: (none)");
    } else {
        let digits = state.digits_as_string();
        let _ = write!(buf, "Digits: {}", digits.as_str());
    }
    draw_left_aligned_text(display, &buf, text_style, text_left, text_base, 1);

    buf.clear();
    match (state.current_suggestion(), state.suggestion_position()) {
        (Some(word), Some((pos, total))) => {
            let _ = write!(buf, "Suggestion: {} ({}/{})", word, pos, total);
        }
        (Some(word), None) => {
            let _ = write!(buf, "Suggestion: {}", word);
        }
        (None, _) => {
            let _ = buf.push_str("Suggestion: (no match)");
        }
    }
    draw_left_aligned_text(display, &buf, text_style, text_left, text_base, 2);

    buf.clear();
    if state.words.is_empty() {
        let _ = buf.push_str("Seed: --");
    } else {
        let _ = write!(buf, "Seed ({}):", state.words.len());
    }
    draw_left_aligned_text(display, &buf, text_style, text_left, text_base, 3);

    if !state.words.is_empty() {
        let mut word_line = HString::<64>::new();
        let skip = state.words.len().saturating_sub(4);
        if skip > 0 {
            let _ = word_line.push('…');
            let _ = word_line.push(' ');
        }
        for word in state.words.iter().skip(skip) {
            if !word_line.is_empty() && !word_line.as_str().ends_with(' ') {
                let _ = word_line.push(' ');
            }
            if word_line.push_str(word.as_str()).is_err() {
                break;
            }
        }
        draw_left_aligned_text(display, &word_line, text_style, text_left, text_base, 4);
    }
}

fn draw_info_buttons(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    for hit in info_buttons() {
        draw_keypad_button(display, hit, state, false);
    }
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
            let letters_style = MonoTextStyle::new(&FONT_6X10, COLOR_TEXT_SUBTLE);
            let digit_label = char::from(b'0' + digit);
            let baseline_digit = hit.top_left.y + 18;
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
            let baseline_letters = hit.top_left.y + hit.size.height as i32 - 6;
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

fn info_buttons() -> [ButtonHit; 2] {
    let panel_left = INFO_PANEL_MARGIN;
    let panel_width = SCREEN_WIDTH as i32 - 2 * INFO_PANEL_MARGIN;
    let width = max(40, (panel_width - INFO_BUTTON_MARGIN * 3) / 2);
    let y = header_height() + INFO_PANEL_MARGIN + INFO_BUTTON_MARGIN;
    let left_x = panel_left + INFO_BUTTON_MARGIN;
    let right_x = left_x + width + INFO_BUTTON_MARGIN;
    [
        ButtonHit {
            button: Button::Seed(SeedButton::Cancel),
            top_left: Point::new(left_x, y),
            size: Size::new(width as u32, INFO_BUTTON_HEIGHT),
        },
        ButtonHit {
            button: Button::Seed(SeedButton::Finish),
            top_left: Point::new(right_x, y),
            size: Size::new(width as u32, INFO_BUTTON_HEIGHT),
        },
    ]
}

fn keypad_layout() -> [[SeedButton; 3]; 4] {
    [
        [
            SeedButton::PrevSuggestion,
            SeedButton::Key(2),
            SeedButton::Key(3),
        ],
        [
            SeedButton::Key(4),
            SeedButton::Key(5),
            SeedButton::Key(6),
        ],
        [
            SeedButton::Key(7),
            SeedButton::Key(8),
            SeedButton::Key(9),
        ],
        [
            SeedButton::Backspace,
            SeedButton::NextSuggestion,
            SeedButton::CommitWord,
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
        (available_height - KEYPAD_MARGIN * 3) / 4,
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
