//! Latitude, and a tile's temperature through the year.
//!
//! A tile's climate is two temperatures, stored in its fields: its winter and
//! summer temperatures, the coldest and the warmest average of the year. The
//! current temperature is never stored. [`temperature_at`] computes it when
//! needed, and it is the only code that combines climate with the date.

use std::f64::consts::TAU;

use crate::field::{Field, Fields};

/// A latitude, in millionths of a degree: positive north of the equator,
/// negative south of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Latitude {
    microdegrees: i32,
}

impl Latitude {
    /// The equator.
    pub const EQUATOR: Self = Self { microdegrees: 0 };

    /// The latitude closest to `degrees`, or `None` if it is not between −90°
    /// and 90°.
    #[must_use]
    pub fn from_degrees(degrees: f64) -> Option<Self> {
        if !(-90.0..=90.0).contains(&degrees) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a latitude in millionths of a degree is within ±90,000,000"
        )]
        let microdegrees = (degrees * 1e6).round() as i32;
        Some(Self { microdegrees })
    }

    /// The latitude in degrees.
    #[must_use]
    pub fn degrees(self) -> f64 {
        f64::from(self.microdegrees) / 1e6
    }

    /// Whether the latitude lies south of the equator, where the seasons are
    /// half a year apart from the calendar's.
    #[must_use]
    pub const fn is_south(self) -> bool {
        self.microdegrees < 0
    }
}

/// Stand-in: replaced by the calendar types in M1 item 6, which give
/// [`temperature_at`] its date.
///
/// A moment of the year: the share of the year elapsed since it began, from
/// 0 included to 1 excluded.
///
/// The calendar's seasons are spring, summer, autumn and winter, a quarter of
/// the year each, in that order from the start of the year. They are the
/// seasons north of the equator; south of it, a tile's season is half a year
/// apart.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TimeOfYear(f64);

impl TimeOfYear {
    /// The moment `elapsed` of the way through the year, or `None` unless
    /// `0 <= elapsed < 1`.
    #[must_use]
    pub fn new(elapsed: f64) -> Option<Self> {
        (0.0..1.0).contains(&elapsed).then_some(Self(elapsed))
    }

    /// The share of the year elapsed.
    #[must_use]
    pub const fn elapsed(self) -> f64 {
        self.0
    }
}

/// The middle of the calendar's summer, as a share of the year: summer is the
/// second quarter.
const MIDSUMMER_NORTH: f64 = 0.375;

/// The temperature, in degrees Celsius, of a tile with `fields` at
/// `latitude`, at moment `time` of the year.
///
/// It follows a smooth yearly cycle that reaches the summer temperature in
/// the middle of the tile's summer and the winter temperature half a year
/// later, in the middle of its winter.
#[must_use]
pub fn temperature_at(fields: &Fields, latitude: Latitude, time: TimeOfYear) -> f64 {
    let winter = Field::WinterTemperature.to_unit(fields.winter_temperature);
    let summer = Field::SummerTemperature.to_unit(fields.summer_temperature);
    let midsummer = if latitude.is_south() {
        MIDSUMMER_NORTH + 0.5
    } else {
        MIDSUMMER_NORTH
    };
    let mean = f64::midpoint(winter, summer);
    mean + (summer - winter) / 2.0 * libm::cos((time.elapsed() - midsummer) * TAU)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::temperature_from_celsius;

    fn fields(winter_celsius: f64, summer_celsius: f64) -> Fields {
        Fields {
            elevation: 0,
            winter_temperature: temperature_from_celsius(winter_celsius),
            summer_temperature: temperature_from_celsius(summer_celsius),
            moisture: 0,
            vegetation: 0,
        }
    }

    /// The moment of the year with the highest and the lowest temperature,
    /// sampled every 1/96 of the year, and those temperatures.
    fn extremes(fields: &Fields, latitude: Latitude) -> ((f64, f64), (f64, f64)) {
        let samples: Vec<(f64, f64)> = (0..96)
            .map(|step| {
                let time = TimeOfYear::new(f64::from(step) / 96.0).expect("within the year");
                (time.elapsed(), temperature_at(fields, latitude, time))
            })
            .collect();
        let by_temperature = |a: &&(f64, f64), b: &&(f64, f64)| a.1.total_cmp(&b.1);
        let warmest = *samples.iter().max_by(by_temperature).expect("samples");
        let coldest = *samples.iter().min_by(by_temperature).expect("samples");
        (warmest, coldest)
    }

    /// Catches a cycle that peaks at the wrong time, ignores the hemisphere,
    /// or does not reach the tile's summer and winter temperatures. Expected
    /// moments worked out by hand: summer is the second quarter of the year,
    /// so its middle is at 3/8, and winter's at 7/8; south of the equator,
    /// the other way round.
    #[test]
    fn temperature_peaks_in_midsummer_and_bottoms_in_midwinter_of_each_hemisphere() {
        let fields = fields(-12.0, 21.0);
        let tolerance = 0.01;
        for (degrees, midsummer, midwinter) in [(45.0, 0.375, 0.875), (-30.0, 0.875, 0.375)] {
            let latitude = Latitude::from_degrees(degrees).expect("a valid latitude");
            let ((warmest_at, warmest), (coldest_at, coldest)) = extremes(&fields, latitude);
            assert_eq!(warmest_at, midsummer, "{degrees}°");
            assert_eq!(coldest_at, midwinter, "{degrees}°");
            assert!((warmest - 21.0).abs() < tolerance, "{degrees}°: {warmest}");
            assert!((coldest + 12.0).abs() < tolerance, "{degrees}°: {coldest}");
        }
    }
}
