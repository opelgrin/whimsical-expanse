//! The fields of a tile, and the units in which they are read.
//!
//! Every field is quantised to a `u16`, and every `u16` is a valid value.
//! Each field has the scale that keeps a threshold predictable:
//!
//! - **Elevation** is a rank in its space, from −1 (the deepest tile) through
//!   0 (sea level) to 1 (the highest). Water tiles and land tiles are ranked
//!   separately, so a threshold below 0 is a share of the water tiles and a
//!   threshold above 0 a share of the land tiles.
//! - **Winter and summer temperatures** are on [the temperature
//!   scale](LOWEST_CELSIUS), in degrees Celsius, the same in every space.
//! - **Moisture and vegetation** are ranks among the land tiles, from 0 to 1,
//!   so a threshold is a share of the land tiles. Water tiles are ranked
//!   among themselves.

/// The values from which a tile's biome is chosen, each quantised to a
/// `u16`. See the [module documentation](self) for their scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fields {
    pub elevation: u16,
    pub winter_temperature: u16,
    pub summer_temperature: u16,
    pub moisture: u16,
    pub vegetation: u16,
}

impl Fields {
    /// The quantised value of `field`.
    #[must_use]
    pub const fn get(&self, field: Field) -> u16 {
        match field {
            Field::Elevation => self.elevation,
            Field::WinterTemperature => self.winter_temperature,
            Field::SummerTemperature => self.summer_temperature,
            Field::Moisture => self.moisture,
            Field::Vegetation => self.vegetation,
        }
    }
}

/// The lowest elevation of a land tile: tiles below it are under water. In
/// the elevation's unit, it is 0.
pub const SEA_LEVEL: u16 = 32_768;

/// The lowest temperature the terrain can hold, in degrees Celsius.
///
/// The temperature scale is part of the terrain format. It is wide enough that
/// no space needs more, so it is never tuned: changing it changes the meaning
/// of every stored temperature. A step is about 0.005 °C.
pub const LOWEST_CELSIUS: f64 = -150.0;

/// The highest temperature the terrain can hold, in degrees Celsius. See
/// [`LOWEST_CELSIUS`].
pub const HIGHEST_CELSIUS: f64 = 150.0;

/// One of the fields of a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Field {
    Elevation,
    WinterTemperature,
    SummerTemperature,
    Moisture,
    Vegetation,
}

impl Field {
    /// Every field, in a fixed order.
    pub const ALL: [Self; 5] = [
        Self::Elevation,
        Self::WinterTemperature,
        Self::SummerTemperature,
        Self::Moisture,
        Self::Vegetation,
    ];

    /// The lowest and the highest value of the field, in its unit.
    #[must_use]
    pub const fn unit_range(self) -> (f64, f64) {
        match self {
            Self::Elevation => (-1.0, 1.0),
            Self::WinterTemperature | Self::SummerTemperature => (LOWEST_CELSIUS, HIGHEST_CELSIUS),
            Self::Moisture | Self::Vegetation => (0.0, 1.0),
        }
    }

    /// The quantised value `value` in the field's unit.
    #[must_use]
    pub fn to_unit(self, value: u16) -> f64 {
        let (lowest, highest) = self.unit_range();
        lowest + f64::from(value) / self.steps() * (highest - lowest)
    }

    /// The smallest quantised value whose unit value is at least `unit`, as a
    /// `u32` so that the top of the unit lies beyond every value: a range up
    /// to the top accepts the highest value. `None` if `unit` lies outside
    /// the unit's range or is not a number.
    #[must_use]
    pub fn boundary(self, unit: f64) -> Option<u32> {
        let (lowest, highest) = self.unit_range();
        if !(lowest..=highest).contains(&unit) {
            return None;
        }
        if unit >= highest {
            return Some(1 << 16);
        }
        let position = ((unit - lowest) / (highest - lowest) * self.steps()).ceil();
        Some(position_to_u32(position))
    }

    /// The number of steps between the lowest and the highest value of the
    /// unit. Temperatures reach both ends of their scale. Ranks reach their
    /// lowest value only: a rank is a share of tiles, and the share of tiles
    /// below the highest is less than 1.
    fn steps(self) -> f64 {
        match self {
            Self::WinterTemperature | Self::SummerTemperature => f64::from(u16::MAX),
            Self::Elevation | Self::Moisture | Self::Vegetation => f64::from(1_u32 << 16),
        }
    }
}

/// The quantised temperature closest to `celsius`, clamped to the
/// temperature scale.
#[must_use]
pub fn temperature_from_celsius(celsius: f64) -> u16 {
    let clamped = celsius.clamp(LOWEST_CELSIUS, HIGHEST_CELSIUS);
    let position = ((clamped - LOWEST_CELSIUS) / (HIGHEST_CELSIUS - LOWEST_CELSIUS)
        * f64::from(u16::MAX))
    .round();
    position_to_u16(position)
}

/// Converts a whole number between 0 and `u16::MAX` to an integer.
fn position_to_u16(position: f64) -> u16 {
    u16::try_from(position_to_u32(position)).expect("a clamped temperature fits in u16")
}

/// Converts a whole number between 0 and 2^16 to an integer.
fn position_to_u32(position: f64) -> u32 {
    debug_assert!((0.0..=65_536.0).contains(&position) && position.fract() == 0.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the position is a whole number between 0 and 2^16"
    )]
    let whole = position as u32;
    whole
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches temperatures that drift when stored and read back, or that are
    /// not clamped to the scale. Values worked out by hand.
    #[test]
    fn temperatures_round_trip_within_a_step_and_clamp_to_the_scale() {
        let step = (HIGHEST_CELSIUS - LOWEST_CELSIUS) / f64::from(u16::MAX);
        for tenths in -1_500..=1_500 {
            let celsius = f64::from(tenths) / 10.0;
            let stored = temperature_from_celsius(celsius);
            let read = Field::WinterTemperature.to_unit(stored);
            assert!((read - celsius).abs() <= step / 2.0 + 1e-9, "{celsius} °C");
        }
        assert_eq!(temperature_from_celsius(-400.0), 0);
        assert_eq!(temperature_from_celsius(400.0), u16::MAX);
        assert_eq!(Field::SummerTemperature.to_unit(0), LOWEST_CELSIUS);
        assert_eq!(Field::SummerTemperature.to_unit(u16::MAX), HIGHEST_CELSIUS);
    }

    /// Catches boundaries off by one value, which would move every threshold,
    /// and a top boundary that excludes the highest value. Values worked out
    /// by hand.
    #[test]
    fn boundaries_split_the_values_where_the_unit_says() {
        assert_eq!(Field::Elevation.boundary(0.0), Some(u32::from(SEA_LEVEL)));
        assert_eq!(Field::Elevation.boundary(-1.0), Some(0));
        assert_eq!(Field::Elevation.boundary(1.0), Some(65_536));
        assert_eq!(Field::Moisture.boundary(0.25), Some(16_384));
        assert_eq!(Field::Vegetation.boundary(1.0), Some(65_536));
        assert_eq!(Field::Moisture.boundary(1.5), None);
        assert_eq!(Field::Moisture.boundary(f64::NAN), None);
        // 0 °C is 150/300 of the way up 65,535 steps: 32,767.5, so the first
        // value at or above it is 32,768.
        assert_eq!(Field::WinterTemperature.boundary(0.0), Some(32_768));
        assert!(Field::WinterTemperature.to_unit(32_767) < 0.0);
        assert!(Field::WinterTemperature.to_unit(32_768) >= 0.0);
        assert_eq!(Field::SummerTemperature.boundary(-200.0), None);
    }
}
