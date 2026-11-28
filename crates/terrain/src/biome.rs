//! The biome table and the obstacle kinds: content that gives the terrain its
//! meaning.
//!
//! The engine never names a biome or an obstacle kind. Content supplies them
//! as definitions, in units a person can read, and [`BiomeTable::new`]
//! checks and converts them.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use expanse_core::asset::{GroundId, SpriteId};
use expanse_core::grid::TileOffset;

use crate::field::{Field, Fields};
use crate::obstacle::{Footprint, InvalidFootprint};
use crate::tile::{BiomeIndex, ObstacleIndex};

/// A biome, as content defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeDefinition {
    /// The stable name, by which saves will refer to the biome.
    pub name: String,
    /// The range of each field that the biome accepts.
    pub ranges: FieldRanges,
    /// The biome's colour on maps, in sRGB.
    pub map_colour: [u8; 3],
    /// What the biome's ground looks like.
    pub ground: GroundId,
    /// Whether the biome blocks movement, as water does.
    pub impassable: bool,
    /// The obstacles placed in the biome.
    pub obstacles: Vec<BiomeObstacleDefinition>,
}

/// The range of each field that a biome accepts, in the field's unit (see
/// [`Field::unit_range`]). A range includes its start and excludes its end,
/// except that a range ending at the top of the unit includes it. A field
/// without a range accepts any value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldRanges {
    pub elevation: Option<Range<f64>>,
    pub winter_temperature: Option<Range<f64>>,
    pub summer_temperature: Option<Range<f64>>,
    pub moisture: Option<Range<f64>>,
    pub vegetation: Option<Range<f64>>,
}

impl FieldRanges {
    fn get(&self, field: Field) -> Option<&Range<f64>> {
        match field {
            Field::Elevation => self.elevation.as_ref(),
            Field::WinterTemperature => self.winter_temperature.as_ref(),
            Field::SummerTemperature => self.summer_temperature.as_ref(),
            Field::Moisture => self.moisture.as_ref(),
            Field::Vegetation => self.vegetation.as_ref(),
        }
    }
}

/// An obstacle kind placed in a biome, as content defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeObstacleDefinition {
    /// The name of the obstacle kind.
    pub kind: String,
    /// The chance that a tile of the biome anchors an obstacle of this kind,
    /// on average over the biome.
    pub density: f64,
    /// How the density varies from place to place, if it does.
    pub variation: Option<Variation>,
}

/// Smooth variation of an obstacle kind's density, which groups obstacles:
/// groves in open land, clearings in woods, clusters of rocks.
///
/// Where the variation's noise is `n`, between −1 and 1, the chance is the
/// density times `1 + strength × n`. The average stays the density.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Variation {
    /// The size of the variation's features, in tiles.
    pub scale_tiles: f64,
    /// How far the density moves away from its average, as a share of it.
    pub strength: f64,
}

/// A kind of obstacle, as content defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObstacleKindDefinition {
    /// The stable name.
    pub name: String,
    /// The tiles the obstacle blocks besides its anchor, as offsets from the
    /// anchor. Empty for an obstacle of one tile.
    pub footprint: Vec<TileOffset>,
    /// What the obstacle looks like.
    pub sprite: SpriteId,
}

/// The biomes, in order, and the obstacle kinds they place.
///
/// A tile takes the first biome whose ranges all contain its fields. The
/// last biome has no ranges, so every tile gets one.
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeTable {
    biomes: Vec<Biome>,
    obstacle_kinds: Vec<ObstacleKind>,
}

/// A biome of a [`BiomeTable`].
#[derive(Debug, Clone, PartialEq)]
pub struct Biome {
    name: String,
    /// For each field in [`Field::ALL`] order, the quantised values accepted:
    /// from the start included to the end excluded.
    ranges: [Option<Range<u32>>; Field::ALL.len()],
    map_colour: [u8; 3],
    ground: GroundId,
    impassable: bool,
    obstacles: Vec<BiomeObstacle>,
}

