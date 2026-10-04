#![warn(clippy::all, clippy::cargo, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::i2c::I2c;
use heapless::Vec;

/// Maximum number of touch points supported by the AXS5106L
const MAX_TOUCH_POINTS: usize = 2;

/// Errors that can occur when using the AXS5106L driver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axs5106lError<I2cError> {
    /// I2C communication error
    I2c(I2cError),
    /// Invalid device ID read during initialization
    InvalidDeviceId,
    /// GPIO error during reset
    Gpio,
}

/// Screen rotation options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    /// No rotation (0°)
    #[default]
    Rotate0 = 0,
    /// 90° clockwise rotation
    Rotate90 = 1,
    /// 180° rotation
    Rotate180 = 2,
    /// 270° clockwise rotation
    Rotate270 = 3,
}

/// Touch point coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coordinates {
    pub x: u16,
    pub y: u16,
}

/// Touch data containing all detected touch points
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TouchData {
    /// Vector of touch coordinates (max 5 points)
    pub points: Vec<Coordinates, MAX_TOUCH_POINTS>,
    /// Number of active touch points
    pub count: usize,
}

impl TouchData {
    /// Create a new empty `TouchData`
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Check if there are any active touch points
    pub fn has_touches(&self) -> bool {
        self.count > 0
    }

    /// Get the first touch point, if any
    pub fn first_touch(&self) -> Option<Coordinates> {
        self.points.first().copied()
    }

    /// Apply a transformation to all touch points, via a closure
    pub fn apply_transform(&mut self, transform: impl Fn(Coordinates) -> Coordinates) {
        for point in &mut self.points {
            *point = transform(*point);
        }
    }
}

/// AXS5106L touchscreen driver
pub struct Axs5106l<I2C, RST>
where
    I2C: I2c,
    RST: OutputPin,
{
    /// I2C interface for communication with the touch controller
    i2c: I2C,
    /// Reset pin (GPIO output)
    reset_pin: RST,
    /// Display width in pixels
    width: u16,
    /// Display height in pixels
    height: u16,
    /// Screen rotation setting
    rotation: Rotation,
}

impl<I2C, RST> Axs5106l<I2C, RST>
where
    I2C: I2c,
    RST: OutputPin,
{
    /// I2C address of the AXS5106L touch controller
    pub const I2C_ADDR: u8 = 0x63;

    /// Register addresses
    pub const ID_REG: u8 = 0x08;
    pub const TOUCH_DATA_REG: u8 = 0x01;

    /// Create a new AXS5106L driver instance
    ///
    /// # Arguments
    /// * `i2c` - I2C interface implementing `embedded_hal::i2c::I2c`
    /// * `reset_pin` - GPIO pin for reset, implementing `embedded_hal::digital::OutputPin`
    /// * `width` - Display width in pixels
    /// * `height` - Display height in pixels
    /// * `rotation` - Screen rotation setting
    pub fn new(i2c: I2C, reset_pin: RST, width: u16, height: u16, rotation: Rotation) -> Self {
        Self {
            i2c,
            reset_pin,
            width,
            height,
            rotation,
        }
    }

    /// Get the current display dimensions
    pub fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Get the current rotation setting
    pub fn get_rotation(&self) -> Rotation {
        self.rotation
    }

    /// Update the rotation setting
    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
    }

    /// Release the I2C and reset pin resources
    pub fn release(self) -> (I2C, RST) {
        (self.i2c, self.reset_pin)
    }
}

