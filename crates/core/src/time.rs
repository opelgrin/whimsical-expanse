//! Simulation time.
//!
//! The simulation counts time in ticks: whole steps of action time. Calendar
//! time (time of day, seasons) is derived from the tick count elsewhere.

/// A point in simulation time: the number of ticks since the world began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(u64);

impl Tick {
    /// The moment the world begins.
    pub const ZERO: Self = Self(0);

    /// Creates the tick with the given count.
    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }

    /// The number of ticks since the world began.
    #[must_use]
    pub const fn count(self) -> u64 {
        self.0
    }

    /// The tick `ticks` later.
    ///
    /// # Panics
    ///
    /// Panics if the count overflows. At 60 ticks per second, that takes
    /// billions of years.
    #[must_use]
    pub const fn after(self, ticks: u64) -> Self {
        match self.0.checked_add(ticks) {
            Some(count) => Self(count),
            None => panic!("tick count overflow"),
        }
    }
}