impl Biome {
    /// The stable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The colour on maps, in sRGB.
    #[must_use]
    pub const fn map_colour(&self) -> [u8; 3] {
        self.map_colour
    }

    /// What the biome's ground looks like.
    #[must_use]
    pub const fn ground(&self) -> &GroundId {
        &self.ground
    }

    /// Whether the biome blocks movement.
    #[must_use]
    pub const fn is_impassable(&self) -> bool {
        self.impassable
    }

    /// The obstacle kinds placed in the biome, in the order content lists them.
    #[must_use]
    pub fn obstacles(&self) -> &[BiomeObstacle] {
        &self.obstacles
    }

    /// Whether every range of the biome contains the field's value.
    fn accepts(&self, fields: &Fields) -> bool {
        Field::ALL.iter().zip(&self.ranges).all(|(&field, range)| {
            range
                .as_ref()
                .is_none_or(|range| range.contains(&u32::from(fields.get(field))))
        })
    }
}

/// An obstacle kind placed in a biome, checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiomeObstacle {
    /// The obstacle kind.
    pub kind: ObstacleIndex,
    /// The average chance that a tile anchors an obstacle of this kind,
    /// between 0 and 1.
    pub density: f64,
    /// How the density varies, if it does. It never takes the chance below 0
    /// or above 1.
    pub variation: Option<Variation>,
}

/// An obstacle kind of a [`BiomeTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObstacleKind {
    name: String,
    footprint: Footprint,
    sprite: SpriteId,
}

impl ObstacleKind {
    /// The stable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The tiles the obstacle blocks.
    #[must_use]
    pub const fn footprint(&self) -> &Footprint {
        &self.footprint
    }

    /// What the obstacle looks like.
    #[must_use]
    pub const fn sprite(&self) -> &SpriteId {
        &self.sprite
    }
}

impl BiomeTable {
    /// Checks the definitions and converts them to the terrain's units.
    ///
    /// # Errors
    ///
    /// Returns an error naming the problem if the table is empty, if its last
    /// biome has ranges, if a name repeats, if a range or a density lies
    /// outside its unit or a variation could take the chance below 0 or above
    /// 1, if a biome names an unknown obstacle kind, or if a footprint is
    /// invalid.
    pub fn new(
        biomes: Vec<BiomeDefinition>,
        obstacle_kinds: Vec<ObstacleKindDefinition>,
    ) -> Result<Self, InvalidBiomeTable> {
        let obstacle_kinds = check_obstacle_kinds(obstacle_kinds)?;
        let last = biomes.last().ok_or(InvalidBiomeTable::NoBiomes)?;
        if Field::ALL
            .iter()
            .any(|&field| last.ranges.get(field).is_some())
        {
            return Err(InvalidBiomeTable::LastBiomeHasRanges(last.name.clone()));
        }
        if u16::try_from(biomes.len()).is_err() {
            return Err(InvalidBiomeTable::TooManyBiomes);
        }
        let mut checked: Vec<Biome> = Vec::with_capacity(biomes.len());
        for biome in biomes {
            if checked.iter().any(|earlier| earlier.name == biome.name) {
                return Err(InvalidBiomeTable::RepeatedName(biome.name));
            }
            checked.push(check_biome(biome, &obstacle_kinds)?);
        }
        Ok(Self {
            biomes: checked,
            obstacle_kinds,
        })
    }

    /// The first biome whose ranges all contain `fields`.
    ///
    /// # Panics
    ///
    /// Never: [`new`](Self::new) ensures that the last biome accepts every
    /// tile and that every biome has an index.
    #[must_use]
    pub fn classify(&self, fields: &Fields) -> BiomeIndex {
        let position = self
            .biomes
            .iter()
            .position(|biome| biome.accepts(fields))
            .expect("the last biome has no ranges, so it accepts every tile");
        BiomeIndex(u16::try_from(position).expect("a table has fewer than 2^16 biomes"))
    }

    /// The biome at `index`.
    ///
    /// # Panics
    ///
    /// Panics if the table has no biome at `index`.
    #[must_use]
    pub fn biome(&self, index: BiomeIndex) -> &Biome {
        &self.biomes[usize::from(index.0)]
    }

