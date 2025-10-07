use core::{convert::Infallible, fmt::Write};
use axs5106l::{Axs5106l, Coordinates, Rotation as TouchRotation};
use display_interface_spi::SPIInterface;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text},
};
use embedded_graphics_core::pixelcolor::{raw::RawU16, Rgb565};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    peripherals::{
        GPIO21, GPIO38, GPIO39, GPIO40, GPIO41, GPIO42, GPIO45, GPIO46, GPIO47, GPIO48, I2C0, SPI2,
    },
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::Rate,
    Blocking,
    usb_serial_jtag::UsbSerialJtag,
};
use heapless::{String, Vec};
use mipidsi::{models::ST7789, options::Orientation, Builder, Display};
include!(concat!(env!("OUT_DIR"), "/boot_logo.rs"));
/// Convenience alias for the SPI device wrapper we use for the LCD.
type DisplaySpiDevice =
    ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, embedded_hal_bus::spi::NoDelay>;
/// Convenience alias for the SPI interface exposed to the display driver.
type DisplayInterface = SPIInterface<DisplaySpiDevice, Output<'static>>;
/// Concrete display type for the ST7789 panel.
type LcdDisplay = Display<DisplayInterface, ST7789, Output<'static>>;
/// Convenience alias for the touch controller type.
type TouchController = Axs5106l<I2c<'static, Blocking>, Output<'static>>;
const PADDING: i32 = 6;
const MAX_PIN_DIGITS: usize = 12;
const DEBUG_BAR_HEIGHT: i32 = 6;
const COLOR_BACKGROUND: Rgb565 = Rgb565::BLACK;
const COLOR_BUTTON: Rgb565 = Rgb565::new(6, 12, 6);
const COLOR_BUTTON_BORDER: Rgb565 = Rgb565::new(2, 4, 2);
const COLOR_BUTTON_ACTIVE: Rgb565 = Rgb565::new(16, 32, 16);
const COLOR_TEXT: Rgb565 = Rgb565::WHITE;
const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];
const DRIVER_ROTATION: TouchRotation = TouchRotation::Rotate180; 
const TOUCH_SENSOR_WIDTH:  u16 = 240;
const TOUCH_SENSOR_HEIGHT: u16 = 320;
const TOUCH_SWAP_AXES: bool = false;           // we just arranged the driver so X is true-X
const LCD_FLIPPED_HORIZONTALLY: bool = false;   // you already flip the ST7789
const INVERT_GUI_Y: bool = false;

const SENSOR_W: i32 = 240;
const SENSOR_H: i32 = 320;

const SWAP_XY:  bool = false;
const MIRROR_X: bool = true;   // counter .flip_horizontal()
const MIRROR_Y: bool = false;

// visible window on the glass
const VX_MIN: i32 = 34;
const VX_MAX: i32 = 205; // inclusive (205-34+1 = 172)


pub struct Gui {
    display: LcdDisplay,
    backlight: Output<'static>,
    touch: Option<TouchController>,
    touch_irq: Option<Input<'static>>,
    touch_ready: bool,
    mode: GuiMode,
    pin_expected: Option<u8>,
    pin_entered: Vec<u8, MAX_PIN_DIGITS>,
    confirm_prompt: String<64>,
    touch_active: bool,
    active_button: Option<(usize, usize)>,
    unlock_anim: u16,
    current_spinner_frame: u8,
    confirm_result: Option<bool>,
    debug_flash: bool,
    debug_touch_raw: Option<(u16, u16)>,
}
/// UI mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiMode {
    Splash,
    Locked,
    Confirm,
    Unlocking,
    Unlocked,
    Error,
}
/// User interactions surfaced by the GUI layer.
#[derive(Debug, Clone)]
pub enum GuiInteraction {
    PinComplete(Vec<u8, MAX_PIN_DIGITS>),
    ConfirmAccepted,
    ConfirmRejected,
    RawTouch(Coordinates),
}
/// Setup errors emitted while building the GUI subsystem.
#[derive(Debug)]
pub enum GuiError {
    SpiConfig(esp_hal::spi::master::ConfigError),
    DisplayInit(mipidsi::error::InitError<Infallible>),
    I2cConfig(esp_hal::i2c::master::ConfigError),
}
#[derive(Clone, Copy, Debug)]
enum Button {
    Digit(u8),
    Clear,
    Ok,
}
struct ButtonHit {
    button: Button,
    row: usize,
    col: usize,
}

