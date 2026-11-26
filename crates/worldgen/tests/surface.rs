//! Properties of generated surfaces, over many seeds and small worlds.
//!
//! The tests check properties only: no golden maps, no expected biome
//! proportions and no values at a fixed seed, so that tuning the generator
//! never breaks them.

use expanse_core::asset::{GroundId, SpriteId};
use expanse_core::grid::{TileOffset, TilePos};
use expanse_core::region::Regions;
use expanse_terrain::{
    BiomeDefinition, BiomeObstacleDefinition, BiomeTable, Field, FieldRanges, ObstacleCell,
    ObstacleKindDefinition, SEA_LEVEL, Terrain, Variation,
};
use expanse_worldgen::{
    FieldParams, NoiseParams, SurfaceParams, TemperatureParams, generate_surface,
};

const SEEDS: std::ops::Range<u64> = 0..8;

fn noise(scale_tiles: f64) -> NoiseParams {
    NoiseParams {
        scale_tiles,
        octaves: 4,
        persistence: 0.5,
    }
}

fn params() -> SurfaceParams {
    SurfaceParams {
        width_chunks: 3,
        height_chunks: 2,
        north_latitude_degrees: 60.0,
        south_latitude_degrees: -35.0,
        water_share: 0.3,
        elevation: FieldParams {
            noise: noise(24.0),
            smoothing_radius: 1,
        },
        temperature: TemperatureParams {
            equator_mean_celsius: 26.0,
            pole_mean_celsius: -20.0,
            equator_swing_celsius: 2.0,
            pole_swing_celsius: 40.0,
            cooling_at_highest_celsius: 15.0,
            noise: noise(16.0),
            noise_strength_celsius: 3.0,
            smoothing_radius: 2,
        },
        moisture: FieldParams {
            noise: noise(20.0),
            smoothing_radius: 0,
        },
        vegetation: FieldParams {
            noise: noise(12.0),
            smoothing_radius: 1,
        },
        min_region_tiles: 6,
    }
}

fn biome(
    name: &str,
    ranges: FieldRanges,
    impassable: bool,
    obstacles: &[(&str, f64)],
) -> BiomeDefinition {
    BiomeDefinition {
        name: name.to_owned(),
        ranges,
        map_colour: [0, 0, 0],
        ground: GroundId::new(name).expect("a valid name"),
        impassable,
        obstacles: obstacles
            .iter()
            .map(|&(kind, density)| BiomeObstacleDefinition {
                kind: kind.to_owned(),
                density,
                variation: (kind == "single").then_some(Variation {
                    scale_tiles: 6.0,
                    strength: 0.8,
                }),
            })
            .collect(),
    }
}

/// A table with dense obstacles, several of them wider than a tile and
/// listed by several biomes, so that footprints meet borders and each other.
fn table() -> BiomeTable {
    let kind = |name: &str, footprint: &[(i32, i32)]| ObstacleKindDefinition {
        name: name.to_owned(),
        footprint: footprint
            .iter()
            .map(|&(dx, dy)| TileOffset::new(dx, dy))
            .collect(),
        sprite: SpriteId::new(name).expect("a valid name"),
    };
    BiomeTable::new(
        vec![
            biome(
                "water",
                FieldRanges {
                    elevation: Some(-1.0..0.0),
                    ..FieldRanges::default()
                },
                true,
                &[],
            ),
            biome(
                "high",
                FieldRanges {
                    elevation: Some(0.8..1.0),
                    ..FieldRanges::default()
                },
                false,
                &[("square", 0.2), ("single", 0.2)],
            ),
            biome(
                "cold",
                FieldRanges {
                    winter_temperature: Some(-150.0..-5.0),
                    ..FieldRanges::default()
                },
                false,
                &[("long", 0.15), ("square", 0.1)],
            ),
            biome(
                "wooded",
                FieldRanges {
                    vegetation: Some(0.5..1.0),
                    moisture: Some(0.3..1.0),
                    ..FieldRanges::default()
                },
                false,
                &[("single", 0.4), ("long", 0.1)],
            ),
            biome("open", FieldRanges::default(), false, &[("long", 0.1)]),
        ],
        vec![
            kind("single", &[]),
            kind("long", &[(1, 0), (2, 0)]),
            kind("square", &[(1, 0), (0, 1), (1, 1)]),
        ],
    )
    .expect("a valid table")
}

fn generate(seed: u64, params: &SurfaceParams) -> Terrain {
    generate_surface(seed, params, &table()).expect("valid parameters")
}

/// Every canonical tile of the terrain, with its row-major index.
fn tiles(terrain: &Terrain) -> impl Iterator<Item = (usize, TilePos)> + '_ {
    let grid = terrain.grid();
    (0..grid.tile_count()).map(move |index| (index, grid.tile_at(index)))
}

fn field(terrain: &Terrain, tile: TilePos, field: Field) -> u16 {
    terrain.tile(tile).expect("a tile").fields.get(field)
}

