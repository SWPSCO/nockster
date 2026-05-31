use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};

const WIDTH: usize = 172;
const HEIGHT: usize = 320;
const CHUNK_ROWS: usize = 8;

static mut ROW_BUFFER: [Rgb565; WIDTH * CHUNK_ROWS] = [Rgb565::BLACK; WIDTH * CHUNK_ROWS];

const SINE_TABLE: [i8; 256] = [
    0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39, 42, 45, 48, 51, 54, 57, 59, 62, 65, 67, 70,
    73, 75, 78, 80, 82, 85, 87, 89, 91, 94, 96, 98, 100, 102, 103, 105, 107, 108, 110, 112, 113,
    114, 116, 117, 118, 119, 120, 121, 122, 123, 123, 124, 125, 125, 126, 126, 126, 126, 126, 127,
    126, 126, 126, 126, 126, 125, 125, 124, 123, 123, 122, 121, 120, 119, 118, 117, 116, 114, 113,
    112, 110, 108, 107, 105, 103, 102, 100, 98, 96, 94, 91, 89, 87, 85, 82, 80, 78, 75, 73, 70, 67,
    65, 62, 59, 57, 54, 51, 48, 45, 42, 39, 36, 33, 30, 27, 24, 21, 18, 15, 12, 9, 6, 3, 0, -3, -6,
    -9, -12, -15, -18, -21, -24, -27, -30, -33, -36, -39, -42, -45, -48, -51, -54, -57, -59, -62,
    -65, -67, -70, -73, -75, -78, -80, -82, -85, -87, -89, -91, -94, -96, -98, -100, -102, -103,
    -105, -107, -108, -110, -112, -113, -114, -116, -117, -118, -119, -120, -121, -122, -123, -123,
    -124, -125, -125, -126, -126, -126, -126, -126, -127, -126, -126, -126, -126, -126, -125, -125,
    -124, -123, -123, -122, -121, -120, -119, -118, -117, -116, -114, -113, -112, -110, -108, -107,
    -105, -103, -102, -100, -98, -96, -94, -91, -89, -87, -85, -82, -80, -78, -75, -73, -70, -67,
    -65, -62, -59, -57, -54, -51, -48, -45, -42, -39, -36, -33, -30, -27, -24, -21, -18, -15, -12,
    -9, -6, -3,
];

// 4x4 Bayer dithering matrix
const BAYER_MATRIX: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

#[derive(Clone, Copy)]
pub struct AnimationState {
    frame: u32,
    next_row: u16,
}

impl AnimationState {
    pub const fn new() -> Self {
        Self {
            frame: 0,
            next_row: 0,
        }
    }

    pub fn is_frame_start(&self) -> bool {
        self.next_row == 0
    }

    pub fn frame_index(&self) -> u32 {
        self.frame
    }
}

pub fn render_next_chunk(
    display: &mut super::GuiDisplay,
    state: &mut AnimationState,
    clip_range: Option<(u16, u16)>,
) -> Result<bool, ()> {
    let (clip_start, clip_end) = clip_range
        .map(|(s, e)| (s.min(HEIGHT as u16), e.min(HEIGHT as u16)))
        .unwrap_or((0, HEIGHT as u16));

    let clip_start_usize = clip_start as usize;
    let clip_end_usize = clip_end as usize;

    if clip_start_usize >= clip_end_usize {
        state.next_row = 0;
        state.frame = state.frame.wrapping_add(1);
        return Ok(true);
    }

    let mut start_row = state.next_row as usize;
    if start_row < clip_start_usize {
        start_row = clip_start_usize;
        state.next_row = clip_start;
    }

    if start_row >= clip_end_usize {
        state.next_row = 0;
        state.frame = state.frame.wrapping_add(1);
        return Ok(true);
    }

    if start_row >= HEIGHT {
        state.next_row = 0;
        state.frame = state.frame.wrapping_add(1);
        return Ok(true);
    }

    let rows_remaining = clip_end_usize - start_row;
    let rows_to_draw = rows_remaining.min(CHUNK_ROWS);
    if rows_to_draw == 0 {
        state.next_row = 0;
        state.frame = state.frame.wrapping_add(1);
        return Ok(true);
    }

    let t = (state.frame & 0xFF) as u8;

    unsafe {
        for row in 0..rows_to_draw {
            let y = start_row + row;
            let y_wave = ((y as u16 * 3 / 2 + t as u16 * 2) & 0xFF) as usize;
            let wave1 = SINE_TABLE[y_wave] as i32;

            let row_base = row * WIDTH;
            for x in 0..WIDTH {
                let x_wave = ((x as u16 * 2 + t as u16) & 0xFF) as usize;
                let wave2 = SINE_TABLE[x_wave] as i32;
                let wave3 = SINE_TABLE[((x + y) as u16 / 2 + t as u16 * 3) as usize & 0xFF] as i32;

                let combined = ((wave1 + wave2 + wave3) / 3 + 127) as u8;

                let threshold = BAYER_MATRIX[y % 4][x % 4] * 16;
                let dithered = if combined > threshold {
                    combined.saturating_sub(threshold / 2)
                } else {
                    combined / 2
                };

                let intensity = dithered / 3;
                let r = intensity.min(31);
                let g = (intensity + intensity / 4).min(63);
                let b = intensity.min(31);

                ROW_BUFFER[row_base + x] = Rgb565::new(r, g, b);
            }
        }

        let end_row = start_row + rows_to_draw - 1;
        if display
            .set_pixels(
                0,
                start_row as u16,
                (WIDTH - 1) as u16,
                end_row as u16,
                ROW_BUFFER[..rows_to_draw * WIDTH].iter().copied(),
            )
            .is_err()
        {
            return Err(());
        }
    }

    state.next_row = state
        .next_row
        .saturating_add(rows_to_draw as u16)
        .min(HEIGHT as u16);

    if state.next_row as usize >= clip_end_usize {
        state.next_row = 0;
        state.frame = state.frame.wrapping_add(1);
        Ok(true)
    } else {
        Ok(false)
    }
}
