//! The content's art: every logical ID that the content names, resolved
//! against a cooked container when the game loads.
//!
//! Resolving checks everything at once and names every problem, so a missing
//! asset never shows up later as wrong art or a panic while drawing.

use std::error::Error;
use std::fmt;

use expanse_cook::{Container, GroundIndex, Region, SEASONS, SpriteIndex};
use expanse_core::asset::{GroundId, SpriteId};
use expanse_core::hash::FixedHashMap;
use expanse_terrain::BiomeTable;

/// The logical asset IDs that presentation needs beyond the biome table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetContent {
    /// The grounds, from the lowest to the highest. Where two grounds meet,
    /// the edge is drawn on the higher one. Every biome's ground is listed.
    pub drawing_order: Vec<GroundId>,
    /// Stand-in: replaced by the species content of the first actors in M2.
    ///
    /// The sprite of the wandering placeholder entity.
    pub wanderer_sprite: SpriteId,
}

/// A ground's place in the drawing order: higher grounds are drawn over lower
/// ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroundRank(pub u16);

/// Index of a ground or ground-pair blob block in [`Assets::ground_blocks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u16);

/// One ground or ground-pair blob block, resolved for GPU lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundBlock {
    /// The atlas page holding every variant and frame of this block.
    pub atlas: u16,
    /// The top-left pixel (inside padding) of the first frame of each
    /// seasonal variant. Assets with a single variant repeat it in every slot.
    pub variant_origins: [[u32; 2]; SEASONS],
}

/// The content's art, resolved against a container.
#[derive(Debug)]
pub struct Assets {
    container: Container,
    /// For each ground rank, its ground in the container.
    grounds: Vec<GroundIndex>,
    /// For each biome, the rank of its ground.
    biome_grounds: Vec<GroundRank>,
    /// Every ground block (at `BlockId(rank.0)`) followed by any applicable
    /// ground-pair blocks.
    ground_blocks: Vec<GroundBlock>,
    /// Pair override blocks, keyed by `(higher, lower)`.
    pair_blocks: FixedHashMap<(GroundRank, GroundRank), BlockId>,
    /// For each obstacle kind, its sprite.
    obstacle_sprites: Vec<SpriteIndex>,
    wanderer_sprite: SpriteIndex,
}

impl Assets {
    /// Resolves every logical ID that `table` and `content` name against
    /// `container`.
    ///
    /// # Errors
    ///
    /// Returns every problem found: an ID the container lacks, a biome whose
    /// ground is not in the drawing order, or a ground listed twice there.
    ///
    /// # Panics
    ///
    /// Never: a biome table holds fewer than 2^16 biomes, so its grounds fit
    /// in a rank.
    pub fn resolve(
        container: Container,
        table: &BiomeTable,
        content: &AssetContent,
    ) -> Result<Self, MissingAssets> {
        let mut problems = Vec::new();
        for (rank, ground) in content.drawing_order.iter().enumerate() {
            if content.drawing_order[..rank].contains(ground) {
                problems.push(AssetProblem::RepeatedInDrawingOrder(ground.clone()));
            }
        }
        let grounds: Vec<GroundIndex> = content
            .drawing_order
            .iter()
            .filter_map(|ground| {
                let index = container.ground_index(ground);
                if index.is_none() {
                    problems.push(AssetProblem::MissingGround {
                        named_by: "the drawing order".to_owned(),
                        id: ground.clone(),
                    });
                }
                index
            })
            .collect();
        let biome_grounds: Vec<GroundRank> = (0..table.biome_count())
            .filter_map(|index| {
                let biome = table.biome(expanse_terrain::BiomeIndex(
                    u16::try_from(index).expect("a table has fewer than 2^16 biomes"),
                ));
                let rank = content
                    .drawing_order
                    .iter()
                    .position(|ground| ground == biome.ground());
                if rank.is_none() {
                    problems.push(AssetProblem::NotInDrawingOrder {
                        biome: biome.name().to_owned(),
                        ground: biome.ground().clone(),
                    });
                }
                rank.map(|rank| GroundRank(u16::try_from(rank).expect("fewer than 2^16 grounds")))
            })
            .collect();
        let mut sprite = |named_by: String, id: &SpriteId| {
            let index = container.sprite_index(id);
            if index.is_none() {
                problems.push(AssetProblem::MissingSprite {
                    named_by,
                    id: id.clone(),
                });
            }
            index
        };
        let obstacle_sprites: Vec<SpriteIndex> = (0..table.obstacle_kind_count())
            .filter_map(|index| {
                let kind = table.obstacle_kind(expanse_terrain::ObstacleIndex(
                    u16::try_from(index).expect("a table has fewer than 2^16 obstacle kinds"),
                ));
                sprite(format!("obstacle kind `{}`", kind.name()), kind.sprite())
            })
            .collect();
        let wanderer_sprite = sprite("the wanderer".to_owned(), &content.wanderer_sprite);

        match wanderer_sprite {
            Some(wanderer_sprite) if problems.is_empty() => {
                let (ground_blocks, pair_blocks) = resolve_ground_blocks(&container, &grounds);
                Ok(Self {
                    container,
                    grounds,
                    biome_grounds,
                    ground_blocks,
                    pair_blocks,
                    obstacle_sprites,
                    wanderer_sprite,
                })
            }
            _ => Err(MissingAssets { problems }),
        }
    }