/// The reproducibility invariant for generation: catches any source of
/// variation, such as iteration over an unordered container, and a seed
/// that is ignored.
#[test]
fn the_same_seed_and_parameters_give_the_same_terrain() {
    let params = params();
    for seed in SEEDS {
        assert_eq!(
            generate(seed, &params),
            generate(seed, &params),
            "seed {seed}"
        );
    }
    assert_ne!(generate(1, &params), generate(2, &params));
}

/// Catches parallel work whose result depends on how it is split, such as a
/// floating-point sum, or on which thread finishes first.
#[test]
fn the_number_of_threads_does_not_change_the_terrain() {
    let params = params();
    let generate_on = |threads: usize, seed: u64| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("a thread pool")
            .install(|| generate(seed, &params))
    };
    for seed in SEEDS {
        assert_eq!(generate_on(1, seed), generate_on(7, seed), "seed {seed}");
    }
}

/// Catches ranks that are not spread evenly over their range, water and
/// land ranked together, and a share under water other than the
/// parameter's. Spread evenly, the share of tiles below a value is that
/// value's share of the range, within the rounding of ranks to values.
#[test]
fn ranked_fields_are_spread_evenly_and_the_water_share_holds() {
    let params = params();
    for seed in SEEDS {
        let terrain = generate(seed, &params);
        let count = terrain.grid().tile_count();
        let is_water = |tile| field(&terrain, tile, Field::Elevation) < SEA_LEVEL;
        let (water, land): (Vec<TilePos>, Vec<TilePos>) = tiles(&terrain)
            .map(|(_, tile)| tile)
            .partition(|&tile| is_water(tile));
        #[expect(clippy::cast_precision_loss, reason = "small test worlds")]
        let expected_water = (params.water_share * count as f64).round();
        #[expect(clippy::cast_precision_loss, reason = "small test worlds")]
        let water_count = water.len() as f64;
        assert_eq!(water_count, expected_water, "seed {seed}");

        let checks = [
            (&land, Field::Moisture, 0.0, 65_536.0),
            (&land, Field::Vegetation, 0.0, 65_536.0),
            (&water, Field::Moisture, 0.0, 65_536.0),
            (&land, Field::Elevation, 32_768.0, 32_768.0),
            (&water, Field::Elevation, 0.0, 32_768.0),
        ];
        for (group, checked, low, span) in checks {
            #[expect(clippy::cast_precision_loss, reason = "small test worlds")]
            let group_size = group.len() as f64;
            for tenth in 1..10 {
                let share = f64::from(tenth) / 10.0;
                let below = group
                    .iter()
                    .filter(|&&tile| f64::from(field(&terrain, tile, checked)) < low + share * span)
                    .count();
                #[expect(clippy::cast_precision_loss, reason = "small test worlds")]
                let measured = below as f64 / group_size;
                assert!(
                    (measured - share).abs() <= 2.0 / group_size,
                    "seed {seed}, {checked:?}: {measured} below {share} of the range"
                );
            }
        }
    }
}

/// Catches a seam: noise that is not sampled around a cylinder, or
/// smoothing that does not wrap. Across the seam, the fields change no more
/// than between other neighbouring columns. Any single column may be the
/// roughest, so the change is summed over many seeds and compared with the
/// average column; a seam changes several times more.
#[test]
fn fields_change_across_the_seam_no_more_than_between_other_columns() {
    let params = params();
    let terrains: Vec<Terrain> = SEEDS.map(|seed| generate(seed, &params)).collect();
    let grid = terrains[0].grid();
    for checked in Field::ALL {
        let column_change = |x: i32| -> f64 {
            let total: u64 = terrains
                .iter()
                .flat_map(|terrain| {
                    (0..grid.height()).map(move |y| {
                        let here = field(terrain, TilePos::new(x, y), checked);
                        let east = field(terrain, TilePos::new(x + 1, y), checked);
                        u64::from(here.abs_diff(east))
                    })
                })
                .sum();
            #[expect(clippy::cast_precision_loss, reason = "small test worlds")]
            let total = total as f64;
            total
        };
        let seam = column_change(grid.width() - 1);
        let elsewhere: f64 =
            (0..grid.width() - 1).map(column_change).sum::<f64>() / f64::from(grid.width() - 1);
        assert!(
            seam <= 1.5 * elsewhere,
            "{checked:?}: {seam} across the seam, {elsewhere} on average elsewhere"
        );
    }
}

/// Catches a climate that ignores latitude, or follows it the wrong way:
/// on average, rows further from the equator have a lower annual mean and a
/// wider swing.
#[test]
fn rows_further_from_the_equator_are_colder_with_a_wider_swing() {
    let params = params();
    for seed in SEEDS {
        let terrain = generate(seed, &params);
        let grid = terrain.grid();
        let mut rows: Vec<(f64, f64, f64)> = (0..grid.height())
            .map(|y| {
                let latitude = terrain.row_latitude(y).expect("the surface has latitudes");
                let (mut mean, mut swing) = (0.0, 0.0);
                for x in 0..grid.width() {
                    let winter = Field::WinterTemperature.to_unit(field(
                        &terrain,
                        TilePos::new(x, y),
                        Field::WinterTemperature,
                    ));
                    let summer = Field::SummerTemperature.to_unit(field(
                        &terrain,
                        TilePos::new(x, y),
                        Field::SummerTemperature,
                    ));
                    mean += f64::midpoint(winter, summer);
                    swing += summer - winter;
                }
                (latitude.degrees().abs(), mean, swing)
            })
            .collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        let halves: Vec<(f64, f64)> = rows
            .chunks(rows.len() / 2)
            .map(|half| {
                half.iter().fold((0.0, 0.0), |(mean, swing), row| {
                    (mean + row.1, swing + row.2)
                })
            })
            .collect();
        let (nearer, further) = (halves[0], halves[1]);
        assert!(further.0 < nearer.0, "seed {seed}: annual mean {halves:?}");
        assert!(further.1 > nearer.1, "seed {seed}: swing {halves:?}");
    }
}