// running raw bounds learned at runtime
static mut RAW_X_MIN: i32 =  i32::MAX;
static mut RAW_X_MAX: i32 = -i32::MAX;
static mut RAW_Y_MIN: i32 =  i32::MAX;
static mut RAW_Y_MAX: i32 = -i32::MAX;

// optional: nudge bounds so the very first sample doesn’t make a zero span
const BOUNDS_PAD: i32 = 8;

#[inline]
fn learn_raw_bounds(rx: u16, ry: u16) {
    let (x, y) = (rx as i32, ry as i32);
    unsafe {
        if x < RAW_X_MIN { RAW_X_MIN = x; }
        if x > RAW_X_MAX { RAW_X_MAX = x; }
        if y < RAW_Y_MIN { RAW_Y_MIN = y; }
        if y > RAW_Y_MAX { RAW_Y_MAX = y; }
    }
}

#[inline]
fn scale_i32(v: i32, in_min: i32, in_max: i32, out_max: i32) -> i32 {
    // clamp + safe linear map [in_min..in_max] -> [0..out_max]
    if in_max <= in_min { return 0; }
    let v = v.clamp(in_min, in_max) - in_min;
    (v as i64 * out_max as i64 / (in_max - in_min) as i64) as i32
}


impl Gui {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi2: SPI2<'static>,
        sck: GPIO38<'static>,
        mosi: GPIO39<'static>,
        dc: GPIO45<'static>,
        cs: GPIO21<'static>,
        rst: GPIO40<'static>,
        backlight_pin: GPIO46<'static>,
        i2c0: I2C0<'static>,
        touch_scl: GPIO41<'static>,
        touch_sda: GPIO42<'static>,
        touch_rst: GPIO47<'static>,
        touch_int: GPIO48<'static>,
        delay: &mut Delay,
    ) -> Result<Self, GuiError> {
        let spi_cfg = SpiConfig::default()
            .with_frequency(Rate::from_mhz(60))
            .with_mode(Mode::_0);
        let spi = Spi::new(spi2, spi_cfg)
            .map_err(GuiError::SpiConfig)?
            .with_sck(sck)
            .with_mosi(mosi);
        let dc = Output::new(dc, Level::Low, OutputConfig::default());
        let cs = Output::new(cs, Level::High, OutputConfig::default());
        let rst = Output::new(rst, Level::High, OutputConfig::default());
        let mut backlight = Output::new(backlight_pin, Level::Low, OutputConfig::default());
        let device = ExclusiveDevice::new_no_delay(spi, cs);
        let interface = SPIInterface::new(device, dc);
        let mut display = Builder::new(ST7789, interface)
            .reset_pin(rst)
            .display_size(BOOT_LOGO_WIDTH, BOOT_LOGO_HEIGHT)
            .display_offset(34, 0)
            .orientation(Orientation::new().flip_horizontal())
            .init(delay)
            .map_err(GuiError::DisplayInit)?;
        let _ = backlight.set_high();
        let i2c = I2c::new(i2c0, I2cConfig::default())
            .map_err(GuiError::I2cConfig)?
            .with_scl(touch_scl)
            .with_sda(touch_sda);
        let touch_reset = Output::new(touch_rst, Level::High, OutputConfig::default());
        let mut touch_driver = Axs5106l::new(
            i2c,
            touch_reset,
            TOUCH_SENSOR_WIDTH,
            TOUCH_SENSOR_HEIGHT,
            DRIVER_ROTATION,
        );
        let mut touch_ready = false;
        for _ in 0..5 {
            if touch_driver.init(delay).is_ok() {
                touch_ready = true;
                break;
            }
            delay.delay_millis(100);
        }
        let touch = Some(touch_driver);
        let touch_irq = Some(Input::new(
            touch_int,
            InputConfig::default().with_pull(Pull::Up),
        ));
        let mut gui = Gui {
            display,
            backlight,
            touch,
            touch_irq,
            touch_ready,
            mode: GuiMode::Splash,
            pin_expected: None,
            pin_entered: Vec::new(),
            confirm_prompt: String::new(),
            touch_active: false,
            active_button: None,
            unlock_anim: 0,
            current_spinner_frame: 0,
            confirm_result: None,
            debug_flash: false,
            debug_touch_raw: None,
        };
        gui.show_boot_logo();
        Ok(gui)
    }
    pub fn show_boot_logo(&mut self) {
        self.active_button = None;
        self.touch_active = false;
        self.mode = GuiMode::Splash;
        self.blit_boot_logo();
    }
    pub fn begin_unlock(&mut self, expected_digits: Option<u8>) {
        self.pin_expected = expected_digits;
        self.pin_entered.clear();
        self.touch_active = false;
        self.active_button = None;
        self.mode = GuiMode::Locked;
        self.draw_pin_pad();
        self.render_pin_header();
    }
    pub fn show_unlocking(&mut self) {
        self.touch_active = false;
        self.active_button = None;
        self.mode = GuiMode::Unlocking;
        self.unlock_anim = 0;
        self.current_spinner_frame = 0;
        let _ = self.display.clear(COLOR_BACKGROUND);
        self.render_header("Unlocking...", COLOR_BACKGROUND);
        self.draw_unlock_spinner_frame(0);
    }
    pub fn show_unlock_success(&mut self) {
        self.touch_active = false;
        self.active_button = None;
        self.mode = GuiMode::Unlocked;
        let _ = self.display.clear(COLOR_BACKGROUND);
        self.draw_centered_message("Unlocked");
    }
    pub fn show_pin_failure(&mut self, attempts_remaining: Option<u8>) {
        self.touch_active = false;
        self.active_button = None;
        self.pin_entered.clear();
        self.mode = GuiMode::Locked;
        self.draw_pin_pad();
        let mut msg = String::<32>::new();
        let _ = msg.push_str("Wrong PIN");
        if let Some(rem) = attempts_remaining {
            let _ = write!(msg, " ({} left)", rem);
        }
        self.render_header(msg.as_str(), COLOR_BUTTON_ACTIVE);
    }
    pub fn show_pin_locked_out(&mut self) {
        self.touch_active = false;
        self.active_button = None;
        self.pin_entered.clear();
        self.mode = GuiMode::Error;
        let _ = self.display.clear(COLOR_BACKGROUND);
        self.draw_centered_message("PIN Locked Out");
    }
    pub fn show_pin_not_initialized(&mut self) {
        self.touch_active = false;
        self.active_button = None;
        self.pin_entered.clear();
        self.mode = GuiMode::Error;
        let _ = self.display.clear(COLOR_BACKGROUND);
        self.draw_centered_message("PIN Not Set");
    }
    pub fn poll_confirmation_result(&mut self) -> Option<bool> {
        self.confirm_result.take()
    }
    pub fn request_confirmation(&mut self, prompt: &str) {
        self.confirm_prompt.clear();
        let _ = self.confirm_prompt.push_str(prompt);
        self.touch_active = false;
        self.active_button = None;
        self.confirm_result = None;
        self.mode = GuiMode::Confirm;
        self.render_confirm_prompt();
    }
    pub fn tick(&mut self) -> Option<GuiInteraction> {
        self.advance_unlocking();
        match self.mode {
            GuiMode::Unlocking | GuiMode::Unlocked | GuiMode::Error | GuiMode::Splash => {
                return None
            }
            _ => {}
        }
        let Some(touch) = self.poll_touch_state() else {
            return None;
        };
        self.flash_touch_debug();
        match self.mode {
            GuiMode::Locked => self.handle_pin_touch(touch),
            GuiMode::Confirm => self.handle_confirm_touch(touch),
            GuiMode::Unlocking | GuiMode::Unlocked | GuiMode::Error | GuiMode::Splash => None,
        }
    }
    pub fn mode(&self) -> GuiMode {
        self.mode
    }
    fn poll_touch_state(&mut self) -> Option<Coordinates> {
        self.ensure_touch_ready();
        let touch = self.touch.as_mut()?;

        let irq_asserted = match &self.touch_irq {
            Some(irq) => {
                if irq.is_high() { // active-low: high => no touch
                    self.clear_touch_latch();
                    return None;
                }
                true
            }
            None => false,
        };
        if irq_asserted && self.touch_active {
            return None;
        }

        // >>> CHANGED: read raw, not get_touch_data() <<<
        let touch_present = match touch.read_raw_touch_data() {
            Ok(data) if data.count > 0 => data.first_touch(),
            _ => {
                self.clear_touch_latch();
                return None;
            }
        };

        let raw = match touch_present {
            Some(p) => p,
            None => {
                self.clear_touch_latch();
                return None;
            }
        };

        // store *true* raw
        self.debug_touch_raw = Some((raw.x, raw.y));

        // map full-range (no cropping)
        let mapped = map_touch_point_raw(raw);
        let mut msg = heapless::String::<48>::new();
        let _ = core::fmt::Write::write_fmt(&mut msg, format_args!("map {},{}  raw {},{}\r\n", mapped.x, mapped.y, raw.x, raw.y));
        if self.touch_active {
            None
        } else {
            self.touch_active = true;
            self.draw_touch_debug(&mapped);
            Some(mapped)
        }
    }

    fn ensure_touch_ready(&mut self) {
        if self.touch_ready {
            return;
        }
        if let Some(touch) = self.touch.as_mut() {
            let mut delay = Delay::new();
            if touch.init(&mut delay).is_ok() {
                self.touch_ready = true;
            }
        }
    }
    pub fn take_debug_touch_raw(&mut self) -> Option<(u16, u16)> {
        self.debug_touch_raw.take()
    }
    fn handle_pin_touch(&mut self, coord: Coordinates) -> Option<GuiInteraction> {
        let hit = match self.button_from_point(coord) {
            Some(hit) => hit,
            None => return Some(GuiInteraction::RawTouch(coord)),
        };
        self.draw_button(hit.row, hit.col, true);
        self.active_button = Some((hit.row, hit.col));
        match hit.button {
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
    fn handle_confirm_touch(&mut self, coord: Coordinates) -> Option<GuiInteraction> {
        let hit = match self.button_from_point(coord) {
            Some(hit) => hit,
            None => return Some(GuiInteraction::RawTouch(coord)),
        };
        self.draw_button(hit.row, hit.col, true);
        self.active_button = Some((hit.row, hit.col));
        let result = match hit.button {
            Button::Ok => Some(GuiInteraction::ConfirmAccepted),
            Button::Clear => Some(GuiInteraction::ConfirmRejected),
            Button::Digit(_) => Some(GuiInteraction::RawTouch(coord)),
        };
        match result {
            Some(GuiInteraction::ConfirmAccepted) => {
                self.confirm_result = Some(true);
            }
            Some(GuiInteraction::ConfirmRejected) => {
                self.confirm_result = Some(false);
            }
            _ => {}
        }
        result
    }
    fn draw_pin_pad(&mut self) {
        let _ = self.display.clear(COLOR_BACKGROUND);
        for (row_idx, row) in self.button_grid().iter().enumerate() {
            for col_idx in 0..row.len() {
                self.draw_button(row_idx, col_idx, false);
            }
        }
    }
    fn render_pin_header(&mut self) {
        let mut status = String::<32>::new();
        let _ = status.push_str("PIN: ");
        for _ in 0..self.pin_entered.len() {
            let _ = status.push('*');
        }
        self.render_header(status.as_str(), COLOR_BACKGROUND);
    }
    fn render_confirm_prompt(&mut self) {
        let _ = self.display.clear(COLOR_BACKGROUND);
        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let header_h = header_height();
        let _ = Text::with_baseline(
            &self.confirm_prompt,
            Point::new(PADDING, header_h / 2),
            style,
            Baseline::Top,
        )
        .draw(&mut self.display);
        for (label, col_idx) in [("NO", 0), ("", 1), ("YES", 2)] {
            if label.is_empty() {
                continue;
            }
            self.draw_button_with_label(3, col_idx, label, false);
        }
    }
    fn blit_boot_logo(&mut self) {
        let expected_len = (BOOT_LOGO_WIDTH as usize) * (BOOT_LOGO_HEIGHT as usize) * 2;
        debug_assert_eq!(BOOT_LOGO.len(), expected_len);
        let logo_iter = BOOT_LOGO.chunks_exact(2).map(|chunk| {
            let raw = u16::from_be_bytes([chunk[0], chunk[1]]);
            Rgb565::from(RawU16::new(raw))
        });
        let _ = self
            .display
            .set_pixels(0, 0, BOOT_LOGO_WIDTH - 1, BOOT_LOGO_HEIGHT - 1, logo_iter);
    }
    fn button_rect(&self, row: i32, col: i32) -> (Point, Size) {
        let width = BOOT_LOGO_WIDTH as i32;
        let row_height = row_height();
        let header_h = header_height();
        let button_width = (width - PADDING * 4) / 3;
        let button_height = (row_height - PADDING * 2).max(4);
        let x = PADDING + col * (button_width + PADDING);
        let y = header_h + row * row_height + PADDING;
        (
            Point::new(x, y),
            Size::new(button_width as u32, button_height as u32),
        )
    }
    fn button_grid(&self) -> [[Button; 3]; 4] {
        [
            [Button::Digit(1), Button::Digit(2), Button::Digit(3)],
            [Button::Digit(4), Button::Digit(5), Button::Digit(6)],
            [Button::Digit(7), Button::Digit(8), Button::Digit(9)],
            [Button::Clear, Button::Digit(0), Button::Ok],
        ]
    }
    fn button_from_point(&self, coord: Coordinates) -> Option<ButtonHit> {
        let x = coord.x as i32;
        let y = coord.y as i32;
        let header_h = header_height();
        if y < header_h {
            return None;
        }
        let width = BOOT_LOGO_WIDTH as i32;
        let button_width = (width - PADDING * 4) / 3;
        let row_height = row_height();
        let rel_x = x - PADDING;
        if rel_x < 0 {
            return None;
        }
        let col = rel_x / (button_width + PADDING);
        if col < 0 || col > 2 {
            return None;
        }
        let col_start = PADDING + col * (button_width + PADDING);
        if x > col_start + button_width {
            return None;
        }
        let rel_y = y - header_h;
        if rel_y < 0 {
            return None;
        }
        let row = rel_y / row_height;
        if row < 0 || row > 3 {
            return None;
        }
        let row_start = header_h + row * row_height;
        if y >= row_start + row_height {
            return None;
        }
        let row_idx = row as usize;
        let col_idx = col as usize;
        Some(ButtonHit {
            button: self.button_grid()[row_idx][col_idx],
            row: row_idx,
            col: col_idx,
        })
    }
    fn draw_button(&mut self, row: usize, col: usize, active: bool) {
        let label = button_label(self.button_grid()[row][col]);
        self.draw_button_with_label(row as i32, col as i32, label, active);
    }
    fn draw_button_with_label(&mut self, row: i32, col: i32, label: &str, active: bool) {
        let (top_left, size) = self.button_rect(row, col);
        let fill_color = if active {
            COLOR_BUTTON_ACTIVE
        } else {
            COLOR_BUTTON
        };
        let button_style = PrimitiveStyleBuilder::new()
            .fill_color(fill_color)
            .stroke_color(COLOR_BUTTON_BORDER)
            .stroke_width(2)
            .build();
        let rect = Rectangle::new(top_left, size);
        let _ = rect.into_styled(button_style).draw(&mut self.display);
        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let center = Point::new(
            top_left.x + size.width as i32 / 2,
            top_left.y + size.height as i32 / 2,
        );
        let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
        let _ = Text::with_alignment(
            label,
            Point::new(center.x, baseline),
            style,
            Alignment::Center,
        )
        .draw(&mut self.display);
    }
    fn render_header(&mut self, text: &str, bg: Rgb565) {
        let header_h = header_height();
        let header_rect = Rectangle::new(
            Point::new(0, 0),
            Size::new(BOOT_LOGO_WIDTH.into(), header_h as u32),
        );
        let _ = header_rect
            .into_styled(PrimitiveStyleBuilder::new().fill_color(bg).build())
            .draw(&mut self.display);
        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let baseline = header_h / 2 + FONT_10X20.character_size.height as i32 / 3;
        let _ = Text::with_alignment(
            text,
            Point::new((BOOT_LOGO_WIDTH / 2) as i32, baseline),
            style,
            Alignment::Center,
        )
        .draw(&mut self.display);
    }
    fn draw_centered_message(&mut self, text: &str) {
        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let baseline = (BOOT_LOGO_HEIGHT / 2) as i32;
        let _ = Text::with_alignment(
            text,
            Point::new((BOOT_LOGO_WIDTH / 2) as i32, baseline),
            style,
            Alignment::Center,
        )
        .draw(&mut self.display);
    }
    fn draw_unlock_spinner_frame(&mut self, frame: u8) {
        let center = Point::new(
            (BOOT_LOGO_WIDTH / 2) as i32,
            header_height() + row_height() * 2,
        );
        let erase = Rectangle::new(Point::new(center.x - 10, center.y - 12), Size::new(20, 24));
        let _ = erase
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_BACKGROUND)
                    .build(),
            )
            .draw(&mut self.display);
        let mut buf = [0u8; 4];
        let spinner_str = SPINNER_FRAMES[frame as usize].encode_utf8(&mut buf);
        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
        let _ = Text::with_alignment(
            spinner_str,
            Point::new(center.x, baseline),
            style,
            Alignment::Center,
        )
        .draw(&mut self.display);
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
                self.draw_unlock_spinner_frame(frame);
            }
        }
    }
    fn flash_touch_debug(&mut self) {
        self.debug_flash = !self.debug_flash;
        let color = if self.debug_flash {
            COLOR_BUTTON_ACTIVE
        } else {
            COLOR_BACKGROUND
        };
        let rect = Rectangle::new(
            Point::new(0, BOOT_LOGO_HEIGHT as i32 - DEBUG_BAR_HEIGHT),
            Size::new(BOOT_LOGO_WIDTH.into(), DEBUG_BAR_HEIGHT as u32),
        );
        let _ = rect
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut self.display);
    }
    fn draw_touch_debug(&mut self, coord: &Coordinates) {
        let size = 8i32;
        let half = size / 2;
        let x = coord.x as i32 - half;
        let y = coord.y as i32 - half;
        self.debug_flash = !self.debug_flash;
        let rect = Rectangle::new(
            Point::new(
                x.clamp(0, BOOT_LOGO_WIDTH as i32 - size),
                y.clamp(0, BOOT_LOGO_HEIGHT as i32 - size),
            ),
            Size::new(size as u32, size as u32),
        );
        let color = if self.debug_flash {
            COLOR_BUTTON_ACTIVE
        } else {
            COLOR_BACKGROUND
        };
        let _ = rect
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut self.display);
    }
    fn clear_touch_latch(&mut self) {
        if self.touch_active {
            self.touch_active = false;
            if let Some((row, col)) = self.active_button.take() {
                self.draw_button(row, col, false);
            }
        }
    }
}
fn button_label(button: Button) -> &'static str {
    match button {
        Button::Digit(0) => "0",
        Button::Digit(1) => "1",
        Button::Digit(2) => "2",
        Button::Digit(3) => "3",
        Button::Digit(4) => "4",
        Button::Digit(5) => "5",
        Button::Digit(6) => "6",
        Button::Digit(7) => "7",
        Button::Digit(8) => "8",
        Button::Digit(9) => "9",
        Button::Digit(_) => "?",
        Button::Clear => "CLR",
        Button::Ok => "OK",
    }
}

