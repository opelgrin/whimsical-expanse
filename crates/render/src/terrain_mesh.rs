//! Terrain tile layering and per-chunk textured instance building.
//!
//! Each tile draws its layers from bottom to top: the lowest ground present in
//! its 3×3 neighbourhood as a full tile, then each higher ground present in
//! that neighbourhood up to its own ground as a blob tile. A layer's mask
//! counts the neighbours whose ground is at that layer or above it in the
//! drawing order. Neighbours wrap east–west across the seam, and missing
//! neighbours beyond the north or south edge count as present.

use bytemuck::{Pod, Zeroable};
use expanse_assets::blob::{Neighbour, NeighbourMask, Slot, slot};
use expanse_core::grid::TilePos;
use expanse_terrain::{CHUNK_SIZE, ChunkPos, Terrain};

use crate::assets::{Assets, BlockId, GroundRank};

/// One textured tile-layer instance, positioned relative to its chunk's origin.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub(crate) struct TerrainInstance {
    /// Packed bytes: `local_x | (local_y << 8) | (slot_col << 16) | (slot_row << 24)`.
    pub tile_and_slot: u32,
    /// Lower 16 bits: [`BlockId`] in the GPU block table.
    /// Upper 16 bits: reserved (0 until item 6 adds per-tile seasonal mode).
    pub block_and_flags: u32,
}

impl TerrainInstance {
    /// Packs a chunk-local tile coordinate, a blob slot, and a resolved block
    /// into one GPU instance.
    pub(crate) const fn new(local_x: u32, local_y: u32, tile_slot: Slot, block: BlockId) -> Self {
        let tile_and_slot = (local_x & 0xFF)
            | ((local_y & 0xFF) << 8)
            | ((tile_slot.column & 0xFF) << 16)
            | ((tile_slot.row & 0xFF) << 24);
        Self {
            tile_and_slot,
            block_and_flags: block.0 as u32,
        }
    }
}

/// A contiguous range of instances in a [`ChunkMesh`] that sample one atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AtlasBatch {
    pub atlas: u16,
    pub start: u32,
    pub count: u32,
}

/// Every textured tile-layer instance for one chunk, grouped into draw batches
/// that preserve bottom-to-top layer order on every tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChunkMesh {
    pub instances: Vec<TerrainInstance>,
    pub batches: Vec<AtlasBatch>,
}

/// One layer of a single tile, from bottom to top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TileLayer {
    pub rank: GroundRank,
    pub block: BlockId,
    pub mask: NeighbourMask,
    pub slot: Slot,
}

/// Computes the bottom-to-top layers for the tile at `pos`, writing them into
/// `out`. Clears `out` first.
pub(crate) fn tile_layers(
    terrain: &Terrain,
    assets: &Assets,
    pos: TilePos,
    out: &mut Vec<TileLayer>,
) {
    out.clear();
    let Some(tile) = terrain.tile(pos) else {
        return;
    };
    let own_rank = assets.biome_ground(tile.biome);
    let neighbour_ranks = Neighbour::ALL.map(|neighbour| {
        let (dx, dy) = neighbour.offset();
        terrain
            .tile(TilePos::new(pos.x + dx, pos.y + dy))
            .map(|neighbour_tile| assets.biome_ground(neighbour_tile.biome))
    });

    // Distinct grounds in the 3×3 neighbourhood up to the tile's own ground.
    let mut ranks = [own_rank; 9];
    let mut count = 1_usize;
    for rank in neighbour_ranks.into_iter().flatten() {
        if rank < own_rank && !ranks[..count].contains(&rank) {
            ranks[count] = rank;
            count += 1;
        }
    }
    ranks[..count].sort_unstable();

    // Base layer: lowest ground in the 3×3 neighbourhood as a full tile.
    let full = NeighbourMask::from_bits(u8::MAX);
    out.push(TileLayer {
        rank: ranks[0],
        block: assets.ground_block(ranks[0]),
        mask: full,
        slot: slot(full),
    });

    // Each higher ground present in the neighbourhood, up to the tile's own.
    for index in 1..count {
        let rank = ranks[index];
        let lower = ranks[index - 1];
        let mut neighbour_index = 0_usize;
        let mask = NeighbourMask::from_fn(|_| {
            // Missing neighbours beyond the north or south edge count as present.
            let present = neighbour_ranks[neighbour_index]
                .is_none_or(|neighbour_rank| neighbour_rank >= rank);
            neighbour_index += 1;
            present
        });
        out.push(TileLayer {
            rank,
            block: assets.layer_block(rank, lower),
            mask,
            slot: slot(mask),
        });
    }
}

