use core::cmp::max;
use core::fmt::Write;

use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10, FONT_8X13};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};
use heapless::{String as HString, Vec as HVec};
use nockster_core::diceware::{dice_entropy_256, DICE_ROLLS_FOR_256_BITS};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::constants::*;
use super::layout::{header_back_button, header_height, point_in_header_back};
use super::palette;
use super::render::{render_header, render_header_with_back};
use super::state::{Button, ButtonHit, GuiMode};
use super::GuiDisplay;

const WORDLIST: &str = include_str!("bip39_english.txt");

pub const MAX_SEED_WORDS: usize = 24;
const MAX_WORD_LEN: usize = 12;
const MAX_DIGITS: usize = 8;
const SUGGESTION_CAP: usize = 6;
const KEYPAD_MARGIN: i32 = 3;
const SEED_VERIFY_WORDS: usize = 3;

pub type SeedWord = HString<MAX_WORD_LEN>;
pub type SeedPhrase = HVec<SeedWord, MAX_SEED_WORDS>;

pub fn zeroize_seed_phrase(phrase: &mut SeedPhrase) {
    for word in phrase.iter_mut() {
        // Zero is valid UTF-8, so overwriting the initialized string bytes
        // preserves String's invariant before clear updates its length.
        unsafe { word.as_mut_str().as_bytes_mut() }.zeroize();
        word.clear();
    }
    phrase.clear();
}

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
    DiceSeed,
    DiceFace(u8),
    VerifyChoice(u8),
    Key(u8),
    Backspace,
    NextSuggestion,
    CommitWord,
    Finish,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeedOrigin {
    Entered,
    Hardware,
    Dice,
}

#[derive(Clone)]
struct SeedVerificationState {
    targets: [u8; SEED_VERIFY_WORDS],
    choices: [[u16; 3]; SEED_VERIFY_WORDS],
    correct_choices: [u8; SEED_VERIFY_WORDS],
    step: usize,
    active: bool,
    failed: bool,
}

impl SeedVerificationState {
    const fn new() -> Self {
        Self {
            targets: [0; SEED_VERIFY_WORDS],
            choices: [[0; 3]; SEED_VERIFY_WORDS],
            correct_choices: [0; SEED_VERIFY_WORDS],
            step: 0,
            active: false,
            failed: false,
        }
    }

    fn reset(&mut self) {
        self.targets.zeroize();
        self.choices.zeroize();
        self.correct_choices.zeroize();
        self.step = 0;
        self.active = false;
        self.failed = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedVerificationResult {
    More,
    Complete,
    Incorrect,
}

#[derive(Clone)]
pub struct SeedEntryState {
    digits: HVec<u8, MAX_DIGITS>,
    suggestions: HVec<&'static str, SUGGESTION_CAP>,
    suggestion_index: usize,
    total_matches: usize,
    words: SeedPhrase,
    dice_rolls: HVec<u8, DICE_ROLLS_FOR_256_BITS>,
    origin: SeedOrigin,
    verification: SeedVerificationState,
}

impl SeedEntryState {
    pub const fn new() -> Self {
        Self {
            digits: HVec::new(),
            suggestions: HVec::new(),
            suggestion_index: 0,
            total_matches: 0,
            words: HVec::new(),
            dice_rolls: HVec::new(),
            origin: SeedOrigin::Entered,
            verification: SeedVerificationState::new(),
        }
    }

    pub fn reset(&mut self) {
        self.digits.clear();
        self.suggestions.clear();
        self.suggestion_index = 0;
        self.total_matches = 0;
        zeroize_seed_phrase(&mut self.words);
        self.clear_dice_rolls();
        self.origin = SeedOrigin::Entered;
        self.verification.reset();
    }

    pub fn is_generated(&self) -> bool {
        self.origin != SeedOrigin::Entered
    }

    pub fn is_dice_generated(&self) -> bool {
        self.origin == SeedOrigin::Dice
    }

    pub fn verification_failed(&self) -> bool {
        self.verification.failed
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
                self.origin = SeedOrigin::Hardware;
                true
            }
            None => false,
        }
    }

    pub fn dice_roll_count(&self) -> usize {
        self.dice_rolls.len()
    }

    pub fn dice_rolls(&self) -> &[u8] {
        self.dice_rolls.as_slice()
    }

    pub fn push_dice_roll(&mut self, roll: u8) -> bool {
        (1..=6).contains(&roll)
            && self.dice_rolls.len() < DICE_ROLLS_FOR_256_BITS
            && self.dice_rolls.push(roll).is_ok()
    }

