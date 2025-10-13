use embedded_graphics::pixelcolor::Rgb565;

const WIDTH: usize = 172;
const HEIGHT: usize = 320;

static mut FRAMEBUFFER: [u16; WIDTH * HEIGHT] = [0u16; WIDTH * HEIGHT];

const SINE_TABLE: [i8; 256] = [
    0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39, 42, 45,
    48, 51, 54, 57, 59, 62, 65, 67, 70, 73, 75, 78, 80, 82, 85, 87,
    89, 91, 94, 96, 98, 100, 102, 103, 105, 107, 108, 110, 112, 113, 114, 116,
    117, 118, 119, 120, 121, 122, 123, 123, 124, 125, 125, 126, 126, 126, 126, 126,
    127, 126, 126, 126, 126, 126, 125, 125, 124, 123, 123, 122, 121, 120, 119, 118,
    117, 116, 114, 113, 112, 110, 108, 107, 105, 103, 102, 100, 98, 96, 94, 91,
    89, 87, 85, 82, 80, 78, 75, 73, 70, 67, 65, 62, 59, 57, 54, 51,
    48, 45, 42, 39, 36, 33, 30, 27, 24, 21, 18, 15, 12, 9, 6, 3,
    0, -3, -6, -9, -12, -15, -18, -21, -24, -27, -30, -33, -36, -39, -42, -45,
    -48, -51, -54, -57, -59, -62, -65, -67, -70, -73, -75, -78, -80, -82, -85, -87,
    -89, -91, -94, -96, -98, -100, -102, -103, -105, -107, -108, -110, -112, -113, -114, -116,
    -117, -118, -119, -120, -121, -122, -123, -123, -124, -125, -125, -126, -126, -126, -126, -126,
    -127, -126, -126, -126, -126, -126, -125, -125, -124, -123, -123, -122, -121, -120, -119, -118,
    -117, -116, -114, -113, -112, -110, -108, -107, -105, -103, -102, -100, -98, -96, -94, -91,
    -89, -87, -85, -82, -80, -78, -75, -73, -70, -67, -65, -62, -59, -57, -54, -51,
    -48, -45, -42, -39, -36, -33, -30, -27, -24, -21, -18, -15, -12, -9, -6, -3
];

// 4x4 Bayer dithering matrix
const BAYER_MATRIX: [[u8; 4]; 4] = [
    [ 0,  8,  2, 10],
    [12,  4, 14,  6],
    [ 3, 11,  1,  9],
    [15,  7, 13,  5]
];

pub fn render_frame_bulk(display: &mut super::GuiDisplay, frame: u32) -> Result<(), core::convert::Infallible> {
    let t = (frame & 0xFF) as u8;
    
    unsafe {
        for y in 0..HEIGHT {
            let y_wave = ((y as u16 * 3 / 2 + t as u16 * 2) & 0xFF) as usize;
            let wave1 = SINE_TABLE[y_wave] as i32;
            
            for x in 0..WIDTH {
                let x_wave = ((x as u16 * 2 + t as u16) & 0xFF) as usize;
                let wave2 = SINE_TABLE[x_wave] as i32;
                let wave3 = SINE_TABLE[((x + y) as u16 / 2 + t as u16 * 3) as usize & 0xFF] as i32;
                
                let combined = ((wave1 + wave2 + wave3) / 3 + 127) as u8;
                
                // Dithering threshold
                let threshold = BAYER_MATRIX[y % 4][x % 4] * 16;
                let dithered = if combined > threshold { 
                    combined.saturating_sub(threshold / 2)
                } else {
                    combined / 2
                };
                
                // Greyscale with slight green tint for CRT feel
                let intensity = dithered / 3;
                let r = intensity.min(31);
                let g = (intensity + intensity / 4).min(63);  // Slightly more green
                let b = intensity.min(31);
                
                let raw: u16 = ((r as u16) << 11) | ((g as u16) << 5) | (b as u16);
                FRAMEBUFFER[y * WIDTH + x] = raw;
            }
        }
    }
    
    let _ = display.set_pixels(
        0, 0,
        (WIDTH - 1) as u16,
        (HEIGHT - 1) as u16,
        unsafe { 
            FRAMEBUFFER.iter().map(|&raw| {
                let r = ((raw >> 11) & 0x1F) as u8;
                let g = ((raw >> 5) & 0x3F) as u8;
                let b = (raw & 0x1F) as u8;
                Rgb565::new(r, g, b)
            })
        }
    );
    
    Ok(())
}