/// Builds the textured tile-layer instances for canonical `chunk`.
///
/// Tiles in a chunk do not overlap each other, so grouping instances by layer
/// depth and then by atlas page minimizes page switches while strictly keeping
/// every tile's layers drawn from bottom to top.
pub(crate) fn build_chunk_mesh(terrain: &Terrain, assets: &Assets, chunk: ChunkPos) -> ChunkMesh {
    let origin = chunk.origin();
    let mut by_depth: [Vec<(u16, TerrainInstance)>; 9] = std::array::from_fn(|_| Vec::new());
    let mut layers = Vec::with_capacity(9);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let pos = TilePos::new(origin.x + local_x, origin.y + local_y);
            tile_layers(terrain, assets, pos, &mut layers);
            let lx = u32::try_from(local_x).expect("local chunk coordinate is non-negative");
            let ly = u32::try_from(local_y).expect("local chunk coordinate is non-negative");
            for (depth, layer) in layers.iter().enumerate() {
                let atlas = assets.block(layer.block).atlas;
                let instance = TerrainInstance::new(lx, ly, layer.slot, layer.block);
                by_depth[depth].push((atlas, instance));
            }
        }
    }

    let total: usize = by_depth.iter().map(Vec::len).sum();
    let mut instances = Vec::with_capacity(total);
    let mut batches: Vec<AtlasBatch> = Vec::new();

    for mut depth_bucket in by_depth {
        if depth_bucket.is_empty() {
            continue;
        }
        depth_bucket.sort_by_key(|&(atlas, _)| atlas);
        for (atlas, instance) in depth_bucket {
            let start = u32::try_from(instances.len()).expect("chunk instance count fits in u32");
            instances.push(instance);
            if let Some(last) = batches.last_mut()
                && last.atlas == atlas
            {
                last.count += 1;
            } else {
                batches.push(AtlasBatch {
                    atlas,
                    start,
                    count: 1,
                });
            }
        }
    }

    ChunkMesh { instances, batches }
}

#[cfg(test)]
mod tests {
    use expanse_assets::{
        Container, GroundEntry, GroundIndex, GroundPairEntry, PADDING, Region, SpriteEntry,
    };
    use expanse_core::asset::{GroundId, SpriteId};
    use expanse_terrain::{BiomeDefinition, BiomeIndex, BiomeTable, FieldRanges, Fields, Tile};

    use crate::assets::AssetContent;

    use super::*;

    fn ground_id(name: &str) -> GroundId {
        GroundId::new(name).expect("valid ground name")
    }

    fn sprite_id(name: &str) -> SpriteId {
        SpriteId::new(name).expect("valid sprite name")
    }

    /// Four grounds in drawing order: `deep < shallow < sand < grass`, with an
    /// optional list of pair overrides.
    fn assets_with_pairs(pairs: &[(&str, &str)]) -> Assets {
        let names = ["deep", "shallow", "sand", "grass"];
        let region = Region {
            atlas: 0,
            x: PADDING,
            y: PADDING,
        };
        let grounds: Vec<GroundEntry> = names
            .iter()
            .map(|name| GroundEntry::new(ground_id(name), vec![vec![region]]))
            .collect();
        let index_of = |name: &str| {
            let position = names
                .iter()
                .position(|&candidate| candidate == name)
                .expect("known test ground");
            GroundIndex(u16::try_from(position).expect("small"))
        };
        let ground_pairs: Vec<GroundPairEntry> = pairs
            .iter()
            .map(|&(higher, lower)| {
                GroundPairEntry::new(index_of(higher), index_of(lower), vec![vec![region]])
            })
            .collect();
        let sprites = vec![SpriteEntry::new(
            sprite_id("walker"),
            [16, 16],
            [0, 0],
            vec![vec![region]],
        )];
        let container = Container::new(Vec::new(), grounds, ground_pairs, sprites);
        let biomes: Vec<BiomeDefinition> = names
            .iter()
            .enumerate()
            .map(|(index, name)| BiomeDefinition {
                name: (*name).to_owned(),
                ranges: if index + 1 == names.len() {
                    FieldRanges::default()
                } else {
                    FieldRanges {
                        elevation: Some(0.0..0.1),
                        ..FieldRanges::default()
                    }
                },
                map_colour: [0, 0, 0],
                ground: ground_id(name),
                impassable: false,
                obstacles: Vec::new(),
            })
            .collect();
        let table = BiomeTable::new(biomes, Vec::new()).expect("valid table");
        let content = AssetContent {
            drawing_order: names.iter().map(|name| ground_id(name)).collect(),
            wanderer_sprite: sprite_id("walker"),
        };
        Assets::resolve(container, &table, &content).expect("assets resolve")
    }

