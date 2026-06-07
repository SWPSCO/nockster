use core::ops::{Add, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration {
    millis: u64,
}

impl Duration {
    pub const fn from_millis(millis: u64) -> Self {
        Self { millis }
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self {
            millis: secs.saturating_mul(1_000),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    millis: u64,
}

impl Instant {
    pub fn now() -> Self {
        let millis = web_sys::window()
            .and_then(|window| window.performance())
            .map(|performance| performance.now().max(0.0) as u64)
            .unwrap_or(0);
        Self { millis }
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Instant {
            millis: self.millis.saturating_add(rhs.millis),
        }
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        Duration {
            millis: self.millis.saturating_sub(rhs.millis),
        }
    }
}
