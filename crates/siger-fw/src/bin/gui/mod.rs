use core::convert::Infallible;

use axs5106l::{Axs5106l, Coordinates, Rotation as TouchRotation};
use display_interface_spi::SPIInterface;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, iso_8859_1, MonoTextStyle},
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text},
};
use embedded_graphics_core::pixelcolor::raw::RawU16;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_hal::delay::DelayNs;
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
};
use heapless::{String, Vec};
use mipidsi::{
    models::ST7789,
    options::{Orientation, Rotation},
    Builder, Display,
};

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

const HEADER_HEIGHT: i32 = 60;
const PADDING: i32 = 6;
const MAX_PIN_DIGITS: usize = 12;

const COLOR_BACKGROUND: Rgb565 = Rgb565::BLACK;
const COLOR_BUTTON: Rgb565 = Rgb565::new(6, 12, 6);
const COLOR_BUTTON_BORDER: Rgb565 = Rgb565::new(2, 4, 2);
const COLOR_HIGHLIGHT: Rgb565 = Rgb565::new(0, 32, 0);
const COLOR_TEXT: Rgb565 = Rgb565::WHITE;

/// Top-level GUI manager for the hardware wallet.
pub struct Gui {
    display: LcdDisplay,
    backlight: Output<'static>,
    touch: Option<TouchController>,
    touch_irq: Input<'static>,
    mode: GuiMode,
    pin_expected: Option<u8>,
    pin_entered: Vec<u8, MAX_PIN_DIGITS>,
    confirm_prompt: String<64>,
    touch_active: bool,
}