    fn uniform_terrain(biome: BiomeIndex) -> Terrain {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        Terrain::new(1, 1, |_| Tile::new(fields, biome, false)).expect("valid terrain")
    }

    const DEEP: BiomeIndex = BiomeIndex(0);
    const SHALLOW: BiomeIndex = BiomeIndex(1);
    const SAND: BiomeIndex = BiomeIndex(2);
    const GRASS: BiomeIndex = BiomeIndex(3);

    fn layers_at(terrain: &Terrain, assets: &Assets, x: i32, y: i32) -> Vec<TileLayer> {
        let mut out = Vec::new();
        tile_layers(terrain, assets, TilePos::new(x, y), &mut out);
        out
    }

    /// Catches neighbour lookups that fail to wrap across the seam, or that
    /// treat tiles beyond the north and south edges as absent (which would
    /// draw a false shore along the world's top and bottom borders).
    #[test]
    fn masks_wrap_across_the_seam_and_count_missing_edge_neighbours_as_present() {
        let assets = assets_with_pairs(&[]);
        let mut terrain = uniform_terrain(GRASS);
        let width = terrain.grid().width();
        let height = terrain.grid().height();

        // Place shallow water on the east edge at y = 0 (north edge) and at
        // y = height - 1 (south edge), immediately west of column 0 across the seam.
        terrain.set_biome(TilePos::new(width - 1, 0), SHALLOW, false);
        terrain.set_biome(TilePos::new(width - 1, height - 1), SHALLOW, false);

        // Tile (0, 0): West neighbour wraps to (width - 1, 0) = SHALLOW.
        // North/NE/NW are beyond the north edge -> count as present.
        let north_seam = layers_at(&terrain, &assets, 0, 0);
        assert_eq!(north_seam.len(), 2);
        assert_eq!(north_seam[0].rank, GroundRank(1));
        assert_eq!(north_seam[0].mask, NeighbourMask::from_bits(u8::MAX));
        assert_eq!(north_seam[1].rank, GroundRank(3));
        assert_eq!(
            north_seam[1].mask,
            NeighbourMask::from_fn(|n| n != Neighbour::West)
        );

        // Tile (0, height - 1): West wraps to (width - 1, height - 1) = SHALLOW.
        // South/SE/SW are beyond the south edge -> count as present.
        let south_seam = layers_at(&terrain, &assets, 0, height - 1);
        assert_eq!(south_seam.len(), 2);
        assert_eq!(
            south_seam[1].mask,
            NeighbourMask::from_fn(|n| n != Neighbour::West)
        );

        // A tile at the north edge surrounded by its own ground has no edge
        // along the top of the world (only one full layer).
        let interior_north = layers_at(&terrain, &assets, 5, 0);
        assert_eq!(interior_north.len(), 1);
        assert_eq!(interior_north[0].rank, GroundRank(3));
        assert_eq!(interior_north[0].mask, NeighbourMask::from_bits(u8::MAX));
    }

    /// Catches missing or extra layers, layers emitted in the wrong order,
    /// higher neighbours wrongly drawn onto a lower tile, or masks that fail
    /// to count grounds at or above each layer.
    #[test]
    fn a_tile_draws_exact_layers_from_lowest_neighbour_up_to_its_own_ground() {
        let assets = assets_with_pairs(&[]);
        let mut terrain = uniform_terrain(SAND);

        // Around (5, 5) which is SAND (rank 2):
        // - West is DEEP (rank 0)
        // - North is SHALLOW (rank 1)
        // - East is GRASS (rank 3, higher than SAND -> must not be drawn on (5, 5))
        terrain.set_biome(TilePos::new(4, 5), DEEP, false);
        terrain.set_biome(TilePos::new(5, 4), SHALLOW, false);
        terrain.set_biome(TilePos::new(6, 5), GRASS, false);

        let layers = layers_at(&terrain, &assets, 5, 5);
        assert_eq!(layers.len(), 3);

        // Layer 0: DEEP as full tile.
        assert_eq!(layers[0].rank, GroundRank(0));
        assert_eq!(layers[0].mask, NeighbourMask::from_bits(u8::MAX));

        // Layer 1: SHALLOW; every neighbour except West has rank >= 1.
        assert_eq!(layers[1].rank, GroundRank(1));
        assert_eq!(
            layers[1].mask,
            NeighbourMask::from_fn(|n| n != Neighbour::West)
        );

        // Layer 2: SAND (own); every neighbour except West and North has rank >= 2
        // (East is GRASS = 3 >= 2, so East counts as present).
        assert_eq!(layers[2].rank, GroundRank(2));
        assert_eq!(
            layers[2].mask,
            NeighbourMask::from_fn(|n| n != Neighbour::West && n != Neighbour::North)
        );
    }

