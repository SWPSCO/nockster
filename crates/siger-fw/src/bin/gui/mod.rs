use core::fmt::Write as _;

mod constants;
mod layout;
mod render;
mod seed;
mod state;
mod touch;
pub mod demo;

pub use seed::{SeedInteraction, SeedPhrase, SeedWord};
pub use state::{GuiInteraction, GuiMode};
pub use touch::ScreenPoint;

use constants::*;
use embedded_graphics::{draw_target::DrawTarget, prelude::Point};
use layout::{button_from_point_confirm, button_from_point_keypad, header_height, lock_button_rect};
use render::{
    blit_boot_logo, clear_idle_overlay, draw_button, draw_centered_message, draw_keypad,
    draw_unlock_header, draw_unlock_spinner_frame, render_confirm_overlay, render_header,
    render_idle_overlay,
};
use state::{Button, ButtonHit, InteractionState};
use touch::transform_raw_touch;
use display_interface_spi::SPIInterface;
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, ConfigError as I2cConfigError, Error as I2cBusError, I2c},
    peripherals::{
        GPIO21, GPIO38, GPIO39, GPIO40, GPIO41, GPIO42, GPIO45, GPIO46, GPIO47, GPIO48, I2C0, SPI2,
    },
    spi::{
        master::{Config as SpiConfig, ConfigError as SpiConfigError, Spi},
        Mode,
    },
    time::{Instant, Duration},
    Blocking,
};
use heapless::{String as HString, Vec as HVec};
use mipidsi::{
    error::InitError as DisplayInitError, models::ST7789, options::Orientation,
    Builder as DisplayBuilder, Display,
};
use siger_fw::axs5106l::{Axs5106l, Rotation};

const UNLOCK_DEMO_MAX_FRAMES: Option<u32> = None; // you can limit demo frames here eg Some(180)

type DisplaySpi<'d> = ExclusiveDevice<Spi<'d, Blocking>, Output<'d>, NoDelay>;
type DisplayInterface<'d> = SPIInterface<DisplaySpi<'d>, Output<'d>>;
pub type GuiDisplay<'d> = Display<DisplayInterface<'d>, ST7789, Output<'d>>;
type TouchDriver<'d> = Axs5106l<I2c<'d, Blocking>, Output<'d>>;

#[derive(Debug)]
pub enum GuiError {
    Spi(SpiConfigError),
    Display(DisplayInitError<core::convert::Infallible>),
    I2cConfig(I2cConfigError),
    TouchInit(siger_fw::axs5106l::Axs5106lError<I2cBusError>),
}

pub struct Gui<'d> {
    display: GuiDisplay<'d>,
    backlight: Output<'d>,
    touch: TouchDriver<'d>,
    touch_int: Input<'d>,
    mode: GuiMode,
    pin_expected: Option<u8>,
    pin_entered: HVec<u8, PIN_BUFFER_LEN>,
    confirm_prompt: HString<64>,
    idle_message: HString<48>,
    unlock_anim: u16,
    unlocking_started_at: Option<Instant>,
    current_spinner_frame: u8,
    confirm_result: Option<bool>,
    interaction: InteractionState,
    unlock_demo_state: Option<demo::AnimationState>,
    unlock_demo_last_frame_start: Option<Instant>,
    unlock_demo_frames_rendered: u32,
    unlock_demo_paused: bool,
    idle_message_until: Option<Instant>,
    lock_button_active: bool,
    lock_button_pressed_at: Option<Instant>,
    header_dirty: bool,
    overlay_dirty: bool,
    auto_lock_deadline: Option<Instant>,
    seed_entry_state: seed::SeedEntryState,
}