    /// The container the art comes from.
    #[must_use]
    pub const fn container(&self) -> &Container {
        &self.container
    }

    /// The container's ground at `rank`.
    #[must_use]
    pub fn ground(&self, rank: GroundRank) -> GroundIndex {
        self.grounds[usize::from(rank.0)]
    }

    /// The rank of a biome's ground.
    #[must_use]
    pub fn biome_ground(&self, biome: expanse_terrain::BiomeIndex) -> GroundRank {
        self.biome_grounds[usize::from(biome.0)]
    }

    /// Every resolved ground and ground-pair block, indexed by [`BlockId`].
    #[must_use]
    pub fn ground_blocks(&self) -> &[GroundBlock] {
        &self.ground_blocks
    }

    /// The block of `rank` itself (used for the base layer or when no pair
    /// overrides it).
    #[must_use]
    pub const fn ground_block(&self, rank: GroundRank) -> BlockId {
        BlockId(rank.0)
    }

    /// The block to draw for `higher` immediately over `lower`: the pair's
    /// block if the container provides one, otherwise `higher`'s own block.
    #[must_use]
    pub fn layer_block(&self, higher: GroundRank, lower: GroundRank) -> BlockId {
        self.pair_blocks
            .get(&(higher, lower))
            .copied()
            .unwrap_or_else(|| self.ground_block(higher))
    }

    /// The resolved metadata for `block`.
    #[must_use]
    pub fn block(&self, block: BlockId) -> GroundBlock {
        self.ground_blocks[usize::from(block.0)]
    }

    /// The sprite of an obstacle kind.
    #[must_use]
    pub fn obstacle_sprite(&self, kind: expanse_terrain::ObstacleIndex) -> SpriteIndex {
        self.obstacle_sprites[usize::from(kind.0)]
    }

    /// The sprite of the wandering placeholder entity.
    #[must_use]
    pub const fn wanderer_sprite(&self) -> SpriteIndex {
        self.wanderer_sprite
    }
}

fn resolve_ground_blocks(
    container: &Container,
    grounds: &[GroundIndex],
) -> (
    Vec<GroundBlock>,
    FixedHashMap<(GroundRank, GroundRank), BlockId>,
) {
    let mut rank_of = FixedHashMap::default();
    let mut blocks = Vec::with_capacity(grounds.len() + container.ground_pairs().len());
    for (rank, &ground_index) in grounds.iter().enumerate() {
        let rank = GroundRank(u16::try_from(rank).expect("fewer than 2^16 grounds"));
        rank_of.insert(ground_index, rank);
        let entry = container.ground(ground_index);
        blocks.push(ground_block_from_variants(entry.atlas(), entry.variants()));
    }
    let mut pair_blocks = FixedHashMap::default();
    for pair in container.ground_pairs() {
        let (Some(&higher), Some(&lower)) =
            (rank_of.get(&pair.higher()), rank_of.get(&pair.lower()))
        else {
            continue;
        };
        if higher <= lower {
            continue;
        }
        let id = BlockId(u16::try_from(blocks.len()).expect("grounds plus pairs fit in u16"));
        blocks.push(ground_block_from_variants(pair.atlas(), pair.variants()));
        pair_blocks.insert((higher, lower), id);
    }
    (blocks, pair_blocks)
}

fn ground_block_from_variants(atlas: u16, variants: &[Vec<Region>]) -> GroundBlock {
    let origin = |variant: usize| {
        let region = variants[variant][0];
        [region.x, region.y]
    };
    let variant_origins = if variants.len() == 1 {
        [origin(0); SEASONS]
    } else {
        std::array::from_fn(origin)
    };
    GroundBlock {
        atlas,
        variant_origins,
    }
}

