//! Deciding how many ticks are due as real time passes.
//!
//! The pacer is pure: callers measure elapsed time and pass it in. That keeps
//! the logic testable without a clock.

use std::time::Duration;

/// The selected simulation speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Paused,
    /// The base tick rate.
    Normal,
    /// Twice the base tick rate.
    Fast,
    /// Three times the base tick rate.
    Faster,
}

impl Speed {
    const fn multiplier(self) -> u32 {
        match self {
            Self::Paused => 0,
            Self::Normal => 1,
            Self::Fast => 2,
            Self::Faster => 3,
        }
    }
}

/// Turns elapsed real time into a number of due ticks.
#[derive(Debug, Clone)]
pub struct TickPacer {
    base_ticks_per_second: u32,
    max_lag: Duration,
    owed: Duration,
}

impl TickPacer {
    /// Creates a pacer running `base_ticks_per_second` at normal speed.
    ///
    /// When the simulation falls behind, at most `max_lag` of owed time is
    /// kept. The rest is dropped instead of being caught up in a burst.
    ///
    /// # Panics
    ///
    /// Panics if `base_ticks_per_second` is zero.
    #[must_use]
    pub fn new(base_ticks_per_second: u32, max_lag: Duration) -> Self {
        assert!(base_ticks_per_second > 0, "the tick rate must be positive");
        Self {
            base_ticks_per_second,
            max_lag,
            owed: Duration::ZERO,
        }
    }

    /// Forgets any owed time. Used when the speed changes or the simulation
    /// resumes, so that time spent paused is not caught up.
    pub const fn reset(&mut self) {
        self.owed = Duration::ZERO;
    }

    /// Records `elapsed` real time and returns how many ticks are now due at
    /// `speed`. Time short of a whole tick is carried over to the next call.
    pub fn due_ticks(&mut self, elapsed: Duration, speed: Speed) -> u64 {
        let Some(tick_length) = self.tick_length(speed) else {
            self.reset();
            return 0;
        };
        let owed = (self.owed + elapsed).min(self.max_lag.max(tick_length));
        let (owed_nanos, tick_nanos) = (owed.as_nanos(), tick_length.as_nanos());
        // The remainder is shorter than one tick, itself at most one second.
        self.owed =
            Duration::from_nanos(u64::try_from(owed_nanos % tick_nanos).unwrap_or(u64::MAX));
        u64::try_from(owed_nanos / tick_nanos).unwrap_or(u64::MAX)
    }

    /// The real time left until the next tick is due at `speed`, or `None` when
    /// paused.
    #[must_use]
    pub fn time_until_next_tick(&self, speed: Speed) -> Option<Duration> {
        self.tick_length(speed)
            .map(|tick_length| tick_length.saturating_sub(self.owed))
    }

    fn tick_length(&self, speed: Speed) -> Option<Duration> {
        let ticks_per_second = self.base_ticks_per_second * speed.multiplier();
        (ticks_per_second > 0).then(|| Duration::from_secs(1) / ticks_per_second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 10;
    const TICK: Duration = Duration::from_millis(100);
    const GENEROUS_LAG: Duration = Duration::from_secs(60);

    /// Catches truncation that drops the time short of a whole tick, which
    /// would make the simulation run slow whenever frames are short.
    #[test]
    fn carries_partial_ticks_over_to_the_next_call() {
        let mut pacer = TickPacer::new(RATE, GENEROUS_LAG);
        let three_quarters = TICK * 3 / 4;
        // Owed after each call: 3/4, then 6/4 → 1 tick + 2/4, then 5/4 → 1 tick + 1/4.
        assert_eq!(pacer.due_ticks(three_quarters, Speed::Normal), 0);
        assert_eq!(pacer.due_ticks(three_quarters, Speed::Normal), 1);
        assert_eq!(pacer.due_ticks(three_quarters, Speed::Normal), 1);
        assert_eq!(
            pacer.time_until_next_tick(Speed::Normal),
            Some(three_quarters)
        );
    }

    /// Catches speed multipliers that are wrong or not applied.
    #[test]
    fn tick_rate_scales_with_speed() {
        let one_second = Duration::from_secs(1);
        let rate = u64::from(RATE);
        for (speed, expected) in [
            (Speed::Paused, 0),
            (Speed::Normal, rate),
            (Speed::Fast, 2 * rate),
            (Speed::Faster, 3 * rate),
        ] {
            let mut pacer = TickPacer::new(RATE, GENEROUS_LAG);
            assert_eq!(pacer.due_ticks(one_second, speed), expected, "{speed:?}");
        }
    }

    /// Catches a pacer that tries to catch up a long stall in one burst, which
    /// would freeze the simulation thread after a hiccup.
    #[test]
    fn a_long_stall_is_capped_at_the_maximum_lag() {
        let max_lag = TICK * 3;
        let mut pacer = TickPacer::new(RATE, max_lag);
        assert_eq!(pacer.due_ticks(Duration::from_secs(10), Speed::Normal), 3);
    }

    /// Catches time spent paused being caught up on resume.
    #[test]
    fn pausing_forgets_owed_time() {
        let mut pacer = TickPacer::new(RATE, GENEROUS_LAG);
        assert_eq!(pacer.due_ticks(TICK / 2, Speed::Normal), 0);
        assert_eq!(pacer.due_ticks(Duration::from_secs(5), Speed::Paused), 0);
        assert_eq!(pacer.due_ticks(TICK / 2, Speed::Normal), 0);
        assert_eq!(pacer.time_until_next_tick(Speed::Paused), None);
    }
}