impl<'d> Gui<'d> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi: SPI2<'d>,
        sclk: GPIO38<'d>,
        mosi: GPIO39<'d>,
        dc: GPIO45<'d>,
        cs: GPIO21<'d>,
        rst: GPIO40<'d>,
        backlight_pin: GPIO46<'d>,
        i2c: I2C0<'d>,
        touch_scl: GPIO41<'d>,
        touch_sda: GPIO42<'d>,
        touch_reset: GPIO47<'d>,
        touch_int: GPIO48<'d>,
        delay: &mut Delay,
    ) -> Result<Self, GuiError> {
        let spi_cfg = SpiConfig::default()
            .with_frequency(esp_hal::time::Rate::from_mhz(40))
            .with_mode(Mode::_0);
        let spi = Spi::new(spi, spi_cfg)
            .map_err(GuiError::Spi)?
            .with_sck(sclk)
            .with_mosi(mosi);

        let dc = Output::new(dc, Level::Low, OutputConfig::default());
        let cs = Output::new(cs, Level::High, OutputConfig::default());
        let rst = Output::new(rst, Level::High, OutputConfig::default());
        let mut backlight = Output::new(backlight_pin, Level::Low, OutputConfig::default());

        let spi_device = ExclusiveDevice::new_no_delay(spi, cs);
        let di = SPIInterface::new(spi_device, dc);

        let display = DisplayBuilder::new(ST7789, di)
            .orientation(Orientation::default().flip_horizontal())
            .display_size(SCREEN_WIDTH, SCREEN_HEIGHT)
            .display_offset(DISPLAY_X_OFFSET, 0)
            .reset_pin(rst)
            .init(delay)
            .map_err(GuiError::Display)?;

        backlight.set_high();

        let i2c_cfg = I2cConfig::default().with_frequency(esp_hal::time::Rate::from_khz(10));
        let i2c = I2c::new(i2c, i2c_cfg)
            .map_err(GuiError::I2cConfig)?
            .with_scl(touch_scl)
            .with_sda(touch_sda);

        let touch_reset = Output::new(touch_reset, Level::High, OutputConfig::default());
        let mut touch = Axs5106l::new(
            i2c,
            touch_reset,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            Rotation::Rotate180,
        );
        touch.init(delay).map_err(GuiError::TouchInit)?;

        let touch_int_cfg = InputConfig::default().with_pull(Pull::Up);
        let touch_int = Input::new(touch_int, touch_int_cfg);

        let mut gui = Self {
            display,
            backlight,
            touch,
            touch_int,
            mode: GuiMode::Splash,
            pin_expected: None,
            pin_entered: HVec::new(),
            confirm_prompt: HString::new(),
            idle_message: HString::new(),
            unlock_anim: 0,
            unlocking_started_at: None,
            current_spinner_frame: 0,
            confirm_result: None,
            interaction: InteractionState::default(),
            unlock_demo_state: None,
            unlock_demo_last_frame_start: None,
            unlock_demo_frames_rendered: 0,
            unlock_demo_paused: false,
            idle_message_until: None,
            lock_button_active: false,
            lock_button_pressed_at: None,
            header_dirty: true,
            overlay_dirty: false,
            auto_lock_deadline: None,
            seed_entry_state: seed::SeedEntryState::new(),
        };

        blit_boot_logo(&mut gui.display);

        Ok(gui)
    }

    pub fn get_display_mut(&mut self) -> &mut GuiDisplay<'d> {
        &mut self.display
    }

    pub fn tick(&mut self) -> Option<GuiInteraction> {
        self.advance_unlocking();
        self.advance_unlock_success();

        let now = Instant::now();

        if let Some(until) = self.idle_message_until {
            if now >= until {
                self.idle_message_until = None;
                if self.mode == GuiMode::Unlocked {
                    self.set_idle_message("");
                    self.mark_overlay_dirty();
                    self.render_current_overlay();
                }
            }
        }
        if matches!(self.mode, GuiMode::Unlocked | GuiMode::Confirm) {
            match self.auto_lock_deadline {
                Some(deadline) => {
                    if now >= deadline {
                        self.auto_lock_deadline = None;
                        self.lock_button_active = false;
                        self.lock_button_pressed_at = None;
                        self.mark_header_dirty();
                        self.render_unlock_header();
                        if self.mode == GuiMode::Unlocked {
                            self.stop_unlock_demo();
                        }
                        return Some(GuiInteraction::LockRequested);
                    }
                }
                None => self.refresh_auto_lock(now),
            }
        } else {
            self.clear_auto_lock();
        }
        if let Some(until) = self.interaction.cooldown_until {
            if now >= until {
                self.interaction.cooldown_until = None;
            }
        }

        let finger_present = self.touch_int.is_low();

        if finger_present {
            self.interaction.finger_down = true;
            self.interaction.last_touch_sample_at = Some(now);
            if let Some(point) = self.read_touch_point() {
                if self.interaction.cooldown_until.is_some() {
                    self.disarm_active();
                } else {
                    self.process_touch_point(now, point);
                }
                return Some(GuiInteraction::RawTouch(point));
            } else if self.interaction.active_button.is_some() {
                self.interaction.active_seen_at = Some(now);
            }
        } else if let Some(event) = self.process_no_touch(now) {
            return Some(event);
        }

        None
    }

    pub fn begin_unlock(&mut self, expected_digits: Option<u8>) {
        self.pin_expected = expected_digits;
        self.pin_entered.clear();
        self.mode = GuiMode::Locked;
        self.disarm_active();
        self.stop_unlock_demo();
        draw_keypad(&mut self.display);
        self.render_pin_header();
    }

    pub fn show_unlocking(&mut self) {
        self.disarm_active();
        self.stop_unlock_demo();
        self.mode = GuiMode::Unlocking;
        self.unlock_anim = 0;
        self.unlocking_started_at = Some(Instant::now());
        self.current_spinner_frame = 0;
        let _ = self.display.clear(COLOR_BACKGROUND);
        render_header(&mut self.display, "Unlocking...", COLOR_SURFACE_HIGH);
        draw_unlock_spinner_frame(&mut self.display, 0);
    }

    pub fn show_unlock_success(&mut self) {
        self.disarm_active();
        self.mode = GuiMode::Unlocked;
        self.restart_unlock_demo();
        self.lock_button_active = false;
        self.lock_button_pressed_at = None;
        self.idle_message_until = None;
        let _ = self.display.clear(COLOR_BACKGROUND);
        self.mark_header_dirty();
        self.render_unlock_header();
        self.set_idle_message("");
        self.mark_overlay_dirty();
        self.refresh_auto_lock(Instant::now());
        self.render_current_overlay();
    }

    fn advance_unlock_success(&mut self) {
        let (range_start, range_end) = self.demo_render_range();
        let clip_range = if range_start == 0 && range_end as usize == SCREEN_HEIGHT as usize {
            None
        } else {
            Some((range_start, range_end))
        };

        let Some(state) = self.unlock_demo_state.as_mut() else {
            return;
        };

        if self.unlock_demo_paused {
            self.render_current_overlay();
            return;
        }

        if state.is_frame_start() {
            let now = Instant::now();
            if let Some(last) = self.unlock_demo_last_frame_start {
                if now - last < Duration::from_millis(33) {
                    return;
                }
            }
            self.unlock_demo_last_frame_start = Some(now);
        }

        let frame_complete = match demo::render_next_chunk(&mut self.display, state, clip_range) {
            Ok(done) => done,
            Err(_) => false,
        };
        self.render_current_overlay();

        if frame_complete {
            self.unlock_demo_frames_rendered =
                self.unlock_demo_frames_rendered.saturating_add(1);
            if let Some(limit) = UNLOCK_DEMO_MAX_FRAMES {
                if self.unlock_demo_frames_rendered >= limit {
                    self.stop_unlock_demo();
                    self.render_current_overlay();
                }
            }
        }
    }

    fn stop_unlock_demo(&mut self) {
        self.unlock_demo_state = None;
        self.unlock_demo_last_frame_start = None;
        self.unlock_demo_frames_rendered = 0;
        self.unlock_demo_paused = false;
        self.idle_message.clear();
        self.mark_overlay_dirty();
        self.mark_header_dirty();
        self.clear_auto_lock();
    }

    fn restart_unlock_demo(&mut self) {
        self.unlock_demo_state = Some(demo::AnimationState::new());
        self.unlock_demo_last_frame_start = None;
        self.unlock_demo_frames_rendered = 0;
        self.unlock_demo_paused = false;
        self.mark_overlay_dirty();
        self.mark_header_dirty();
    }

    fn set_idle_message(&mut self, text: &str) {
        if self.idle_message.as_str() != text {
            self.idle_message.clear();
            let _ = self.idle_message.push_str(text);
            self.mark_overlay_dirty();
        }
    }

    fn render_current_overlay(&mut self) {
        if self.header_dirty && matches!(self.mode, GuiMode::Unlocked) {
            self.render_unlock_header();
        }
        if !self.overlay_dirty {
            return;
        }
        match self.mode {
            GuiMode::Unlocked => {
                if self.idle_message.is_empty() {
                    clear_idle_overlay(&mut self.display);
                } else {
                    render_idle_overlay(&mut self.display, self.idle_message.as_str());
                }
            }
            GuiMode::Confirm => {
                let subtitle = if self.idle_message.is_empty() {
                    None
                } else {
                    Some(self.idle_message.as_str())
                };
                render_confirm_overlay(
                    &mut self.display,
                    self.confirm_prompt.as_str(),
                    subtitle,
                    self.interaction.active_button.map(|hit| hit.button),
                );
            }
            _ => {}
        }
        self.overlay_dirty = false;
    }

    fn mark_overlay_dirty(&mut self) {
        self.overlay_dirty = true;
    }

    fn mark_header_dirty(&mut self) {
        self.header_dirty = true;
    }

    fn render_unlock_header(&mut self) {
        draw_unlock_header(&mut self.display, self.lock_button_active);
        self.header_dirty = false;
    }

    fn refresh_auto_lock(&mut self, now: Instant) {
        if matches!(self.mode, GuiMode::Unlocked | GuiMode::Confirm) {
            self.auto_lock_deadline = Some(now + AUTO_LOCK_TIMEOUT);
        }
    }

    fn clear_auto_lock(&mut self) {
        self.auto_lock_deadline = None;
    }

    fn clear_lock_button(&mut self) {
        if self.lock_button_active || self.lock_button_pressed_at.is_some() {
            self.lock_button_active = false;
            self.lock_button_pressed_at = None;
            self.mark_header_dirty();
            if matches!(self.mode, GuiMode::Unlocked) {
                self.render_unlock_header();
            }
        }
    }

    fn finalize_lock_button(&mut self, now: Instant) -> Option<GuiInteraction> {
        if let Some(start) = self.lock_button_pressed_at.take() {
            let was_active = self.lock_button_active;
            self.lock_button_active = false;
            self.mark_header_dirty();
            self.render_unlock_header();
            if was_active && now - start >= MIN_PRESS_DURATION {
                return Some(GuiInteraction::LockRequested);
            }
        }
        None
    }

    fn idle_overlay_top_row(&self) -> Option<u16> {
        if self.idle_message.is_empty() {
            return None;
        }
        let top = SCREEN_HEIGHT as i32 - IDLE_OVERLAY_MARGIN - IDLE_OVERLAY_HEIGHT;
        if top <= 0 {
            None
        } else {
            Some(top as u16)
        }
    }

    fn demo_render_range(&self) -> (u16, u16) {
        let mut start: u16 = if matches!(self.mode, GuiMode::Unlocked | GuiMode::Confirm) {
            header_height().max(0) as u16
        } else {
            0
        };
        let mut end: u16 = SCREEN_HEIGHT;
        if self.mode == GuiMode::Unlocked {
            if let Some(bottom) = self.idle_overlay_top_row() {
                end = end.min(bottom);
            }
        }
        if start > end {
            start = end;
        }
        (start, end)
    }

    pub fn show_idle_message(&mut self, text: &str) {
        self.show_idle_message_with_timeout(text, None);
    }

    pub fn show_idle_message_timed(&mut self, text: &str, timeout: Duration) {
        self.show_idle_message_with_timeout(text, Some(timeout));
    }

    fn show_idle_message_with_timeout(&mut self, text: &str, timeout: Option<Duration>) {
        self.disarm_active();
        self.mode = GuiMode::Unlocked;
        if self.unlock_demo_state.is_none() {
            self.restart_unlock_demo();
            let _ = self.display.clear(COLOR_BACKGROUND);
        }
        self.unlock_demo_paused = false;
        self.mark_header_dirty();
        self.render_unlock_header();
        self.idle_message_until = timeout.map(|d| Instant::now() + d);
        self.set_idle_message(text);
        self.refresh_auto_lock(Instant::now());
        self.render_current_overlay();
    }

    pub fn show_pin_failure(&mut self, attempts_remaining: Option<u8>) {
        self.disarm_active();
        self.stop_unlock_demo();
        self.pin_entered.clear();
        self.mode = GuiMode::Locked;
        let _ = self.display.clear(COLOR_BACKGROUND);
        draw_keypad(&mut self.display);
        let mut msg = HString::<32>::new();
        let _ = msg.push_str("Bad PIN");
        if let Some(remaining) = attempts_remaining {
            let _ = write!(msg, " ({} left)", remaining);
        }
        render_header(&mut self.display, msg.as_str(), COLOR_SURFACE_HIGH);
    }

    pub fn show_pin_locked_out(&mut self) {
        self.disarm_active();
        self.stop_unlock_demo();
        self.pin_entered.clear();
        self.mode = GuiMode::Error;
        let _ = self.display.clear(COLOR_BACKGROUND);
        render_header(&mut self.display, "Locked Out", COLOR_SURFACE_HIGH);
        draw_centered_message(&mut self.display, "Lockout :(");
    }

    pub fn show_pin_not_initialized(&mut self) {
        self.disarm_active();
        self.stop_unlock_demo();
        self.pin_entered.clear();
        self.mode = GuiMode::Error;
        let _ = self.display.clear(COLOR_BACKGROUND);
        render_header(&mut self.display, "PIN Required", COLOR_SURFACE_HIGH);
        draw_centered_message(&mut self.display, "PIN Not Set");
    }

    pub fn show_seed_setup(&mut self) {
        self.disarm_active();
        self.stop_unlock_demo();
        self.seed_entry_state.reset();
        self.mode = GuiMode::SeedFirstBoot;
        seed::render_seed_setup(&mut self.display);
    }

    pub fn show_seed_entry(&mut self) {
        self.disarm_active();
        self.stop_unlock_demo();
        self.mode = GuiMode::SeedEntry;
        seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
    }

    pub fn poll_confirmation_result(&mut self) -> Option<bool> {
        self.confirm_result.take()
    }

    pub fn request_confirmation(&mut self, prompt: &str) {
        self.confirm_prompt.clear();
        let _ = self.confirm_prompt.push_str(prompt);
        self.mark_overlay_dirty();
        self.disarm_active();
        self.confirm_result = None;
        self.mode = GuiMode::Confirm;
        if self.unlock_demo_state.is_none() {
            self.restart_unlock_demo();
            let _ = self.display.clear(COLOR_BACKGROUND);
        }
        self.unlock_demo_paused = true;
        render_header(&mut self.display, "Confirm", COLOR_SURFACE_HIGH);
        self.set_idle_message("");
        self.render_current_overlay();
        self.refresh_auto_lock(Instant::now());
    }

    fn process_touch_point(&mut self, now: Instant, point: ScreenPoint) {
        self.refresh_auto_lock(now);
        if self.mode == GuiMode::Unlocked {
            let rect = lock_button_rect();
            let pt = Point::new(point.x as i32, point.y as i32);
            if rect.contains(pt) {
                if !self.lock_button_active {
                    self.lock_button_active = true;
                    if self.lock_button_pressed_at.is_none() {
                        self.lock_button_pressed_at = Some(now);
                    }
                    self.mark_header_dirty();
                    self.render_unlock_header();
                }
            } else {
                if self.lock_button_active || self.lock_button_pressed_at.is_some() {
                    self.lock_button_active = false;
                    self.lock_button_pressed_at = None;
                    self.mark_header_dirty();
                    self.render_unlock_header();
                }
            }
            return;
        }

        let candidate = match self.mode {
            GuiMode::Locked => button_from_point_keypad(Point::new(point.x as i32, point.y as i32)),
            GuiMode::Confirm => {
                button_from_point_confirm(Point::new(point.x as i32, point.y as i32))
            }
            GuiMode::SeedFirstBoot => {
                seed::button_from_point_seed_setup(Point::new(point.x as i32, point.y as i32))
            }
            GuiMode::SeedEntry => {
                seed::button_from_point_seed_entry(Point::new(point.x as i32, point.y as i32))
            }
            GuiMode::SeedConfirm => {
                seed::button_from_point_seed_confirm(Point::new(point.x as i32, point.y as i32))
            }
            _ => None,
        };

        if let Some(active) = self.interaction.active_button {
            if Some(active) == candidate {
                self.interaction.active_seen_at = Some(now);
            } else if let Some(seen) = self.interaction.active_seen_at {
                if now - seen >= BUTTON_INACTIVE_GRACE {
                    self.deactivate_button();
                    self.interaction.press_started_at = None;
                    self.interaction.active_seen_at = None;
                }
            }
        }

        match candidate {
            Some(hit) => {
                if let Some(active) = self.interaction.active_button {
                    if active == hit {
                        if self.interaction.press_started_at.is_none() {
                            self.interaction.press_started_at = Some(now);
                        }
                        return;
                    }
                    self.deactivate_button();
                    self.interaction.press_started_at = None;
                    self.interaction.active_seen_at = None;
                }

                if let Some(pending) = self.interaction.pending_hit {
                    if pending == hit {
                        if let Some(since) = self.interaction.pending_since {
                            if now - since >= BUTTON_STABLE_DURATION {
                                self.activate_button(hit, now);
                                self.clear_pending();
                            }
                        }
                    } else {
                        self.interaction.pending_hit = Some(hit);
                        self.interaction.pending_since = Some(now);
                    }
                } else {
                    self.interaction.pending_hit = Some(hit);
                    self.interaction.pending_since = Some(now);
                }
            }
            None => {
                self.clear_pending();
                if let Some(seen) = self.interaction.active_seen_at {
                    if now - seen >= BUTTON_INACTIVE_GRACE {
                        self.deactivate_button();
                        self.interaction.press_started_at = None;
                        self.interaction.active_seen_at = None;
                    }
                }
            }
        }
    }

    fn process_no_touch(&mut self, now: Instant) -> Option<GuiInteraction> {
        if self.interaction.finger_down {
            if let Some(last_seen) = self.interaction.last_touch_sample_at {
                if now - last_seen >= RELEASE_DEBOUNCE {
                    self.interaction.finger_down = false;
                    self.interaction.last_touch_sample_at = None;
                    self.clear_pending();
                    if self.interaction.cooldown_until.is_some() {
                        self.disarm_active();
                        return None;
                    }
                    if let Some(result) = self.finalize_press(now) {
                        return Some(result);
                    }
                    if self.mode == GuiMode::Unlocked {
                        return self.finalize_lock_button(now);
                    }
                    return None;
                }
            }
            return None;
        }

        self.disarm_active();
        if self.mode == GuiMode::Unlocked {
            if let Some(result) = self.finalize_lock_button(now) {
                return Some(result);
            }
        }
        None
    }

    fn finalize_press(&mut self, now: Instant) -> Option<GuiInteraction> {
        let active = self.interaction.active_button;
        let started = self.interaction.press_started_at;
        self.deactivate_button();
        self.interaction.press_started_at = None;
        self.interaction.active_seen_at = None;

        if let (Some(hit), Some(start)) = (active, started) {
            if now - start >= MIN_PRESS_DURATION {
                self.interaction.cooldown_until = Some(now + PRESS_COOLDOWN);
                return match self.mode {
                    GuiMode::Locked => self.handle_pin_button(hit.button),
                    GuiMode::Confirm => self.handle_confirm_button(hit.button),
                    GuiMode::SeedFirstBoot | GuiMode::SeedEntry | GuiMode::SeedConfirm => self.handle_seed_button(hit.button),
                    _ => None,
                };
            }
        }
        None
    }

    fn activate_button(&mut self, hit: ButtonHit, now: Instant) {
        if self.interaction.active_button == Some(hit) {
            self.interaction.active_seen_at = Some(now);
            return;
        }
        self.deactivate_button();
        match self.mode {
            GuiMode::Confirm => {
                self.interaction.active_button = Some(hit);
                self.interaction.active_seen_at = Some(now);
                self.interaction.press_started_at = Some(now);
                self.mark_overlay_dirty();
                self.render_current_overlay();
            }
            GuiMode::SeedFirstBoot | GuiMode::SeedEntry | GuiMode::SeedConfirm => {
                seed::draw_seed_button(&mut self.display, self.mode, hit, Some(&self.seed_entry_state), true);
                self.interaction.active_button = Some(hit);
                self.interaction.active_seen_at = Some(now);
                self.interaction.press_started_at = Some(now);
            }
            _ => {
                draw_button(&mut self.display, self.mode, hit, true);
                self.interaction.active_button = Some(hit);
                self.interaction.active_seen_at = Some(now);
                self.interaction.press_started_at = Some(now);
            }
        }
    }

    fn deactivate_button(&mut self) {
        if let Some(old) = self.interaction.active_button.take() {
            match self.mode {
                GuiMode::Confirm => {
                    self.mark_overlay_dirty();
                    self.render_current_overlay();
                }
                GuiMode::SeedFirstBoot | GuiMode::SeedEntry | GuiMode::SeedConfirm => {
                    seed::draw_seed_button(&mut self.display, self.mode, old, Some(&self.seed_entry_state), false);
                }
                _ => draw_button(&mut self.display, self.mode, old, false),
            }
        }
    }

    fn disarm_active(&mut self) {
        self.deactivate_button();
        self.clear_pending();
        self.interaction.press_started_at = None;
        self.interaction.active_seen_at = None;
        self.interaction.cooldown_until = None;
        self.clear_lock_button();
    }

    fn clear_pending(&mut self) {
        self.interaction.pending_hit = None;
        self.interaction.pending_since = None;
    }

    fn handle_pin_button(&mut self, button: Button) -> Option<GuiInteraction> {
        match button {
            Button::Digit(digit) => {
                if self.pin_entered.len() < MAX_PIN_DIGITS {
                    let _ = self.pin_entered.push(digit);
                    self.render_pin_header();
                    if let Some(expected) = self.pin_expected {
                        if self.pin_entered.len() as u8 == expected {
                            return Some(GuiInteraction::PinComplete(self.pin_entered.clone()));
                        }
                    }
                }
                None
            }
            Button::Clear => {
                self.pin_entered.clear();
                self.render_pin_header();
                None
            }
            Button::Ok => {
                if let Some(expected) = self.pin_expected {
                    if self.pin_entered.len() as u8 != expected {
                        self.show_pin_failure(None);
                        return None;
                    }
                } else if self.pin_entered.len() < 4 {
                    self.show_pin_failure(None);
                    return None;
                }
                Some(GuiInteraction::PinComplete(self.pin_entered.clone()))
            }
            Button::Seed(_) => None,
        }
    }

    fn handle_confirm_button(&mut self, button: Button) -> Option<GuiInteraction> {
        match button {
            Button::Ok => {
                self.confirm_result = Some(true);
                Some(GuiInteraction::ConfirmAccepted)
            }
            Button::Clear => {
                self.confirm_result = Some(false);
                Some(GuiInteraction::ConfirmRejected)
            }
            Button::Digit(_) => None,
            Button::Seed(_) => None,
        }
    }

    fn handle_seed_button(&mut self, button: Button) -> Option<GuiInteraction> {
        use seed::{SeedButton, SeedInteraction};

        let seed_button = match button {
            Button::Seed(sb) => sb,
            _ => return None,
        };

        let interaction = match seed_button {
            SeedButton::EnterSeed => {
                self.show_seed_entry();
                SeedInteraction::EnterSeedRequested
            }
            SeedButton::Key(digit) => {
                if self.seed_entry_state.push_digit(digit) {
                    seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
                }
                return None;
            }
            SeedButton::Backspace => {
                let removed = self.seed_entry_state.backspace();
                seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
                SeedInteraction::WordRemoved(removed)
            }
            SeedButton::NextSuggestion => {
                if self.seed_entry_state.next_suggestion() {
                    seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
                }
                return None;
            }
            SeedButton::PrevSuggestion => {
                if self.seed_entry_state.prev_suggestion() {
                    seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
                }
                return None;
            }
            SeedButton::CommitWord => {
                if let Some(word) = self.seed_entry_state.commit_current() {
                    seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
                    SeedInteraction::WordCommitted(word)
                } else {
                    return None;
                }
            }
            SeedButton::Finish => {
                // If we're in entry mode, show confirmation screen
                if self.mode == GuiMode::SeedEntry {
                    if self.seed_entry_state.finish().is_some() {
                        self.mode = GuiMode::SeedConfirm;
                        seed::render_seed_confirm(&mut self.display, &self.seed_entry_state);
                        return None;
                    } else {
                        return None;
                    }
                }
                // If we're in confirm mode, actually finish
                if self.mode == GuiMode::SeedConfirm {
                    if let Some(phrase) = self.seed_entry_state.finish() {
                        SeedInteraction::EntryCompleted(phrase)
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            SeedButton::Cancel => {
                // If we're in confirm mode, go back to entry
                if self.mode == GuiMode::SeedConfirm {
                    self.mode = GuiMode::SeedEntry;
                    seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
                    return None;
                }
                // Otherwise go back to setup screen
                self.show_seed_setup();
                SeedInteraction::EntryCancelled
            }
        };

        Some(GuiInteraction::Seed(interaction))
    }

    fn read_touch_point(&mut self) -> Option<ScreenPoint> {
        match self.touch.read_raw_touch_data() {
            Ok((raw_data, _)) if raw_data.count > 0 => {
                raw_data.points.first().copied().map(transform_raw_touch)
            }
            _ => None,
        }
    }

    fn render_pin_header(&mut self) {
        let mut header = HString::<32>::new();
        let _ = header.push_str("PIN: ");
        for _ in 0..self.pin_entered.len() {
            let _ = header.push('*');
        }
        render_header(&mut self.display, header.as_str(), COLOR_BACKGROUND);
    }

    fn advance_unlocking(&mut self) {
        if self.mode != GuiMode::Unlocking {
            return;
        }

        let Some(started_at) = self.unlocking_started_at else {
            return;
        };

        // 5 fps = 200ms per frame
        let now = Instant::now();
        if now < started_at {
            return;
        }
        let elapsed = now - started_at;
        let elapsed_ms = elapsed.as_micros() / 1000;
        let frame_index = (elapsed_ms / 200) as usize;
        let frame = (frame_index % SPINNER_FRAMES.len()) as u8;

        if frame != self.current_spinner_frame {
            self.current_spinner_frame = frame;
            draw_unlock_spinner_frame(&mut self.display, frame);
        }
    }
}