fn header_height() -> i32 {
    row_height()
}
fn row_height() -> i32 {
    (BOOT_LOGO_HEIGHT as i32) / 5
}

fn map_touch_point_raw(raw: Coordinates) -> Coordinates {
    // Sensor full ranges
    const SW_MIN: i32 = 0;   const SW_MAX: i32 = 239; // touch X
    const SH_MIN: i32 = 0;   const SH_MAX: i32 = 319; // touch Y

    // Visible window on the LCD (columns) due to display_offset(34, 0), width=172
    const VX_MIN: i32 = 34;
    const VX_MAX: i32 = 205; // 34 + 172 - 1

    // GUI target sizes
    let gw_max = (BOOT_LOGO_WIDTH  as i32) - 1; // 171
    let gh_max = (BOOT_LOGO_HEIGHT as i32) - 1; // 319

    // 1) mirror X to match flip_horizontal()
    let rx_mir = (SW_MAX - (raw.x as i32)).clamp(SW_MIN, SW_MAX);
    let ry     = (raw.y as i32).clamp(SH_MIN, SH_MAX);

    // 2) crop mirrored X to visible window 34..205
    let rx_vis = rx_mir.clamp(VX_MIN, VX_MAX);

    // 3) scale X: [34..205] -> [0..gw_max]
    let x_span = (VX_MAX - VX_MIN).max(1);
    let gx = ((rx_vis - VX_MIN) as i64 * gw_max as i64 / x_span as i64) as i32;

    // 4) scale Y: [0..319] -> [0..gh_max]
    let y_span = (SH_MAX - SH_MIN).max(1);
    let gy = ((ry - SH_MIN) as i64 * gh_max as i64 / y_span as i64) as i32;

    Coordinates {
        x: gx as u16,
        y: gy as u16,
    }
}