    /// The number of biomes.
    #[must_use]
    pub const fn biome_count(&self) -> usize {
        self.biomes.len()
    }

    /// The obstacle kind at `index`.
    ///
    /// # Panics
    ///
    /// Panics if the table has no obstacle kind at `index`.
    #[must_use]
    pub fn obstacle_kind(&self, index: ObstacleIndex) -> &ObstacleKind {
        &self.obstacle_kinds[usize::from(index.0)]
    }

    /// The number of obstacle kinds.
    #[must_use]
    pub const fn obstacle_kind_count(&self) -> usize {
        self.obstacle_kinds.len()
    }
}

fn check_obstacle_kinds(
    definitions: Vec<ObstacleKindDefinition>,
) -> Result<Vec<ObstacleKind>, InvalidBiomeTable> {
    if u16::try_from(definitions.len()).is_err() {
        return Err(InvalidBiomeTable::TooManyObstacleKinds);
    }
    let mut kinds: Vec<ObstacleKind> = Vec::with_capacity(definitions.len());
    for definition in definitions {
        if kinds.iter().any(|earlier| earlier.name == definition.name) {
            return Err(InvalidBiomeTable::RepeatedName(definition.name));
        }
        let footprint = Footprint::new(definition.footprint).map_err(|error| {
            InvalidBiomeTable::InvalidFootprint {
                kind: definition.name.clone(),
                error,
            }
        })?;
        kinds.push(ObstacleKind {
            name: definition.name,
            footprint,
            sprite: definition.sprite,
        });
    }
    Ok(kinds)
}

fn check_biome(
    definition: BiomeDefinition,
    obstacle_kinds: &[ObstacleKind],
) -> Result<Biome, InvalidBiomeTable> {
    let name = definition.name;
    let mut ranges = [const { None }; Field::ALL.len()];
    for (slot, field) in ranges.iter_mut().zip(Field::ALL) {
        let Some(range) = definition.ranges.get(field) else {
            continue;
        };
        let invalid = || InvalidBiomeTable::InvalidRange {
            biome: name.clone(),
            field,
        };
        let start = field.boundary(range.start).ok_or_else(invalid)?;
        let end = field.boundary(range.end).ok_or_else(invalid)?;
        if range.start >= range.end {
            return Err(invalid());
        }
        *slot = Some(start..end);
    }
    let mut obstacles = Vec::with_capacity(definition.obstacles.len());
    for obstacle in definition.obstacles {
        let position = obstacle_kinds
            .iter()
            .position(|kind| kind.name == obstacle.kind);
        let invalid = |problem| InvalidBiomeTable::InvalidObstacle {
            biome: name.clone(),
            kind: obstacle.kind.clone(),
            problem,
        };
        let position = position.ok_or_else(|| invalid(ObstacleProblem::UnknownKind))?;
        if !(0.0..=1.0).contains(&obstacle.density) {
            return Err(invalid(ObstacleProblem::DensityOutsideZeroToOne));
        }
        if let Some(variation) = obstacle.variation {
            let highest = obstacle.density * (1.0 + variation.strength);
            if !(variation.scale_tiles > 0.0 && variation.scale_tiles.is_finite()) {
                return Err(invalid(ObstacleProblem::VariationScaleNotPositive));
            }
            if !((0.0..=1.0).contains(&variation.strength) && highest <= 1.0) {
                return Err(invalid(ObstacleProblem::VariationOutsideZeroToOne));
            }
        }
        obstacles.push(BiomeObstacle {
            kind: ObstacleIndex(u16::try_from(position).expect("checked above")),
            density: obstacle.density,
            variation: obstacle.variation,
        });
    }
    Ok(Biome {
        name,
        ranges,
        map_colour: definition.map_colour,
        ground: definition.ground,
        impassable: definition.impassable,
        obstacles,
    })
}