    /// Catches pair blocks applied over the wrong lower ground, or skipped
    /// when the pair's lower ground is immediately beneath the layer.
    #[test]
    fn a_pair_block_is_used_only_immediately_over_its_lower_ground() {
        let assets = assets_with_pairs(&[("grass", "shallow")]);
        let grass_own = assets.ground_block(GroundRank(3));
        let grass_over_shallow = assets.layer_block(GroundRank(3), GroundRank(1));
        assert_ne!(
            grass_over_shallow, grass_own,
            "the pair must resolve to its own block"
        );

        // 1. Grass directly over shallow water -> uses pair block.
        let mut over_shallow = uniform_terrain(GRASS);
        over_shallow.set_biome(TilePos::new(4, 5), SHALLOW, false);
        let layers = layers_at(&over_shallow, &assets, 5, 5);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[1].block, grass_over_shallow);

        // 2. Grass over deep water (no shallow water) -> uses grass's own block.
        let mut over_deep = uniform_terrain(GRASS);
        over_deep.set_biome(TilePos::new(4, 5), DEEP, false);
        let layers = layers_at(&over_deep, &assets, 5, 5);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[1].block, grass_own);

        // 3. Deep, shallow, and grass meet -> shallow is directly beneath grass
        //    -> uses pair block.
        let mut deep_and_shallow = uniform_terrain(GRASS);
        deep_and_shallow.set_biome(TilePos::new(4, 5), DEEP, false);
        deep_and_shallow.set_biome(TilePos::new(5, 4), SHALLOW, false);
        let layers = layers_at(&deep_and_shallow, &assets, 5, 5);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[1].block, assets.ground_block(GroundRank(1)));
        assert_eq!(layers[2].block, grass_over_shallow);

        // 4. Shallow, sand, and grass meet -> sand is immediately beneath grass
        //    (not shallow) -> uses grass's own block.
        let mut with_sand_between = uniform_terrain(GRASS);
        with_sand_between.set_biome(TilePos::new(4, 5), SHALLOW, false);
        with_sand_between.set_biome(TilePos::new(5, 4), SAND, false);
        let layers = layers_at(&with_sand_between, &assets, 5, 5);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[2].block, grass_own);
    }

    /// Catches chunk mesh batching that drops instances, mis-packs local tile
    /// coordinates or slots, or emits upper layers before base layers.
    #[test]
    fn building_a_chunk_mesh_emits_base_layers_before_transitions_in_coalesced_batches() {
        let assets = assets_with_pairs(&[]);
        let mut terrain = uniform_terrain(GRASS);
        // One shallow-water tile creates a transition layer on its 8 neighbours.
        terrain.set_biome(TilePos::new(5, 5), SHALLOW, false);

        let mesh = build_chunk_mesh(&terrain, &assets, ChunkPos { x: 0, y: 0 });
        let tile_count = usize::try_from(CHUNK_SIZE * CHUNK_SIZE).expect("fits in usize");
        // Every tile emits 1 base layer; the 8 neighbours of (5, 5) also emit
        // 1 grass transition layer.
        assert_eq!(mesh.instances.len(), tile_count + 8);
        // All grounds fit on one atlas page -> coalesced into a single draw batch.
        assert_eq!(
            mesh.batches,
            vec![AtlasBatch {
                atlas: 0,
                start: 0,
                count: u32::try_from(tile_count + 8).expect("fits in u32"),
            }]
        );
        // Base layers come first; transition layers follow after all base layers.
        let grass_block = u32::from(assets.ground_block(GroundRank(3)).0);
        let full_slot = slot(NeighbourMask::from_bits(u8::MAX));
        let east_neighbour = &mesh.instances[tile_count..];
        assert!(east_neighbour.iter().all(|instance| {
            let slot_col = (instance.tile_and_slot >> 16) & 0xFF;
            let slot_row = (instance.tile_and_slot >> 24) & 0xFF;
            instance.block_and_flags == grass_block
                && (slot_col != full_slot.column || slot_row != full_slot.row)
        }));
    }
}