    pub fn pop_dice_roll(&mut self) -> bool {
        let Some(last) = self.dice_rolls.last_mut() else {
            return false;
        };
        last.zeroize();
        let _ = self.dice_rolls.pop();
        true
    }

    pub fn dice_rolls_complete(&self) -> bool {
        self.dice_rolls.len() == DICE_ROLLS_FOR_256_BITS
    }

    pub fn load_dice_generated(&mut self) -> bool {
        let Ok(mut entropy) = dice_entropy_256(self.dice_rolls.as_slice()) else {
            return false;
        };
        let phrase = mnemonic_from_entropy(&entropy);
        entropy.zeroize();
        match phrase {
            Some(words) => {
                self.reset();
                self.words = words;
                self.origin = SeedOrigin::Dice;
                true
            }
            None => false,
        }
    }

    pub fn begin_verification(&mut self) -> bool {
        if !self.is_generated() || !self.is_complete() {
            return false;
        }
        let mut random = [0u8; 32];
        if getrandom::getrandom(&mut random).is_err() {
            random.zeroize();
            return false;
        }

        let mut targets = [0u8; SEED_VERIFY_WORDS];
        for challenge in 0..SEED_VERIFY_WORDS {
            let mut target = random[challenge] % MAX_SEED_WORDS as u8;
            while targets[..challenge].contains(&target) {
                target = (target + 1) % MAX_SEED_WORDS as u8;
            }
            targets[challenge] = target;

            let Some(correct_word) = self.words.get(target as usize) else {
                random.zeroize();
                return false;
            };
            let Some(correct_index) = word_index(correct_word.as_str()) else {
                random.zeroize();
                return false;
            };
            let correct_slot = random[3 + challenge] % 3;
            let mut choices = [0u16; 3];
            choices[correct_slot as usize] = correct_index as u16;
            let mut random_offset = 6 + challenge * 4;
            for slot in 0..3 {
                if slot == correct_slot as usize {
                    continue;
                }
                let mut candidate =
                    u16::from_be_bytes([random[random_offset], random[random_offset + 1]])
                        & 0x07ff;
                random_offset += 2;
                while candidate == correct_index as u16 || choices[..slot].contains(&candidate) {
                    candidate = (candidate + 1) & 0x07ff;
                }
                choices[slot] = candidate;
            }
            self.verification.choices[challenge] = choices;
            self.verification.correct_choices[challenge] = correct_slot;
        }
        random.zeroize();
        self.verification.targets = targets;
        self.verification.step = 0;
        self.verification.active = true;
        self.verification.failed = false;
        true
    }

    pub fn verification_prompt(&self) -> Option<(usize, usize)> {
        self.verification.active.then(|| {
            (
                self.verification.step + 1,
                self.verification.targets[self.verification.step] as usize + 1,
            )
        })
    }

