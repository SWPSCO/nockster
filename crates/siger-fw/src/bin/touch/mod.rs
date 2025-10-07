use heapless::Vec;
#[derive(Copy, Clone)]
pub enum Rotation { Rot0, Rot90, Rot180, Rot270 }
#[derive(Copy, Clone)]
pub struct TouchCal {
    // Raw bounds observed from your controller (use your printed min/max)
    pub x_min: i32, pub x_max: i32,
    pub y_min: i32, pub y_max: i32,
    // GUI logical size (pixels)
    pub width: i32,  // e.g. 320
    pub height: i32, // e.g. 172
    pub rotation: Rotation,
    pub invert_x: bool,
    pub invert_y: bool,
}
impl TouchCal {
    #[inline]
    pub fn map(&self, rx: i32, ry: i32) -> (i32, i32) {
        // Clamp raw
        let clamp = |v, lo, hi| if v < lo { lo } else if v > hi { hi } else { v };
        let rx = clamp(rx, self.x_min, self.x_max);
        let ry = clamp(ry, self.y_min, self.y_max);
        // Normalize raw -> integer [0 .. W-1] / [0 .. H-1] without floats
        // nx = (rx - x_min) / (x_max - x_min), then scale to width/height-1
        let dx = (rx - self.x_min).max(0);
        let dy = (ry - self.y_min).max(0);
        let span_x = (self.x_max - self.x_min).max(1);
        let span_y = (self.y_max - self.y_min).max(1);
        let mut u = (dx as i64) * ((self.width  - 1) as i64)  / (span_x as i64);
        let mut v = (dy as i64) * ((self.height - 1) as i64) / (span_y as i64);
        // Optional mirrors
        if self.invert_x { u = (self.width  - 1) as i64 - u; }
        if self.invert_y { v = (self.height - 1) as i64 - v; }
        // Rotate into screen space
        let (w, h) = (self.width as i64, self.height as i64);
        let (x, y) = match self.rotation {
            Rotation::Rot0   => (u,                  v),
            Rotation::Rot90  => (v,                  (h - 1) - u),
            Rotation::Rot180 => ((w - 1) - u,        (h - 1) - v),
            Rotation::Rot270 => ((w - 1) - v,        u),
        };
        // Final clamp and cast
        let x = x.clamp(0, (w - 1)) as i32;
        let y = y.clamp(0, (h - 1)) as i32;
        (x, y)
    }
}
// -------- Debounced tap filter (no timers required) --------
enum TapState {
    Idle,
    Collect { ticks_left: u16 },
    Cooldown { ticks_left: u16 },
}
pub struct TapFilter<const N: usize> {
    state: TapState,
    // Collected raw samples for current tap
    buf: Vec<(i32,i32), N>,
    // How many poll() calls to collect before deciding the tap
    collect_ticks: u16,
    // How many poll() calls to ignore after a tap
    cooldown_ticks: u16,
}
impl<const N: usize> TapFilter<N> {
    pub fn new(collect_ticks: u16, cooldown_ticks: u16) -> Self {
        Self {
            state: TapState::Idle,
            buf: Vec::new(),
            collect_ticks,
            cooldown_ticks,
        }
    }
    /// Call once per loop. Pass current raw sample if finger is down, else None.
    /// Returns one (x,y) per tap (debounced & averaged), or None.
    pub fn poll(
        &mut self,
        raw: Option<(i32,i32)>,
        cal: &TouchCal,
    ) -> Option<(i32,i32)> {
        match self.state {
            TapState::Idle => {
                if let Some(pt) = raw {
                    self.buf.clear();
                    let _ = self.buf.push(pt);
                    self.state = TapState::Collect { ticks_left: self.collect_ticks };
                }
                None
            }
            TapState::Collect { mut ticks_left } => {
                // If still touching, keep collecting
                if let Some(pt) = raw {
                    let _ = self.buf.push(pt);
                }
                // Count down our window regardless, so we finish even if finger stays down
                if ticks_left > 0 { ticks_left -= 1; }
                if ticks_left == 0 || raw.is_none() {
                    // Decide one stable point
                    let (mx, my) = median_xy(&self.buf);
                    let (sx, sy) = cal.map(mx, my);
                    self.buf.clear();
                    self.state = TapState::Cooldown { ticks_left: self.cooldown_ticks };
                    Some((sx, sy))
                } else {
                    self.state = TapState::Collect { ticks_left };
                    None
                }
            }
            TapState::Cooldown { mut ticks_left } => {
                // Ignore input until cooldown elapses AND finger lifted
                if ticks_left > 0 { ticks_left -= 1; }
                if ticks_left == 0 && raw.is_none() {
                    self.state = TapState::Idle;
                } else {
                    self.state = TapState::Cooldown { ticks_left };
                }
                None
            }
        }
    }
}
fn median_xy<const N: usize>(pts: &Vec<(i32,i32), N>) -> (i32,i32) {
    let mut xs: Vec<i32, N> = Vec::new();
    let mut ys: Vec<i32, N> = Vec::new();
    for (x,y) in pts.iter() {
        let _ = xs.push(*x);
        let _ = ys.push(*y);
    }
    xs.sort_unstable();
    ys.sort_unstable();
    let m = xs.len().saturating_sub(1) / 2;
    (xs.get(m).copied().unwrap_or(0), ys.get(m).copied().unwrap_or(0))
}