/// A logical ID that cannot be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetProblem {
    /// Content names a ground the container lacks.
    MissingGround { named_by: String, id: GroundId },
    /// Content names a sprite the container lacks.
    MissingSprite { named_by: String, id: SpriteId },
    /// A biome's ground is not in the drawing order.
    NotInDrawingOrder { biome: String, ground: GroundId },
    /// The drawing order lists a ground twice.
    RepeatedInDrawingOrder(GroundId),
}

impl fmt::Display for AssetProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingGround { named_by, id } => {
                write!(
                    f,
                    "{named_by} names ground `{id}`, which the container lacks"
                )
            }
            Self::MissingSprite { named_by, id } => {
                write!(
                    f,
                    "{named_by} names sprite `{id}`, which the container lacks"
                )
            }
            Self::NotInDrawingOrder { biome, ground } => write!(
                f,
                "biome `{biome}` has ground `{ground}`, which the drawing order does not list"
            ),
            Self::RepeatedInDrawingOrder(ground) => {
                write!(f, "the drawing order lists `{ground}` twice")
            }
        }
    }
}

/// Every logical ID that could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingAssets {
    pub problems: Vec<AssetProblem>,
}

impl fmt::Display for MissingAssets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the content does not match the cooked assets:")?;
        for problem in &self.problems {
            write!(f, "\n  - {problem}")?;
        }
        Ok(())
    }
}

impl Error for MissingAssets {}

#[cfg(test)]
mod tests {
    use expanse_cook::{BLOCK_SIZE, Image, SourceGround, SourceSprite, Sources, cook};
    use expanse_terrain::{BiomeDefinition, FieldRanges, ObstacleKindDefinition};

    use super::*;

    fn ground_id(name: &str) -> GroundId {
        GroundId::new(name).expect("a valid name")
    }

    fn sprite(name: &str) -> SpriteId {
        SpriteId::new(name).expect("a valid name")
    }

    /// A container holding the ground `grass` and the sprite `walker`.
    fn container() -> Container {
        let sources = Sources {
            grounds: vec![SourceGround {
                id: ground_id("grass"),
                variants: vec![vec![Image::new(BLOCK_SIZE[0], BLOCK_SIZE[1])]],
            }],
            pairs: Vec::new(),
            sprites: vec![SourceSprite {
                id: sprite("walker"),
                anchor: [0, 0],
                variants: vec![vec![Image::new(16, 16)]],
            }],
        };
        cook(&sources).expect("valid sources")
    }

    /// One biome on `ground`, with one obstacle kind drawn as `obstacle`.
    fn table(ground: &str, obstacle: &str) -> BiomeTable {
        BiomeTable::new(
            vec![BiomeDefinition {
                name: "plain".to_owned(),
                ranges: FieldRanges::default(),
                map_colour: [0, 0, 0],
                ground: ground_id(ground),
                impassable: false,
                obstacles: Vec::new(),
            }],
            vec![ObstacleKindDefinition {
                name: "stone".to_owned(),
                footprint: Vec::new(),
                sprite: sprite(obstacle),
            }],
        )
        .expect("a valid table")
    }

    fn content(drawing_order: &[&str]) -> AssetContent {
        AssetContent {
            drawing_order: drawing_order.iter().map(|name| ground_id(name)).collect(),
            wanderer_sprite: sprite("walker"),
        }
    }

    /// Catches content that names art the container lacks, or a ground left
    /// out of the drawing order, reaching the renderer, where it would draw
    /// the wrong art or panic. Every problem is reported, not only the first.
    #[test]
    fn every_unresolved_asset_is_reported() {
        let mismatched = table("sand", "pebble");
        let result = Assets::resolve(container(), &mismatched, &content(&["grass", "mud"]));
        let problems = result.expect_err("content names missing art").problems;
        assert_eq!(
            problems,
            vec![
                AssetProblem::MissingGround {
                    named_by: "the drawing order".to_owned(),
                    id: ground_id("mud"),
                },
                AssetProblem::NotInDrawingOrder {
                    biome: "plain".to_owned(),
                    ground: ground_id("sand"),
                },
                AssetProblem::MissingSprite {
                    named_by: "obstacle kind `stone`".to_owned(),
                    id: sprite("pebble"),
                },
            ]
        );

        let matching = table("grass", "walker");
        assert!(Assets::resolve(container(), &matching, &content(&["grass"])).is_ok());
    }
}