/// UI mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiMode {
    Splash,
    Locked,
    Confirm,
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
        // SPI setup for the ST7789 panel
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

        // Prepare I2C + touch controller (best-effort: ignore failures)
        let i2c = I2c::new(i2c0, I2cConfig::default())
            .map_err(GuiError::I2cConfig)?
            .with_scl(touch_scl)
            .with_sda(touch_sda);
        let touch_reset = Output::new(touch_rst, Level::High, OutputConfig::default());
        let mut touch_driver = Axs5106l::new(
            i2c,
            touch_reset,
            BOOT_LOGO_WIDTH,
            BOOT_LOGO_HEIGHT,
            TouchRotation::Rotate0,
        );
        let touch = match touch_driver.init(delay) {
            Ok(()) => Some(touch_driver),
            Err(_err) => None,
        };

        let touch_irq = Input::new(touch_int, InputConfig::default().with_pull(Pull::Up));

        let mut gui = Gui {
            display,
            backlight,
            touch,
            touch_irq,
            mode: GuiMode::Splash,
            pin_expected: None,
            pin_entered: Vec::new(),
            confirm_prompt: String::new(),
            touch_active: false,
        };

        gui.show_boot_logo();
        Ok(gui)
    }

    pub fn show_boot_logo(&mut self) {
        self.blit_boot_logo();
        self.mode = GuiMode::Splash;
    }

    pub fn begin_unlock(&mut self, expected_digits: Option<u8>) {
        self.pin_expected = expected_digits;
        self.pin_entered.clear();
        self.mode = GuiMode::Locked;
        self.draw_pin_pad();
        self.render_pin_header();
    }

    pub fn request_confirmation(&mut self, prompt: &str) {
        self.confirm_prompt.clear();
        let _ = self.confirm_prompt.push_str(prompt);
        self.mode = GuiMode::Confirm;
        self.render_confirm_prompt();
    }

    pub fn tick(&mut self) -> Option<GuiInteraction> {
        let Some(touch) = self.poll_touch_state() else {
            return None;
        };

        match self.mode {
            GuiMode::Locked => self.handle_pin_touch(touch),
            GuiMode::Confirm => self.handle_confirm_touch(touch),
            GuiMode::Splash => Some(GuiInteraction::RawTouch(touch)),
        }
    }

    pub fn mode(&self) -> GuiMode {
        self.mode
    }

    fn poll_touch_state(&mut self) -> Option<Coordinates> {
        let touch = self.touch.as_mut()?;

        if self.touch_irq.is_high() {
            self.touch_active = false;
            return None;
        }

        let touch_present = match touch.get_touch_data() {
            Ok(Some(data)) => data.first_touch(),
            _ => None,
        };

        match touch_present {
            Some(coord) => {
                if self.touch_active {
                    None
                } else {
                    self.touch_active = true;
                    Some(Coordinates {
                        x: mirror_x(coord.x),
                        y: coord.y,
                    })
                }
            }
            None => {
                self.touch_active = false;
                None
            }
        }
    }

    fn handle_pin_touch(&mut self, coord: Coordinates) -> Option<GuiInteraction> {
        let button = self.button_from_point(coord)?;

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
                        self.flash_pin_error();
                        return None;
                    }
                } else if self.pin_entered.len() < 4 {
                    self.flash_pin_error();
                    return None;
                }
                Some(GuiInteraction::PinComplete(self.pin_entered.clone()))
            }
        }
    }

    fn handle_confirm_touch(&mut self, coord: Coordinates) -> Option<GuiInteraction> {
        match self.button_from_point(coord) {
            Some(Button::Ok) => Some(GuiInteraction::ConfirmAccepted),
            Some(Button::Clear) => Some(GuiInteraction::ConfirmRejected),
            Some(Button::Digit(_)) | None => Some(GuiInteraction::RawTouch(coord)),
        }
    }

    fn draw_pin_pad(&mut self) {
        let _ = self.display.clear(COLOR_BACKGROUND);

        let button_grid = self.button_grid();
        let button_style = PrimitiveStyleBuilder::new()
            .fill_color(COLOR_BUTTON)
            .stroke_color(COLOR_BUTTON_BORDER)
            .stroke_width(2)
            .build();

        for (row_idx, row) in button_grid.iter().enumerate() {
            for (col_idx, button) in row.iter().enumerate() {
                let (top_left, size) = self.button_rect(row_idx as i32, col_idx as i32);
                let rect = Rectangle::new(top_left, size);
                let _ = rect.into_styled(button_style).draw(&mut self.display);

                let label = button_label(*button);
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
        }
    }

    fn render_pin_header(&mut self) {
        let mut status = String::<32>::new();
        let _ = status.push_str("PIN: ");
        for _ in 0..self.pin_entered.len() {
            let _ = status.push('*');
        }

        let header_rect = Rectangle::new(
            Point::new(0, 0),
            Size::new(BOOT_LOGO_WIDTH.into(), HEADER_HEIGHT as u32),
        );
        let _ = header_rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_BACKGROUND)
                    .build(),
            )
            .draw(&mut self.display);

        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let _ = Text::with_alignment(
            &status,
            Point::new(
                (BOOT_LOGO_WIDTH / 2) as i32,
                HEADER_HEIGHT / 2 + FONT_10X20.baseline as i32 / 2,
            ),
            style,
            Alignment::Center,
        )
        .draw(&mut self.display);
    }

    fn render_confirm_prompt(&mut self) {
        let _ = self.display.clear(COLOR_BACKGROUND);

        let style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
        let _ = Text::with_baseline(
            &self.confirm_prompt,
            Point::new(PADDING, HEADER_HEIGHT / 2),
            style,
            Baseline::Top,
        )
        .draw(&mut self.display);

        // Reuse button grid bottom row for clear/ok actions.
        let button_style = PrimitiveStyleBuilder::new()
            .fill_color(COLOR_BUTTON)
            .stroke_color(COLOR_BUTTON_BORDER)
            .stroke_width(2)
            .build();

        for (label, col_idx) in [("NO", 0), ("", 1), ("YES", 2)] {
            if label.is_empty() {
                continue;
            }
            let (top_left, size) = self.button_rect(3, col_idx);
            let rect = Rectangle::new(top_left, size);
            let _ = rect.into_styled(button_style).draw(&mut self.display);

            let text_style = MonoTextStyle::new(&FONT_10X20, COLOR_TEXT);
            let center = Point::new(
                top_left.x + size.width as i32 / 2,
                top_left.y + size.height as i32 / 2,
            );
            let baseline = center.y + FONT_10X20.character_size.height as i32 / 3;
            let _ = Text::with_alignment(
                label,
                Point::new(center.x, baseline),
                text_style,
                Alignment::Center,
            )
            .draw(&mut self.display);
        }
    }

    fn flash_pin_error(&mut self) {
        let highlight = Rectangle::new(
            Point::new(0, 0),
            Size::new(BOOT_LOGO_WIDTH.into(), HEADER_HEIGHT as u32),
        );
        let _ = highlight
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLOR_HIGHLIGHT)
                    .build(),
            )
            .draw(&mut self.display);
        self.render_pin_header();
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
        let height = BOOT_LOGO_HEIGHT as i32;

        let button_width = (width - PADDING * 4) / 3;
        let button_height = (height - HEADER_HEIGHT - PADDING * 5) / 4;

        let x = PADDING + col * (button_width + PADDING);
        let y = HEADER_HEIGHT + PADDING + row * (button_height + PADDING);

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

    fn button_from_point(&self, coord: Coordinates) -> Option<Button> {
        let x = coord.x as i32;
        let y = coord.y as i32;

        if y < HEADER_HEIGHT + PADDING {
            return None;
        }

        let width = BOOT_LOGO_WIDTH as i32;
        let height = BOOT_LOGO_HEIGHT as i32;
        let button_width = (width - PADDING * 4) / 3;
        let button_height = (height - HEADER_HEIGHT - PADDING * 5) / 4;

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

        let rel_y = y - (HEADER_HEIGHT + PADDING);
        if rel_y < 0 {
            return None;
        }
        let row = rel_y / (button_height + PADDING);
        if row < 0 || row > 3 {
            return None;
        }
        let row_start = HEADER_HEIGHT + PADDING + row * (button_height + PADDING);
        if y > row_start + button_height {
            return None;
        }

        Some(self.button_grid()[row as usize][col as usize])
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

fn mirror_x(x: u16) -> u16 {
    BOOT_LOGO_WIDTH.saturating_sub(1).saturating_sub(x)
}
