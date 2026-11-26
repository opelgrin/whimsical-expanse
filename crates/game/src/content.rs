//! Stand-in: replaced by content files when the `content` crate exists.
//!
//! The default biome table and obstacle kinds, and the logical asset IDs
//! that presentation needs beyond them.
//!
//! The table is not limited to what the asset pack can show: biomes without
//! art show their placeholder. It is tuned by looking at generated worlds.

use expanse_core::asset::{GroundId, SpriteId};
use expanse_core::grid::TileOffset;
use expanse_terrain::{
    BiomeDefinition, BiomeObstacleDefinition, BiomeTable, FieldRanges, ObstacleKindDefinition,
    Variation,
};

/// The default biome table, with its obstacle kinds.
pub fn biome_table() -> BiomeTable {
    BiomeTable::new(biomes(), obstacle_kinds()).expect("the default biome table is valid")
}

fn obstacle_kinds() -> Vec<ObstacleKindDefinition> {
    let kind = |name: &str, footprint: &[(i32, i32)]| ObstacleKindDefinition {
        name: name.to_owned(),
        footprint: footprint
            .iter()
            .map(|&(dx, dy)| TileOffset::new(dx, dy))
            .collect(),
        sprite: SpriteId::new(name).expect("default sprite names are valid"),
    };
    vec![
        kind("rock", &[]),
        kind("boulder", &[(1, 0), (0, 1), (1, 1)]),
        kind("broadleaf_tree", &[]),
        kind("conifer", &[]),
        kind("palm", &[]),
        kind("fallen_log", &[(1, 0), (2, 0)]),
        kind("cactus", &[]),
        kind("shrub", &[]),
    ]
}

/// An obstacle kind of a biome, without variation.
fn even(kind: &str, density: f64) -> BiomeObstacleDefinition {
    BiomeObstacleDefinition {
        kind: kind.to_owned(),
        density,
        variation: None,
    }
}

/// An obstacle kind of a biome, grouped by a variation.
fn grouped(kind: &str, density: f64, scale_tiles: f64, strength: f64) -> BiomeObstacleDefinition {
    BiomeObstacleDefinition {
        kind: kind.to_owned(),
        density,
        variation: Some(Variation {
            scale_tiles,
            strength,
        }),
    }
}

fn biome(
    name: &str,
    map_colour: [u8; 3],
    ranges: FieldRanges,
    obstacles: Vec<BiomeObstacleDefinition>,
) -> BiomeDefinition {
    BiomeDefinition {
        name: name.to_owned(),
        ranges,
        map_colour,
        ground: GroundId::new(name).expect("default ground names are valid"),
        impassable: false,
        obstacles,
    }
}

fn impassable(name: &str, map_colour: [u8; 3], ranges: FieldRanges) -> BiomeDefinition {
    BiomeDefinition {
        impassable: true,
        ..biome(name, map_colour, ranges, Vec::new())
    }
}