#[inline]
fn scale_clamped(v: i32, in_min: i32, in_max: i32, out_max: i32) -> i32 {
    if in_max <= in_min { return 0; }
    let vv = v.clamp(in_min, in_max) - in_min;
    (vv as i64 * out_max as i64 / (in_max - in_min) as i64) as i32
  }

// ---- helpers ----

#[inline]
fn normalize_12bit_to_range(v: u16, max_target: u16) -> u16 {
    // If v looks like 12-bit (much larger than target), scale; else leave as-is.
    let vt = v as u32;
    let mt = max_target as u32;
    if vt > mt + 8 { // tiny hysteresis to ignore jitter
        // scale 0..4095 -> 0..max_target
        const RAW_MAX: u32 = 4095;
        ((vt * mt + RAW_MAX/2) / RAW_MAX) as u16
    } else {
        v
    }
}

#[inline]
fn undo_driver_rotation(mut x: u16, mut y: u16, rot: TouchRotation, w: u16, h: u16) -> (u16, u16) {
    // Invert the exact transforms in your lib’s apply_rotation():
    match rot {
        TouchRotation::Rotate0 => {
            // lib did: x = (w-1) - x; y = y;
            x = (w - 1) - x;
            (x, y)
        }
        TouchRotation::Rotate90 => {
            // lib did: x = y; y = x;  => inverse is the same swap
            core::mem::swap(&mut x, &mut y);
            (x, y)
        }
        TouchRotation::Rotate180 => {
            // lib did: x = x; y = (h-1) - y;
            y = (h - 1) - y;
            (x, y)
        }
        TouchRotation::Rotate270 => {
            // lib did: x = (w-1) - y; y = (h-1) - x;
            // Inverse of that:
            let orig_x = (w - 1) - y;
            let orig_y = (h - 1) - x;
            (orig_x, orig_y)
        }
    }
}

fn map_touch_axis(value: u16, min: u16, max: u16, output: u16) -> u16 {
    if output <= 1 || max <= min {
        return 0;
    }
    let clamped = value.clamp(min, max);
    let span = u32::from(max - min);
    let relative = u32::from(clamped - min);
    let target = u32::from(output - 1);
    ((relative * target + span / 2) / span) as u16
}


#[inline]
fn usb_write(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, buf: &[u8]) {
    for &byte in buf {
        let _ = usb.write_byte_nb(byte);
    }
    let _ = usb.flush_tx_nb();
}