    pub fn verification_choice(&self, choice: usize) -> Option<&'static str> {
        if !self.verification.active || choice >= 3 {
            return None;
        }
        word_at_index(self.verification.choices[self.verification.step][choice] as usize)
    }

    pub fn verify_choice(&mut self, choice: usize) -> SeedVerificationResult {
        if !self.verification.active
            || choice >= 3
            || choice != self.verification.correct_choices[self.verification.step] as usize
        {
            self.verification.reset();
            self.verification.failed = true;
            return SeedVerificationResult::Incorrect;
        }
        self.verification.step += 1;
        if self.verification.step == SEED_VERIFY_WORDS {
            self.verification.reset();
            SeedVerificationResult::Complete
        } else {
            SeedVerificationResult::More
        }
    }

    pub fn cancel_verification(&mut self) {
        self.verification.reset();
    }

    fn clear_dice_rolls(&mut self) {
        for roll in self.dice_rolls.iter_mut() {
            roll.zeroize();
        }
        self.dice_rolls.clear();
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
            let mut word = self.words.pop()?;
            unsafe { word.as_mut_str().as_bytes_mut() }.zeroize();
            word.clear();
            Some(word)
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

    pub fn is_complete(&self) -> bool {
        self.words.len() == MAX_SEED_WORDS && self.digits.is_empty()
    }

    pub fn take_finished(&mut self) -> Option<SeedPhrase> {
        self.is_complete()
            .then(|| core::mem::take(&mut self.words))
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

impl Drop for SeedEntryState {
    fn drop(&mut self) {
        self.reset();
    }
}

pub fn render_seed_setup(display: &mut GuiDisplay<'_>, title: &str, show_back: bool) {
    let _ = display.clear(palette::background());
    if show_back {
        render_header_with_back(display, title, palette::surface_high(), false);
    } else {
        render_header(display, title, palette::surface_high());
    }

    let mut body = HString::<96>::new();
    let _ = body.push_str(
        "Generate with the hardware RNG, roll four labeled dice 25 times, or enter an existing seed.",
    );
    let text_style = MonoTextStyle::new(&FONT_6X10, palette::text());
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
    let _ = display.clear(palette::background());

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

    render_header(display, header_text.as_str(), palette::surface_high());
    draw_keypad(display, state);
    draw_corner_buttons(display, state);
}

pub fn button_from_point_seed_setup(point: Point, show_back: bool) -> Option<ButtonHit> {
    if show_back && point_in_header_back(point) {
        return Some(header_back_button(Button::Back));
    }
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
        Button::Seed(_) if mode == GuiMode::SeedDice => {
            if let Some(state) = state {
                draw_dice_button(display, hit, state, active);
            }
        }
        Button::Seed(_) if mode == GuiMode::SeedVerify => {
            if let Some(state) = state {
                draw_verify_button(display, hit, state, active);
            }
        }
        Button::Seed(sb) if mode == GuiMode::SeedFirstBoot => {
            draw_text_button(display, hit, setup_button_label(sb), active)
        }
        Button::Seed(SeedButton::EnterSeed) => draw_text_button(display, hit, "Enter Seed", active),
        _ => {}
    }
}

/// The actions on the setup screen: hardware RNG, physical dice, and an
/// existing phrase. Back is provided by the header when adding a seed.
fn setup_buttons(_show_back: bool) -> HVec<ButtonHit, 3> {
    let width = (SCREEN_WIDTH as i32 - 2 * 20).max(80);
    let height = 42;
    let x = ((SCREEN_WIDTH as i32) - width) / 2;
    let gap = 10;
    let y0 = header_height() + 100;
    let mut out: HVec<ButtonHit, 3> = HVec::new();
    let _ = out.push(ButtonHit {
        button: Button::Seed(SeedButton::GenerateSeed),
        top_left: Point::new(x, y0),
        size: Size::new(width as u32, height as u32),
    });
    let _ = out.push(ButtonHit {
        button: Button::Seed(SeedButton::DiceSeed),
        top_left: Point::new(x, y0 + height + gap),
        size: Size::new(width as u32, height as u32),
    });
    let _ = out.push(ButtonHit {
        button: Button::Seed(SeedButton::EnterSeed),
        top_left: Point::new(x, y0 + (height + gap) * 2),
        size: Size::new(width as u32, height as u32),
    });
    out
}

fn setup_button_label(button: SeedButton) -> &'static str {
    match button {
        SeedButton::GenerateSeed => "Hardware RNG",
        SeedButton::DiceSeed => "4 Dice x 25",
        SeedButton::Cancel => "Back to menu",
        _ => "Enter Seed",
    }
}

pub fn render_seed_dice(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let _ = display.clear(palette::background());
    let roll_count = state.dice_roll_count();
    let phase = roll_count % 4;
    let round = (roll_count / 4 + 1).min(25);
    let mut title = HString::<24>::new();
    let _ = write!(title, "Throw {}/25", round);
    render_header(display, title.as_str(), palette::surface_high());

    let complete = state.dice_rolls_complete();
    let action = if complete {
        "DICE COMPLETE"
    } else if phase == 0 {
        if roll_count == 0 {
            "ROLL 4 DICE"
        } else {
            "ROLL AGAIN"
        }
    } else {
        match phase {
            1 => "ENTER DIE B",
            2 => "ENTER DIE C",
            _ => "ENTER DIE D",
        }
    };
    let center_x = (SCREEN_WIDTH / 2) as i32;
    let _ = Text::with_alignment(
        action,
        Point::new(center_x, header_height() + 25),
        MonoTextStyle::new(&FONT_10X20, palette::text()),
        Alignment::Center,
    )
    .draw(display);

    let mut detail = HString::<24>::new();
    let rolls = state.dice_rolls();
    if complete {
        let _ = detail.push_str("PRESS DONE");
    } else if phase == 0 {
        let _ = detail.push_str("THEN ENTER A B C D");
    } else {
        let start = roll_count - phase;
        for index in 0..4 {
            if index > 0 {
                let _ = detail.push(' ');
            }
            let _ = detail.push(char::from(b'A' + index as u8));
            let value = rolls.get(start + index).copied();
            let _ = detail.push(value.map_or('-', |roll| char::from(b'0' + roll)));
        }
    }
    let _ = Text::with_alignment(
        detail.as_str(),
        Point::new(center_x, header_height() + 50),
        MonoTextStyle::new(&FONT_8X13, palette::text_subtle()),
        Alignment::Center,
    )
    .draw(display);

    for hit in dice_buttons() {
        draw_dice_button(display, hit, state, false);
    }
    for hit in dice_control_buttons() {
        draw_dice_button(display, hit, state, false);
    }
}

