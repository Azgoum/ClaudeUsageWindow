use chrono::{DateTime, Utc};

/// Wall-clock source (used for pace, reset detection and `retry_at`). Scheduling uses
/// `tokio::time` so tests can drive both together.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(any(test, feature = "test-util"))]
pub struct FakeClock(std::sync::Mutex<DateTime<Utc>>);

#[cfg(any(test, feature = "test-util"))]
impl FakeClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self(std::sync::Mutex::new(start))
    }

    pub fn advance(&self, d: chrono::Duration) {
        *self.0.lock().unwrap() += d;
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_advances() {
        let t0: DateTime<Utc> = "2026-11-03T12:00:00Z".parse().unwrap();
        let c = FakeClock::new(t0);
        c.advance(chrono::Duration::seconds(90));
        assert_eq!(c.now(), t0 + chrono::Duration::seconds(90));
    }
}