/// The biomes in priority order: water first, then high ground, then the
/// biomes of each climate, from the coldest. Temperatures are in degrees
/// Celsius, elevation from −1 to 1, moisture and vegetation as shares of
/// the land tiles.
#[expect(
    clippy::too_many_lines,
    reason = "a table of content, one entry per biome"
)]
fn biomes() -> Vec<BiomeDefinition> {
    let any = FieldRanges::default;
    vec![
        impassable(
            "deep_water",
            [24, 52, 104],
            FieldRanges {
                elevation: Some(-1.0..-0.45),
                ..any()
            },
        ),
        impassable(
            "shallow_water",
            [46, 98, 160],
            FieldRanges {
                elevation: Some(-0.45..0.0),
                ..any()
            },
        ),
        impassable(
            "peaks",
            [236, 236, 240],
            FieldRanges {
                elevation: Some(0.94..1.0),
                ..any()
            },
        ),
        biome(
            "ice_sheet",
            [208, 226, 236],
            FieldRanges {
                summer_temperature: Some(-150.0..0.0),
                ..any()
            },
            vec![even("rock", 0.01)],
        ),
        biome(
            "highlands",
            [128, 120, 108],
            FieldRanges {
                elevation: Some(0.8..1.0),
                ..any()
            },
            vec![grouped("boulder", 0.03, 10.0, 0.9), even("rock", 0.04)],
        ),
        biome(
            "tundra",
            [150, 160, 132],
            FieldRanges {
                summer_temperature: Some(-150.0..10.0),
                ..any()
            },
            vec![even("rock", 0.02), grouped("shrub", 0.03, 8.0, 1.0)],
        ),
        biome(
            "taiga",
            [46, 86, 64],
            FieldRanges {
                winter_temperature: Some(-150.0..-10.0),
                vegetation: Some(0.45..1.0),
                ..any()
            },
            vec![
                grouped("conifer", 0.3, 14.0, 0.6),
                even("fallen_log", 0.01),
                even("rock", 0.005),
            ],
        ),
        biome(
            "boreal_shrubland",
            [124, 138, 98],
            FieldRanges {
                winter_temperature: Some(-150.0..-10.0),
                ..any()
            },
            vec![
                grouped("conifer", 0.03, 10.0, 1.0),
                grouped("shrub", 0.04, 6.0, 0.8),
                even("rock", 0.01),
            ],
        ),
        biome(
            "desert",
            [220, 196, 132],
            FieldRanges {
                summer_temperature: Some(24.0..150.0),
                moisture: Some(0.0..0.2),
                ..any()
            },
            vec![grouped("cactus", 0.008, 12.0, 1.0), even("rock", 0.01)],
        ),
        biome(
            "rainforest",
            [24, 96, 40],
            FieldRanges {
                winter_temperature: Some(18.0..150.0),
                moisture: Some(0.5..1.0),
                vegetation: Some(0.35..1.0),
                ..any()
            },
            vec![
                even("broadleaf_tree", 0.3),
                even("palm", 0.05),
                even("fallen_log", 0.01),
            ],
        ),
        biome(
            "savanna",
            [190, 176, 92],
            FieldRanges {
                winter_temperature: Some(18.0..150.0),
                ..any()
            },
            vec![
                grouped("broadleaf_tree", 0.02, 16.0, 1.0),
                grouped("shrub", 0.03, 8.0, 0.8),
            ],
        ),
        biome(
            "temperate_forest",
            [54, 112, 52],
            FieldRanges {
                vegetation: Some(0.5..1.0),
                moisture: Some(0.3..1.0),
                ..any()
            },
            vec![
                grouped("broadleaf_tree", 0.25, 12.0, 0.7),
                even("fallen_log", 0.012),
                even("rock", 0.005),
            ],
        ),
        biome(
            "steppe",
            [176, 172, 110],
            FieldRanges {
                moisture: Some(0.0..0.3),
                ..any()
            },
            vec![grouped("shrub", 0.02, 10.0, 1.0), even("rock", 0.01)],
        ),
        biome(
            "grassland",
            [118, 168, 80],
            any(),
            vec![
                grouped("broadleaf_tree", 0.02, 14.0, 1.0),
                even("rock", 0.006),
                even("fallen_log", 0.002),
            ],
        ),
    ]
}

/// The grounds in drawing order, from the lowest to the highest: where two
/// grounds meet, the edge is drawn on the higher one.
pub fn drawing_order() -> Vec<GroundId> {
    [
        "deep_water",
        "shallow_water",
        "desert",
        "steppe",
        "savanna",
        "tundra",
        "grassland",
        "boreal_shrubland",
        "temperate_forest",
        "taiga",
        "rainforest",
        "highlands",
        "ice_sheet",
        "peaks",
    ]
    .into_iter()
    .map(|name| GroundId::new(name).expect("default ground names are valid"))
    .collect()
}

/// Stand-in: replaced by the species content of the first actors in M2.
///
/// The sprite of the wandering placeholder entity.
pub fn wanderer_sprite() -> SpriteId {
    SpriteId::new("wanderer").expect("a valid name")
}