pub fn button_from_point_seed_dice(point: Point, state: &SeedEntryState) -> Option<ButtonHit> {
    dice_buttons()
        .into_iter()
        .chain(dice_control_buttons())
        .find(|hit| {
            let enabled = hit.button != Button::Seed(SeedButton::Finish)
                || state.dice_rolls_complete();
            enabled && within_hit(hit, point, 4)
        })
}

fn dice_buttons() -> HVec<ButtonHit, 6> {
    let mut out = HVec::new();
    let margin = 4;
    let gap = 4;
    let width = (SCREEN_WIDTH as i32 - margin * 2 - gap * 2) / 3;
    let height = 58;
    let top = header_height() + 70;
    for face in 1..=6u8 {
        let index = face as i32 - 1;
        let row = index / 3;
        let col = index % 3;
        let _ = out.push(ButtonHit {
            button: Button::Seed(SeedButton::DiceFace(face)),
            top_left: Point::new(
                margin + col * (width + gap),
                top + row * (height + gap),
            ),
            size: Size::new(width as u32, height as u32),
        });
    }
    out
}

fn dice_control_buttons() -> [ButtonHit; 3] {
    let bottom_y = SCREEN_HEIGHT as i32 - 50;
    let control_gap = 4;
    let control_width = (SCREEN_WIDTH as i32 - 8 - control_gap * 2) / 3;
    let buttons = [
        SeedButton::Cancel,
        SeedButton::Backspace,
        SeedButton::Finish,
    ];
    core::array::from_fn(|col| ButtonHit {
        button: Button::Seed(buttons[col]),
        top_left: Point::new(4 + col as i32 * (control_width + control_gap), bottom_y),
        size: Size::new(control_width as u32, 46),
    })
}

fn draw_dice_button(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    state: &SeedEntryState,
    active: bool,
) {
    draw_button_frame(display, hit, active);
    let (label, enabled) = match hit.button {
        Button::Seed(SeedButton::DiceFace(1)) => ("1", true),
        Button::Seed(SeedButton::DiceFace(2)) => ("2", true),
        Button::Seed(SeedButton::DiceFace(3)) => ("3", true),
        Button::Seed(SeedButton::DiceFace(4)) => ("4", true),
        Button::Seed(SeedButton::DiceFace(5)) => ("5", true),
        Button::Seed(SeedButton::DiceFace(6)) => ("6", true),
        Button::Seed(SeedButton::Cancel) => ("BACK", true),
        Button::Seed(SeedButton::Backspace) => ("DEL", true),
        Button::Seed(SeedButton::Finish) => ("DONE", state.dice_rolls_complete()),
        _ => ("", false),
    };
    if label.is_empty() {
        return;
    }
    if enabled {
        draw_fitted_button_label(display, hit, label);
    } else {
        let center = Point::new(
            hit.top_left.x + hit.size.width as i32 / 2,
            hit.top_left.y + hit.size.height as i32 / 2 + 4,
        );
        let _ = Text::with_alignment(
            label,
            center,
            MonoTextStyle::new(&FONT_8X13, palette::text_subtle()),
            Alignment::Center,
        )
        .draw(display);
    }
}

pub fn render_seed_verify(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let _ = display.clear(palette::background());
    let Some((step, word_number)) = state.verification_prompt() else {
        render_header(display, "Backup Check", palette::surface_high());
        return;
    };
    let mut title = HString::<24>::new();
    let _ = write!(title, "Check {}/{}", step, SEED_VERIFY_WORDS);
    render_header(display, title.as_str(), palette::surface_high());

    let mut prompt = HString::<24>::new();
    let _ = write!(prompt, "Select word #{}", word_number);
    let _ = Text::with_alignment(
        prompt.as_str(),
        Point::new((SCREEN_WIDTH / 2) as i32, header_height() + 29),
        MonoTextStyle::new(&FONT_10X20, palette::text()),
        Alignment::Center,
    )
    .draw(display);

    for hit in verify_buttons() {
        draw_verify_button(display, hit, state, false);
    }
}