/// Why a biome table was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidBiomeTable {
    /// The table has no biome.
    NoBiomes,
    /// The last biome, named here, has ranges, so some tiles might get no
    /// biome.
    LastBiomeHasRanges(String),
    /// Two biomes, or two obstacle kinds, have this name.
    RepeatedName(String),
    /// The table has more biomes than a tile can refer to.
    TooManyBiomes,
    /// The table has more obstacle kinds than a tile can refer to.
    TooManyObstacleKinds,
    /// A range is empty, or lies outside its field's unit.
    InvalidRange { biome: String, field: Field },
    /// An obstacle kind of a biome cannot be placed as defined.
    InvalidObstacle {
        biome: String,
        kind: String,
        problem: ObstacleProblem,
    },
    /// An obstacle kind's footprint is invalid.
    InvalidFootprint {
        kind: String,
        error: InvalidFootprint,
    },
}

/// What is wrong with an obstacle kind of a biome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleProblem {
    /// No obstacle kind has the name.
    UnknownKind,
    /// The density is not between 0 and 1.
    DensityOutsideZeroToOne,
    /// The variation's scale is not a positive number of tiles.
    VariationScaleNotPositive,
    /// The variation could take the chance below 0 or above 1.
    VariationOutsideZeroToOne,
}

impl fmt::Display for InvalidBiomeTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBiomes => write!(f, "the biome table has no biome"),
            Self::LastBiomeHasRanges(name) => write!(
                f,
                "the last biome, `{name}`, has ranges; it must accept every tile"
            ),
            Self::RepeatedName(name) => write!(f, "the name `{name}` is used twice"),
            Self::TooManyBiomes => write!(f, "the biome table has more than 65,535 biomes"),
            Self::TooManyObstacleKinds => write!(f, "there are more than 65,535 obstacle kinds"),
            Self::InvalidRange { biome, field } => write!(
                f,
                "biome `{biome}`: the {field:?} range is empty or outside the field's unit"
            ),
            Self::InvalidObstacle {
                biome,
                kind,
                problem,
            } => {
                let problem = match problem {
                    ObstacleProblem::UnknownKind => "no obstacle kind has this name",
                    ObstacleProblem::DensityOutsideZeroToOne => {
                        "the density is not between 0 and 1"
                    }
                    ObstacleProblem::VariationScaleNotPositive => {
                        "the variation's scale is not a positive number of tiles"
                    }
                    ObstacleProblem::VariationOutsideZeroToOne => {
                        "the variation could take the chance below 0 or above 1"
                    }
                };
                write!(f, "biome `{biome}`, obstacle `{kind}`: {problem}")
            }
            Self::InvalidFootprint { kind, error } => write!(f, "obstacle `{kind}`: {error}"),
        }
    }
}

impl Error for InvalidBiomeTable {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{SEA_LEVEL, temperature_from_celsius};

    fn biome(name: &str, ranges: FieldRanges) -> BiomeDefinition {
        BiomeDefinition {
            name: name.to_owned(),
            ranges,
            map_colour: [0, 0, 0],
            ground: GroundId::new("ground").expect("a valid name"),
            impassable: false,
            obstacles: Vec::new(),
        }
    }

