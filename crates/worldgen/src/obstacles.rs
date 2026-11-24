//! The obstacle pass: places the obstacles of each biome.

use expanse_core::rng::{stream_key, unit_interval};
use expanse_terrain::{BiomeIndex, BiomeTable, ObstacleIndex, Terrain};

use crate::noise::Noise;
use crate::params::NoiseParams;
use crate::space::{Space, stream};

/// Places obstacles, visiting the tiles in row-major order.
///
/// For each obstacle kind of the tile's biome, a draw from randomness derived
/// from the seed, the tile and the kind decides whether the tile anchors one,
/// with the kind's density, varied by its variation, as the chance. The
/// obstacle is placed only if every tile of its footprint is free and has a
/// biome that lists the kind. Otherwise it is skipped, so the density is an
/// upper bound.
pub(crate) fn place(
    terrain: &mut Terrain,
    space: &Space,
    table: &BiomeTable,
    biomes: &[BiomeIndex],
) {
    let grid = space.grid();
    let draws = space.stream(&[stream::OBSTACLE]);
    let lists_kind = |biome: BiomeIndex, kind: ObstacleIndex| {
        table
            .biome(biome)
            .obstacles()
            .iter()
            .any(|obstacle| obstacle.kind == kind)
    };
    let variations = variation_noises(space, table);

    for (index, &biome) in biomes.iter().enumerate() {
        let anchor = grid.tile_at(index);
        for (position, obstacle) in table.biome(biome).obstacles().iter().enumerate() {
            let variation = variations[usize::from(biome.0)][position]
                .as_ref()
                .map_or(0.0, |(noise, strength)| strength * noise.at(space, index));
            let chance = obstacle.density * (1.0 + variation);
            let draw = stream_key(
                draws,
                &[
                    i64::from(anchor.x).cast_unsigned(),
                    i64::from(anchor.y).cast_unsigned(),
                    u64::from(obstacle.kind.0),
                ],
            );
            if unit_interval(draw) >= chance {
                continue;
            }
            let footprint = table.obstacle_kind(obstacle.kind).footprint();
            let fits = footprint.offsets().all(|offset| {
                grid.translate(anchor, offset)
                    .and_then(|tile| grid.index(tile))
                    .is_some_and(|tile| lists_kind(biomes[tile], obstacle.kind))
            });
            if fits
                && terrain
                    .place_obstacle(anchor, obstacle.kind, footprint)
                    .is_ok()
            {
                break;
            }
        }
    }
}

/// For each biome, and each of its obstacle kinds, the noise of its variation
/// and the variation's strength, if it has one.
fn variation_noises(space: &Space, table: &BiomeTable) -> Vec<Vec<Option<(Noise, f64)>>> {
    (0..table.biome_count())
        .map(|biome| {
            let index =
                BiomeIndex(u16::try_from(biome).expect("a table has fewer than 2^16 biomes"));
            table
                .biome(index)
                .obstacles()
                .iter()
                .map(|obstacle| {
                    let variation = obstacle.variation?;
                    let seed = space.stream(&[
                        stream::OBSTACLE_VARIATION,
                        u64::from(index.0),
                        u64::from(obstacle.kind.0),
                    ]);
                    let params = NoiseParams {
                        scale_tiles: variation.scale_tiles,
                        octaves: 1,
                        persistence: 1.0,
                    };
                    Some((Noise::new(seed, params), variation.strength))
                })
                .collect()
        })
        .collect()
}
