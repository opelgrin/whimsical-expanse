//! A world created on a generated surface, as the game creates it.
//!
//! The surface is small and has its own parameters, so that generation stays
//! a small part of the time these tests take.

use expanse_core::grid::TileOffset;
use expanse_sim::{World, WorldConfig};
use expanse_terrain::{
    BiomeDefinition, BiomeObstacleDefinition, BiomeTable, FieldRanges, ObstacleKindDefinition,
    Terrain,
};
use expanse_worldgen::{
    FieldParams, NoiseParams, SurfaceParams, TemperatureParams, generate_surface,
};

fn surface(seed: u64) -> Terrain {
    let noise = NoiseParams {
        scale_tiles: 12.0,
        octaves: 3,
        persistence: 0.5,
    };
    let field = FieldParams {
        noise,
        smoothing_radius: 0,
    };
    let params = SurfaceParams {
        width_chunks: 2,
        height_chunks: 1,
        north_latitude_degrees: 40.0,
        south_latitude_degrees: 20.0,
        water_share: 0.4,
        elevation: field,
        temperature: TemperatureParams {
            equator_mean_celsius: 25.0,
            pole_mean_celsius: -15.0,
            equator_swing_celsius: 2.0,
            pole_swing_celsius: 35.0,
            cooling_at_highest_celsius: 10.0,
            noise,
            noise_strength_celsius: 2.0,
            smoothing_radius: 1,
        },
        moisture: field,
        vegetation: field,
        min_region_tiles: 4,
    };
    let obstacles = |kind: &str, density| {
        vec![BiomeObstacleDefinition {
            kind: kind.to_owned(),
            density,
            variation: None,
        }]
    };
    let table = BiomeTable::new(
        vec![
            BiomeDefinition {
                name: "water".to_owned(),
                ranges: FieldRanges {
                    elevation: Some(-1.0..0.0),
                    ..FieldRanges::default()
                },
                map_colour: [0, 0, 255],
                impassable: true,
                obstacles: Vec::new(),
            },
            BiomeDefinition {
                name: "land".to_owned(),
                ranges: FieldRanges::default(),
                map_colour: [0, 255, 0],
                impassable: false,
                obstacles: obstacles("log", 0.1),
            },
        ],
        vec![ObstacleKindDefinition {
            name: "log".to_owned(),
            footprint: vec![TileOffset::new(1, 0)],
        }],
    )
    .expect("a valid table");
    generate_surface(seed, &params, &table).expect("valid parameters")
}

fn world(seed: u64) -> World {
    let config = WorldConfig {
        seed,
        ticks_per_step: 5,
    };
    World::new(config, surface(seed)).expect("a surface with passable tiles")
}

/// The determinism invariant: catches any source of variation between runs,
/// in generation or in the simulation, such as a randomly seeded hasher or
/// iteration over an unordered container.
#[test]
fn the_same_seed_gives_the_same_state() {
    let run = |seed| {
        let mut world = world(seed);
        for _ in 0..500 {
            world.advance();
        }
        world
    };
    assert_eq!(run(3), run(3));
    assert_eq!(run(3).state_hash(), run(3).state_hash());
}

/// Catches a wanderer that starts or steps onto water or an obstacle, or
/// that moves on other terrain than the world's, by checking it against the
/// terrain that presentation is given.
#[test]
fn the_wanderer_never_stands_on_a_blocked_tile() {
    for seed in 0..5 {
        let mut world = world(seed);
        for _ in 0..2_000 {
            let snapshot = world.snapshot();
            let motion = snapshot.entities[0].motion;
            assert!(!snapshot.terrain.is_blocked(motion.from), "{motion:?}");
            assert!(!snapshot.terrain.is_blocked(motion.to), "{motion:?}");
            world.advance();
        }
    }
}