    fn fields(elevation: u16, winter_celsius: f64, moisture_share: f64) -> Fields {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test shares are between 0 and 1"
        )]
        let moisture = (moisture_share * 65_536.0) as u16;
        Fields {
            elevation,
            winter_temperature: temperature_from_celsius(winter_celsius),
            summer_temperature: temperature_from_celsius(20.0),
            moisture,
            vegetation: 0,
        }
    }

    /// Catches classification that ignores the order of the table, ignores a
    /// field, or includes the end of a range. The expected biomes are worked
    /// out by hand from the table.
    #[test]
    fn a_tile_takes_the_first_biome_whose_ranges_contain_its_fields() {
        let table = BiomeTable::new(
            vec![
                biome(
                    "water",
                    FieldRanges {
                        elevation: Some(-1.0..0.0),
                        ..FieldRanges::default()
                    },
                ),
                biome(
                    "cold",
                    FieldRanges {
                        winter_temperature: Some(-150.0..-10.0),
                        ..FieldRanges::default()
                    },
                ),
                biome(
                    "wet",
                    FieldRanges {
                        moisture: Some(0.5..1.0),
                        ..FieldRanges::default()
                    },
                ),
                biome("anything", FieldRanges::default()),
            ],
            Vec::new(),
        )
        .expect("a valid table");
        let land = SEA_LEVEL;
        let cases = [
            (fields(SEA_LEVEL - 1, -20.0, 0.9), 0),
            (fields(land, -20.0, 0.9), 1),
            (fields(land, -10.0, 0.9), 2),
            (fields(land, 5.0, 0.5), 2),
            (fields(land, 5.0, 0.49), 3),
            (fields(u16::MAX, 5.0, 0.99), 2),
        ];
        for (fields, expected) in cases {
            assert_eq!(table.classify(&fields), BiomeIndex(expected), "{fields:?}");
        }
    }

    /// Catches tables that could leave a tile without a biome, and content
    /// errors that would surface later as a panic or a silent misplacement.
    #[test]
    fn rejects_tables_that_cannot_classify_or_place() {
        let any = || biome("any", FieldRanges::default());
        let rock = || ObstacleKindDefinition {
            name: "rock".to_owned(),
            footprint: Vec::new(),
            sprite: SpriteId::new("rock").expect("a valid name"),
        };
        let with_obstacle = |density, variation| BiomeDefinition {
            obstacles: vec![BiomeObstacleDefinition {
                kind: "rock".to_owned(),
                density,
                variation,
            }],
            ..any()
        };
        let reject = |biomes, kinds| BiomeTable::new(biomes, kinds).expect_err("invalid");
        assert_eq!(reject(vec![], vec![]), InvalidBiomeTable::NoBiomes);
        let ranged = biome(
            "ranged",
            FieldRanges {
                moisture: Some(0.0..1.0),
                ..FieldRanges::default()
            },
        );
        assert_eq!(
            reject(vec![any(), ranged.clone()], vec![]),
            InvalidBiomeTable::LastBiomeHasRanges("ranged".to_owned())
        );
        assert!(BiomeTable::new(vec![ranged, any()], vec![]).is_ok());
        for bad in [0.5..0.5, -0.1..0.5, 0.2..1.1, f64::NAN..0.5] {
            let invalid = biome(
                "bad",
                FieldRanges {
                    vegetation: Some(bad),
                    ..FieldRanges::default()
                },
            );
            assert_eq!(
                reject(vec![invalid, any()], vec![]),
                InvalidBiomeTable::InvalidRange {
                    biome: "bad".to_owned(),
                    field: Field::Vegetation
                }
            );
        }
        assert_eq!(
            reject(vec![any(), any()], vec![]),
            InvalidBiomeTable::RepeatedName("any".to_owned())
        );
        let problem = |biomes, kinds| match reject(biomes, kinds) {
            InvalidBiomeTable::InvalidObstacle { problem, .. } => problem,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            problem(vec![with_obstacle(0.1, None)], vec![]),
            ObstacleProblem::UnknownKind
        );
        assert_eq!(
            problem(vec![with_obstacle(1.2, None)], vec![rock()]),
            ObstacleProblem::DensityOutsideZeroToOne
        );
        let variation = |scale_tiles, strength| {
            Some(Variation {
                scale_tiles,
                strength,
            })
        };
        assert_eq!(
            problem(vec![with_obstacle(0.6, variation(8.0, 0.8))], vec![rock()]),
            ObstacleProblem::VariationOutsideZeroToOne
        );
        assert_eq!(
            problem(vec![with_obstacle(0.1, variation(8.0, 1.5))], vec![rock()]),
            ObstacleProblem::VariationOutsideZeroToOne
        );
        assert_eq!(
            problem(vec![with_obstacle(0.1, variation(0.0, 0.5))], vec![rock()]),
            ObstacleProblem::VariationScaleNotPositive
        );
        assert!(
            BiomeTable::new(vec![with_obstacle(0.5, variation(8.0, 1.0))], vec![rock()]).is_ok()
        );
    }
}