pub fn button_from_point_seed_verify(point: Point) -> Option<ButtonHit> {
    verify_buttons()
        .into_iter()
        .find(|hit| within_hit(hit, point, 4))
}

fn verify_buttons() -> [ButtonHit; 4] {
    let margin = 10;
    let width = SCREEN_WIDTH as i32 - margin * 2;
    let height = 40;
    let top = header_height() + 46;
    [
        ButtonHit {
            button: Button::Seed(SeedButton::VerifyChoice(0)),
            top_left: Point::new(margin, top),
            size: Size::new(width as u32, height as u32),
        },
        ButtonHit {
            button: Button::Seed(SeedButton::VerifyChoice(1)),
            top_left: Point::new(margin, top + 46),
            size: Size::new(width as u32, height as u32),
        },
        ButtonHit {
            button: Button::Seed(SeedButton::VerifyChoice(2)),
            top_left: Point::new(margin, top + 92),
            size: Size::new(width as u32, height as u32),
        },
        ButtonHit {
            button: Button::Seed(SeedButton::Cancel),
            top_left: Point::new(46, SCREEN_HEIGHT as i32 - 42),
            size: Size::new((SCREEN_WIDTH - 92) as u32, 36),
        },
    ]
}

fn draw_verify_button(
    display: &mut GuiDisplay<'_>,
    hit: ButtonHit,
    state: &SeedEntryState,
    active: bool,
) {
    let label = match hit.button {
        Button::Seed(SeedButton::VerifyChoice(choice)) => {
            state.verification_choice(choice as usize).unwrap_or("")
        }
        Button::Seed(SeedButton::Cancel) => "BACK",
        _ => "",
    };
    draw_text_button(display, hit, label, active);
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
            let digit_style = MonoTextStyle::new(&FONT_10X20, palette::text());
            let letters_style = MonoTextStyle::new(&FONT_6X10, palette::text()); // Bright, not subtle!
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
        SeedButton::GenerateSeed
        | SeedButton::DiceSeed
        | SeedButton::DiceFace(_)
        | SeedButton::VerifyChoice(_) => {}
    }
}

fn draw_label(display: &mut GuiDisplay<'_>, x: i32, y: i32, label: &str) {
    let style = MonoTextStyle::new(&FONT_10X20, palette::text());
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
                    .fill_color(palette::panel_shadow())
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
                    palette::text()
                } else {
                    palette::panel_highlight()
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
                        palette::text()
                    } else {
                        palette::panel_highlight()
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
                        .fill_color(palette::text())
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
            base: palette::keypad_active(),
            light: palette::keypad_active_light(),
            dark: palette::keypad_active_dark(),
            border: palette::keypad_border(),
        }
    } else {
        Palette {
            base: palette::keypad_idle(),
            light: palette::btn_disabled_light(),
            dark: palette::btn_disabled_dark(),
            border: palette::keypad_border(),
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

fn word_index(word: &str) -> Option<usize> {
    WORDLIST.lines().position(|candidate| candidate == word)
}

pub fn render_seed_confirm(display: &mut GuiDisplay<'_>, state: &SeedEntryState) {
    let _ = display.clear(palette::background());
    // For a generated seed the user is seeing it for the first time and must
    // copy it down; for a typed-in seed this is a read-back confirmation.
    let title = if state.verification_failed() {
        "Check Backup"
    } else if state.is_dice_generated() {
        "Dice Seed"
    } else if state.is_generated() {
        "Write It Down"
    } else {
        "Confirm Seed"
    };
    render_header(display, title, palette::surface_high());

    // Two columns (1-12 | 13-24) so the full 24-word phrase fits on screen.
    let text_style = MonoTextStyle::new(&FONT_6X10, palette::text());
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
                .fill_color(palette::panel_shadow())
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
                .fill_color(palette::surface_low())
                .stroke_color(palette::divider())
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
            .stroke_color(palette::panel_highlight())
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
            .stroke_color(palette::divider())
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
        let style = MonoTextStyle::new(&FONT_10X20, palette::text());
        let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
        let _ = Text::with_alignment(
            label,
            Point::new(center.x, baseline),
            style,
            Alignment::Center,
        )
        .draw(display);
    } else {
        let style = MonoTextStyle::new(&FONT_8X13, palette::text());
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
