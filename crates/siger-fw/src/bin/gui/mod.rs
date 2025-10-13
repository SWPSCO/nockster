use core::fmt::Write as _;

mod constants;
mod layout;
mod render;
mod state;
mod touch;
pub mod demo;

pub use state::{GuiInteraction, GuiMode};
pub use touch::ScreenPoint;

use constants::*;
use embedded_graphics::{draw_target::DrawTarget, prelude::Point};
use layout::{button_from_point_confirm, button_from_point_keypad, confirm_buttons};
use render::{
    blit_boot_logo, draw_button, draw_centered_message, draw_keypad, draw_unlock_spinner_frame,
    render_header,
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
    time::Instant,
    Blocking,
};
use heapless::{String as HString, Vec as HVec};
use mipidsi::{
    error::InitError as DisplayInitError, models::ST7789, options::Orientation,
    Builder as DisplayBuilder, Display,
};
use siger_fw::axs5106l::{Axs5106l, Rotation};

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
    unlock_anim: u16,
    current_spinner_frame: u8,
    confirm_result: Option<bool>,
    interaction: InteractionState,
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
            unlock_anim: 0,
            current_spinner_frame: 0,
            confirm_result: None,
            interaction: InteractionState::default(),
        };

        blit_boot_logo(&mut gui.display);

        Ok(gui)
    }

    pub fn get_display_mut(&mut self) -> &mut GuiDisplay<'d> {
        &mut self.display
    }

    pub fn tick(&mut self) -> Option<GuiInteraction> {
        self.advance_unlocking();

        let now = Instant::now();
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
        draw_keypad(&mut self.display);
        self.render_pin_header();
    }

    pub fn show_unlocking(&mut self) {
        self.disarm_active();
        self.mode = GuiMode::Unlocking;
        self.unlock_anim = 0;
        self.current_spinner_frame = 0;
        let _ = self.display.clear(COLOR_BACKGROUND);
        render_header(&mut self.display, "Unlocking...", COLOR_BACKGROUND);
        draw_unlock_spinner_frame(&mut self.display, 0);
    }

    pub fn show_unlock_success(&mut self) {
        self.disarm_active();
        self.mode = GuiMode::Unlocked;
        let _ = self.display.clear(COLOR_UNLOCK_BG);
        draw_centered_message(&mut self.display, "Unlocked");
          // let mut frame = 0u32;
          // loop {
          //     let _ = demo::render_frame_bulk(&mut self.display, frame);
          //     frame = frame.wrapping_add(1);
          //     let delay = Delay::new();
          //     delay.delay_millis(33u32);
          // }
    }

    pub fn show_idle_message(&mut self, text: &str) {
        self.disarm_active();
        self.mode = GuiMode::Unlocked;
        let _ = self.display.clear(COLOR_UNLOCK_BG);
        draw_centered_message(&mut self.display, text);
    }

    pub fn show_pin_failure(&mut self, attempts_remaining: Option<u8>) {
        self.disarm_active();
        self.pin_entered.clear();
        self.mode = GuiMode::Locked;
        draw_keypad(&mut self.display);
        let mut msg = HString::<32>::new();
        let _ = msg.push_str("Bad PIN");
        if let Some(remaining) = attempts_remaining {
            let _ = write!(msg, " ({} left)", remaining);
        }
        render_header(&mut self.display, msg.as_str(), COLOR_BUTTON_ACTIVE);
    }

    pub fn show_pin_locked_out(&mut self) {
        self.disarm_active();
        self.pin_entered.clear();
        self.mode = GuiMode::Error;
        let _ = self.display.clear(COLOR_BACKGROUND);
        draw_centered_message(&mut self.display, "Lockout :(");
    }

    pub fn show_pin_not_initialized(&mut self) {
        self.disarm_active();
        self.pin_entered.clear();
        self.mode = GuiMode::Error;
        let _ = self.display.clear(COLOR_BACKGROUND);
        draw_centered_message(&mut self.display, "PIN Not Set");
    }

    pub fn poll_confirmation_result(&mut self) -> Option<bool> {
        self.confirm_result.take()
    }

    pub fn request_confirmation(&mut self, prompt: &str) {
        self.confirm_prompt.clear();
        let _ = self.confirm_prompt.push_str(prompt);
        self.disarm_active();
        self.confirm_result = None;
        self.mode = GuiMode::Confirm;
        let _ = self.display.clear(COLOR_BACKGROUND);
        render_header(
            &mut self.display,
            self.confirm_prompt.as_str(),
            COLOR_BACKGROUND,
        );
        for hit in confirm_buttons() {
            draw_button(&mut self.display, self.mode, hit, false);
        }
    }

    fn process_touch_point(&mut self, now: Instant, point: ScreenPoint) {
        let candidate = match self.mode {
            GuiMode::Locked => button_from_point_keypad(Point::new(point.x as i32, point.y as i32)),
            GuiMode::Confirm => {
                button_from_point_confirm(Point::new(point.x as i32, point.y as i32))
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
                    return self.finalize_press(now);
                }
            }
            return None;
        }

        self.disarm_active();
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
        draw_button(&mut self.display, self.mode, hit, true);
        self.interaction.active_button = Some(hit);
        self.interaction.active_seen_at = Some(now);
        self.interaction.press_started_at = Some(now);
    }

    fn deactivate_button(&mut self) {
        if let Some(old) = self.interaction.active_button.take() {
            draw_button(&mut self.display, self.mode, old, false);
        }
    }

    fn disarm_active(&mut self) {
        self.deactivate_button();
        self.clear_pending();
        self.interaction.press_started_at = None;
        self.interaction.active_seen_at = None;
        self.interaction.cooldown_until = None;
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
        }
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
        self.unlock_anim = self.unlock_anim.wrapping_add(1);
        if self.unlock_anim % 8 == 0 {
            let frame = ((self.unlock_anim / 8) % SPINNER_FRAMES.len() as u16) as u8;
            if frame != self.current_spinner_frame {
                self.current_spinner_frame = frame;
                draw_unlock_spinner_frame(&mut self.display, frame);
            }
        }
    }
}