impl<I2C, RST, E> Axs5106l<I2C, RST>
where
    I2C: I2c<Error = E>,
    RST: OutputPin,
{
    /// Write data to a register via I2C
    #[allow(dead_code)]
    fn write_register(&mut self, reg_addr: u8, data: &[u8]) -> Result<(), Axs5106lError<E>> {
        let mut buffer = [0u8; 16]; // Max data length we might need
        if data.len() >= buffer.len() {
            // This should never happen with this driver, but let's be safe
            return Err(Axs5106lError::I2c(
                self.i2c.write(Self::I2C_ADDR, &[reg_addr]).unwrap_err(),
            ));
        }

        buffer[0] = reg_addr;
        buffer[1..=data.len()].copy_from_slice(data);

        self.i2c
            .write(Self::I2C_ADDR, &buffer[..=data.len()])
            .map_err(Axs5106lError::I2c)
    }

    /// Read data from a register via I2C
    fn read_register(&mut self, reg_addr: u8, buffer: &mut [u8]) -> Result<(), Axs5106lError<E>> {
        // First write the register address
        self.i2c
            .write(Self::I2C_ADDR, &[reg_addr])
            .map_err(Axs5106lError::I2c)?;

        // Then read the data
        self.i2c
            .read(Self::I2C_ADDR, buffer)
            .map_err(Axs5106lError::I2c)
    }

    /// Initialize the AXS5106L touch controller
    ///
    /// This performs the reset sequence and verifies the device ID.
    ///
    /// Takes a `delay` as an argument implementing `embedded_hal::delay::DelayNs`
    /// Returns `Ok(())` if initialization is successful.
    ///
    /// # Errors
    /// This function will return an `Err(Axs5106lError)` if initialization fails
    pub fn init<D>(&mut self, delay: &mut D) -> Result<(), Axs5106lError<E>>
    where
        D: DelayNs,
    {
        // Reset sequence: LOW for 200ms, then HIGH, then wait 300ms
        self.reset_pin.set_low().map_err(|_| Axs5106lError::Gpio)?;
        delay.delay_ms(200);

        self.reset_pin.set_high().map_err(|_| Axs5106lError::Gpio)?;
        delay.delay_ms(300);

        // Read and verify device ID
        let mut id_buffer = [0u8; 3];
        self.read_register(Self::ID_REG, &mut id_buffer)?;

        // The C code checks if data[0] != 0, which suggests any non-zero first byte is valid
        // In real implementation, you might want to check for specific ID values
        if id_buffer[0] == 0 {
            return Err(Axs5106lError::InvalidDeviceId);
        }

        Ok(())
    }

    /// Read raw touch data from the AXS5106L registers
    ///
    /// This method reads the raw touch data from the controller and parses
    /// it into a `TouchData` structure. It does not apply any transformations.
    ///
    /// *you should use `get_touch_data()` to retrieve the processed data*
    ///
    /// Note: In the original C code, this was called when an interrupt occurred.
    /// In this Rust version, you should call this method periodically or when
    /// you suspect touch data might be available.
    ///
    /// # Errors
    /// Returns `Err(Axs5106lError)` if I2C communication fails
    pub fn read_raw_touch_data(&mut self) -> Result<(TouchData, [u8; 14]), Axs5106lError<E>> {
        // Read 14 bytes of touch data (matches the C implementation)
        let mut data = [0u8; 14];
        self.read_register(Self::TOUCH_DATA_REG, &mut data)?;

        // Parse the number of touch points from byte 1
        let touch_count = data[1] as usize;

        let mut touch_data = TouchData::new();

        touch_data.points.clear();
        touch_data.count = touch_count;

        // If no touches, we're done
        if touch_count != 0 {
            // Parse each touch point (up to MAX_TOUCH_POINTS)
            // Each touch point is 6 bytes: [status, x_high, x_low, y_high, y_low, pressure]
            let max_points = touch_count.min(MAX_TOUCH_POINTS);

            for i in 0..max_points {
                let base_idx = 2 + i * 6; // Start after the header bytes

                // Extract X coordinate (12-bit value: high nibble stored in low bits of first byte)
                let x = ((u16::from(data[base_idx] & 0x0f)) << 8) | u16::from(data[base_idx + 1]);

                // Extract Y coordinate (12-bit value)
                let y =
                    ((u16::from(data[base_idx + 2] & 0x0f)) << 8) | u16::from(data[base_idx + 3]);

                let coords = Coordinates { x, y };

                // Add to our touch data (heapless::Vec will handle capacity limits)
                let _ = touch_data.points.push(coords);
            }
        }

        Ok((touch_data, data))
    }

    /// Get the processed touch data with rotation applied
    ///
    /// This method returns the touch data with coordinate transformations
    /// applied based on the current rotation setting.
    /// Returns `Ok(Some(TouchData))` if touch data is available, or
    /// `Ok(None)` if no touches are detected.
    ///
    /// # Errors
    /// Returns `Err(Axs5106lError)` if I2C communication fails
    pub fn get_touch_data(&mut self) -> Result<Option<TouchData>, Axs5106lError<E>> {
        let (mut touch_data, _) = self.read_raw_touch_data()?;
        if touch_data.count == 0 {
            return Ok(None);
        }

        // Apply rotation transformation to each touch point
        touch_data.apply_transform(|coords| self.apply_rotation(coords));
        Ok(Some(touch_data))
    }

    /// Apply rotation transformation to coordinates
    ///
    /// This internal method transforms raw coordinates based on the current
    /// rotation setting, matching the behavior of the original C implementation.
    fn apply_rotation(&self, coords: Coordinates) -> Coordinates {
        match self.rotation {
            Rotation::Rotate90 => {
                // Case 1: 90° rotation - swap X and Y
                Coordinates {
                    x: coords.y,
                    y: coords.x,
                }
            }
            Rotation::Rotate180 => {
                // Case 2: 180° rotation - flip Y axis only
                Coordinates {
                    x: coords.x,
                    y: self.height.saturating_sub(1).saturating_sub(coords.y),
                }
            }
            Rotation::Rotate270 => {
                // Case 3: 270° rotation - swap and flip both axes
                Coordinates {
                    x: self.width.saturating_sub(1).saturating_sub(coords.y),
                    y: self.height.saturating_sub(1).saturating_sub(coords.x),
                }
            }
            Rotation::Rotate0 => {
                // Case 0 (default): flip X axis only
                Coordinates {
                    x: self.width.saturating_sub(1).saturating_sub(coords.x),
                    y: coords.y,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinates_creation() {
        let coords = Coordinates { x: 100, y: 200 };
        assert_eq!(coords.x, 100);
        assert_eq!(coords.y, 200);
    }

    #[test]
    fn test_touch_data_creation() {
        let touch_data = TouchData::new();
        assert_eq!(touch_data.count, 0);
        assert!(!touch_data.has_touches());
        assert_eq!(touch_data.first_touch(), None);
    }

    #[test]
    fn test_rotation_default() {
        let rotation = Rotation::default();
        assert_eq!(rotation, Rotation::Rotate0);
    }
}