/// Catches tiles without a valid biome, an impassable flag that disagrees
/// with the table, and clean-up that stops early or misses regions joined
/// across the seam.
#[test]
fn every_tile_has_a_biome_and_no_region_is_smaller_than_the_minimum() {
    let params = params();
    let table = table();
    for seed in SEEDS {
        let terrain = generate(seed, &params);
        let grid = terrain.grid();
        for (_, tile) in tiles(&terrain) {
            let tile = terrain.tile(tile).expect("a tile");
            assert!(usize::from(tile.biome.0) < table.biome_count());
            assert_eq!(tile.impassable, table.biome(tile.biome).is_impassable());
        }
        let regions = Regions::find(grid, |index| {
            Some(terrain.tile(grid.tile_at(index)).expect("a tile").biome)
        });
        let min_tiles = usize::try_from(params.min_region_tiles).expect("a small minimum");
        for region in 0..regions.count() {
            assert!(regions.tiles(region).len() >= min_tiles, "seed {seed}");
        }
    }
}

/// Catches overlapping footprints, footprints cut off or written wrongly,
/// obstacles in a biome that does not list them, and parts without an
/// anchor. Rebuilds every footprint from its anchor and the table.
#[test]
fn footprints_never_overlap_and_lie_in_biomes_that_list_their_kind() {
    let params = params();
    let table = table();
    let mut wide_obstacles = 0;
    for seed in SEEDS {
        let terrain = generate(seed, &params);
        let grid = terrain.grid();
        let mut claimed = vec![false; grid.tile_count()];
        for (_, anchor) in tiles(&terrain) {
            let ObstacleCell::Anchor(kind) = terrain.tile(anchor).expect("a tile").obstacle()
            else {
                continue;
            };
            let footprint = table.obstacle_kind(kind).footprint();
            if footprint.offsets().count() > 1 {
                wide_obstacles += 1;
            }
            for offset in footprint.offsets() {
                let part = grid
                    .translate(anchor, offset)
                    .expect("no footprint crosses the north or south edge");
                let index = grid.index(part).expect("a canonical tile");
                assert!(!claimed[index], "seed {seed}: {part:?} is claimed twice");
                claimed[index] = true;
                let tile = terrain.tile(part).expect("a tile");
                if offset != TileOffset::new(0, 0) {
                    let ObstacleCell::Part(back) = tile.obstacle() else {
                        panic!("seed {seed}: {part:?} holds {:?}", tile.obstacle());
                    };
                    assert_eq!(grid.translate(part, back.into()), Some(anchor));
                }
                let lists = table
                    .biome(tile.biome)
                    .obstacles()
                    .iter()
                    .any(|obstacle| obstacle.kind == kind);
                assert!(lists, "seed {seed}: {part:?}");
            }
        }
        for (index, tile) in tiles(&terrain) {
            let empty = terrain.tile(tile).expect("a tile").obstacle() == ObstacleCell::Empty;
            assert_eq!(!empty, claimed[index], "seed {seed}: {tile:?}");
        }
    }
    assert!(
        wide_obstacles > 0,
        "no footprint of several tiles was tested"
    );
}

/// Catches invalid parameters that would panic or generate nonsense instead
/// of being reported, and errors that do not name the parameter.
#[test]
fn invalid_parameters_are_rejected_and_named() {
    type Breaker = fn(&mut SurfaceParams);
    let table = table();
    let cases: [(&str, Breaker); 8] = [
        ("width_chunks", |p| p.width_chunks = 0),
        ("south_latitude_degrees", |p| {
            p.south_latitude_degrees = 70.0;
        }),
        ("north_latitude_degrees", |p| {
            p.north_latitude_degrees = 91.0;
        }),
        ("water_share", |p| p.water_share = f64::NAN),
        ("min_region_tiles", |p| p.min_region_tiles = 0),
        ("moisture.noise.octaves", |p| p.moisture.noise.octaves = 0),
        ("vegetation.smoothing_radius", |p| {
            p.vegetation.smoothing_radius = 48;
        }),
        ("temperature.pole_mean_celsius", |p| {
            p.temperature.pole_mean_celsius = -200.0;
        }),
    ];
    for (parameter, break_it) in cases {
        let mut params = params();
        break_it(&mut params);
        let error = generate_surface(1, &params, &table).expect_err(parameter);
        assert_eq!(error.parameter, parameter);
    }
}
