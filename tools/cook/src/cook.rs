//! Cooking: from source assets to a container, through packing, padding and
//! mipmap generation.

use std::error::Error;
use std::fmt;

use expanse_assets::blob::{BLOCK_COLUMNS, BLOCK_ROWS, slots};
use expanse_assets::{
    Atlas, CELL, Container, GroundEntry, GroundIndex, GroundPairEntry, Image, MAX_ATLAS_SIZE,
    PADDING, Region, SpriteEntry, TILE_SIZE,
};

use crate::mipmap::downsample;
use crate::pack::{Placement, pack};
use crate::source::{InvalidSource, Sources};

/// Width of one padded blob block on an atlas page, in pixels.
const PACKED_BLOCK_WIDTH: u32 = BLOCK_COLUMNS * CELL;

/// Height of one padded blob block on an atlas page, in pixels.
const PACKED_BLOCK_HEIGHT: u32 = BLOCK_ROWS * CELL;

/// How many padded blob blocks fit side by side on one atlas page.
const BLOCKS_PER_ROW: u32 = MAX_ATLAS_SIZE / PACKED_BLOCK_WIDTH;

/// One item to place on an atlas page.
enum Item<'a> {
    /// All seasonal variants and frames of one ground or ground pair, packed
    /// together so every variant lies on the same atlas page.
    BlockGroup(&'a [Vec<Image>]),
    /// A sprite's image, padded as a whole.
    Sprite(&'a Image),
}

impl Item<'_> {
    /// The rectangle the item takes with its padding. Sizes are rounded up to
    /// even, so that every region starts on an even pixel and stays whole in a
    /// mipmap level of half the size.
    fn packed_size(&self) -> [u32; 2] {
        match self {
            Self::BlockGroup(variants) => {
                let count = u32::try_from(variants.len() * variants[0].len())
                    .expect("block count fits in u32");
                let columns = count.min(BLOCKS_PER_ROW);
                let rows = count.div_ceil(BLOCKS_PER_ROW);
                [columns * PACKED_BLOCK_WIDTH, rows * PACKED_BLOCK_HEIGHT]
            }
            Self::Sprite(image) => [
                (image.width() + 2 * PADDING).next_multiple_of(2),
                (image.height() + 2 * PADDING).next_multiple_of(2),
            ],
        }
    }
}

/// Builds the container from `sources`: places every image on atlas pages,
/// pads each tile and sprite with copies of its edge pixels, builds each
/// page's half-size mipmap, and records each logical ID's regions.
///
/// All variants and frames of a ground or ground pair are placed on the same
/// atlas page, so rendering can switch seasons without changing pages.
///
/// The work is small (a few hundred images), so it runs sequentially.
///
/// # Errors
///
/// Returns an error naming the asset if a source asset is invalid or an image
/// (or a ground's variants) is larger than an atlas page.
///
/// # Panics
///
/// Never: every pair ground and index bound is checked before lookup.
pub fn cook(sources: &Sources) -> Result<Container, CookError> {
    sources.check().map_err(CookError::Source)?;
    let mut items = Vec::new();
    let mut owners = Vec::new();
    for ground in &sources.grounds {
        items.push(Item::BlockGroup(&ground.variants));
        owners.push(format!("ground `{}`", ground.id));
    }
    for pair in &sources.pairs {
        items.push(Item::BlockGroup(&pair.variants));
        owners.push(format!(
            "ground pair `{}` over `{}`",
            pair.higher, pair.lower
        ));
    }
    for sprite in &sources.sprites {
        for image in sprite.variants.iter().flatten() {
            items.push(Item::Sprite(image));
            owners.push(format!("sprite `{}`", sprite.id));
        }
    }
    let sizes: Vec<[u32; 2]> = items.iter().map(Item::packed_size).collect();
    let packing = pack(&sizes).map_err(|err| CookError::TooLarge(owners[err.index].clone()))?;

    let mut pages: Vec<Image> = packing
        .atlas_sizes
        .iter()
        .map(|&[width, height]| Image::new(width, height))
        .collect();

    let mut placements = packing.placements.into_iter();
    let grounds: Vec<GroundEntry> = sources
        .grounds
        .iter()
        .map(|ground| {
            let placement = next(&mut placements);
            GroundEntry::new(
                ground.id.clone(),
                draw_block_group(&mut pages, &ground.variants, placement),
            )
        })
        .collect();
    let ground_index = |id: &expanse_core::asset::GroundId| {
        let pos = grounds
            .iter()
            .position(|ground| ground.id() == id)
            .expect("Sources::check verified every pair ground exists");
        GroundIndex(u16::try_from(pos).expect("fewer than 2^16 grounds"))
    };
    let ground_pairs: Vec<GroundPairEntry> = sources
        .pairs
        .iter()
        .map(|pair| {
            let placement = next(&mut placements);
            GroundPairEntry::new(
                ground_index(&pair.higher),
                ground_index(&pair.lower),
                draw_block_group(&mut pages, &pair.variants, placement),
            )
        })
        .collect();
    let sprites: Vec<SpriteEntry> = sources
        .sprites
        .iter()
        .map(|sprite| {
            let first = &sprite.variants[0][0];
            let variants = sprite
                .variants
                .iter()
                .map(|frames| {
                    frames
                        .iter()
                        .map(|image| draw_sprite(&mut pages, image, next(&mut placements)))
                        .collect()
                })
                .collect();
            SpriteEntry::new(
                sprite.id.clone(),
                [first.width(), first.height()],
                sprite.anchor,
                variants,
            )
        })
        .collect();
    let atlases = pages
        .into_iter()
        .map(|page| {
            let mipmap = downsample(&page);
            Atlas::new(page, mipmap)
        })
        .collect();
    Ok(Container::new(atlases, grounds, ground_pairs, sprites))
}

fn next(placements: &mut impl Iterator<Item = Placement>) -> Placement {
    placements
        .next()
        .expect("one placement was produced for every item, in the same order")
}

/// Draws every variant and frame of a ground or pair inside its composite
/// placement on one atlas page.
fn draw_block_group(
    atlases: &mut [Image],
    variants: &[Vec<Image>],
    placement: Placement,
) -> Vec<Vec<Region>> {
    let atlas = &mut atlases[usize::from(placement.atlas)];
    let mut block_index = 0_u32;
    variants
        .iter()
        .map(|frames| {
            frames
                .iter()
                .map(|block| {
                    let origin_x =
                        placement.x + (block_index % BLOCKS_PER_ROW) * PACKED_BLOCK_WIDTH;
                    let origin_y =
                        placement.y + (block_index / BLOCKS_PER_ROW) * PACKED_BLOCK_HEIGHT;
                    block_index += 1;
                    for slot in slots() {
                        copy_padded(
                            atlas,
                            block,
                            [slot.column * TILE_SIZE, slot.row * TILE_SIZE],
                            [TILE_SIZE, TILE_SIZE],
                            [origin_x + slot.column * CELL, origin_y + slot.row * CELL],
                        );
                    }
                    Region {
                        atlas: placement.atlas,
                        x: origin_x + PADDING,
                        y: origin_y + PADDING,
                    }
                })
                .collect()
        })
        .collect()
}

/// Draws a sprite image at `placement` and returns its region inside padding.
fn draw_sprite(atlases: &mut [Image], image: &Image, placement: Placement) -> Region {
    let atlas = &mut atlases[usize::from(placement.atlas)];
    copy_padded(
        atlas,
        image,
        [0, 0],
        [image.width(), image.height()],
        [placement.x, placement.y],
    );
    Region {
        atlas: placement.atlas,
        x: placement.x + PADDING,
        y: placement.y + PADDING,
    }
}

/// Copies the `size` pixels at `from` in `source` to `to + PADDING` in
/// `atlas`, and fills the padding around them with copies of their edge
/// pixels.
fn copy_padded(atlas: &mut Image, source: &Image, from: [u32; 2], size: [u32; 2], to: [u32; 2]) {
    for y in 0..size[1] + 2 * PADDING {
        for x in 0..size[0] + 2 * PADDING {
            let source_x = x.saturating_sub(PADDING).min(size[0] - 1);
            let source_y = y.saturating_sub(PADDING).min(size[1] - 1);
            atlas.set(
                to[0] + x,
                to[1] + y,
                source.get(from[0] + source_x, from[1] + source_y),
            );
        }
    }
}

/// Why source assets could not be cooked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookError {
    /// A source asset is invalid.
    Source(InvalidSource),
    /// An image of this asset, such as "sprite `house`", is larger than an
    /// atlas page.
    TooLarge(String),
}

impl fmt::Display for CookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(err) => err.fmt(f),
            Self::TooLarge(asset) => write!(f, "{asset} is larger than an atlas page"),
        }
    }
}

impl Error for CookError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(err) => Some(err),
            Self::TooLarge(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use expanse_assets::{BLOCK_SIZE, SEASONS};
    use expanse_core::asset::{GroundId, SpriteId};
    use expanse_core::rng::Rng;

    use super::*;
    use crate::source::{SourceGround, SourceGroundPair, SourceSprite};

    /// An image of random pixels, so that a misplaced copy shows.
    pub(crate) fn noise(rng: &mut Rng, width: u32, height: u32) -> Image {
        let pixels = (0..width * height)
            .map(|_| rng.next_u32().to_le_bytes())
            .collect();
        Image::from_pixels(width, height, pixels).expect("one pixel per position")
    }

    /// Grounds with seasons and frames, a ground pair, and sprites of many
    /// sizes: enough to fill more than one atlas page.
    pub(crate) fn sources(seed: u64) -> Sources {
        let mut rng = Rng::from_stream(seed, &[]);
        let grounds: Vec<SourceGround> = (0..3)
            .map(|index| SourceGround {
                id: GroundId::new(format!("ground_{index}")).expect("a valid name"),
                variants: (0..SEASONS)
                    .map(|_| {
                        (0..2)
                            .map(|_| noise(&mut rng, BLOCK_SIZE[0], BLOCK_SIZE[1]))
                            .collect()
                    })
                    .collect(),
            })
            .collect();
        let pairs = vec![SourceGroundPair {
            higher: grounds[1].id.clone(),
            lower: grounds[0].id.clone(),
            variants: (0..SEASONS)
                .map(|_| vec![noise(&mut rng, BLOCK_SIZE[0], BLOCK_SIZE[1])])
                .collect(),
        }];
        let sprites = (0..600)
            .map(|index| {
                let width = 1 + rng.below(160);
                let height = 1 + rng.below(160);
                SourceSprite {
                    id: SpriteId::new(format!("thing_{index}")).expect("a valid name"),
                    anchor: [rng.below(width + 1), rng.below(height + 1)],
                    variants: vec![vec![noise(&mut rng, width, height)]],
                }
            })
            .collect();
        Sources {
            grounds,
            pairs,
            sprites,
        }
    }

    /// The rectangle a region takes with its padding.
    fn padded(region: Region, size: [u32; 2]) -> (u16, [u32; 4]) {
        (
            region.atlas,
            [
                region.x - PADDING,
                region.y - PADDING,
                region.x + size[0] + PADDING,
                region.y + size[1] + PADDING,
            ],
        )
    }

    /// Catches images that overlap or spill off their page, padding left out
    /// of the space an image takes, variants of one ground split across pages,
    /// and copies to the wrong place, all of which make the game sample the
    /// wrong art.
    #[test]
    fn every_image_lies_whole_and_alone_on_its_page() {
        let sources = sources(7);
        let container = cook(&sources).expect("valid sources");
        assert!(
            container.atlases().len() > 1,
            "the test needs several pages"
        );

        let mut rects = Vec::new();
        for (entry, source) in container.grounds().iter().zip(&sources.grounds) {
            for (blocks, images) in entry.variants().iter().zip(&source.variants) {
                for (&block, image) in blocks.iter().zip(images) {
                    assert_eq!(block.atlas, entry.atlas(), "variants stay on one page");
                    for slot in slots() {
                        let tile = GroundEntry::tile(block, slot);
                        rects.push(padded(tile, [TILE_SIZE, TILE_SIZE]));
                        let from = [slot.column * TILE_SIZE, slot.row * TILE_SIZE];
                        assert_same_pixels(&container, tile, image, from, [TILE_SIZE; 2]);
                    }
                }
            }
        }
        for (entry, source) in container.ground_pairs().iter().zip(&sources.pairs) {
            for (blocks, images) in entry.variants().iter().zip(&source.variants) {
                for (&block, image) in blocks.iter().zip(images) {
                    assert_eq!(block.atlas, entry.atlas(), "pair variants stay on one page");
                    for slot in slots() {
                        let tile = GroundEntry::tile(block, slot);
                        rects.push(padded(tile, [TILE_SIZE, TILE_SIZE]));
                        let from = [slot.column * TILE_SIZE, slot.row * TILE_SIZE];
                        assert_same_pixels(&container, tile, image, from, [TILE_SIZE; 2]);
                    }
                }
            }
        }
        for (entry, source) in container.sprites().iter().zip(&sources.sprites) {
            let region = entry.variants()[0][0];
            rects.push(padded(region, entry.size()));
            assert_same_pixels(
                &container,
                region,
                &source.variants[0][0],
                [0, 0],
                entry.size(),
            );
        }

        for (index, &(atlas, [left, top, right, bottom])) in rects.iter().enumerate() {
            let page = &container.atlases()[usize::from(atlas)];
            assert!(
                right <= page.width() && bottom <= page.height(),
                "off its page"
            );
            assert_eq!(page.mipmap().width(), page.width() / 2);
            assert_eq!(page.mipmap().height(), page.height() / 2);
            for &(other_atlas, [other_left, other_top, other_right, other_bottom]) in
                &rects[index + 1..]
            {
                let overlap = atlas == other_atlas
                    && left < other_right
                    && other_left < right
                    && top < other_bottom
                    && other_top < bottom;
                assert!(!overlap, "two padded images overlap");
            }
        }
    }

    fn assert_same_pixels(
        container: &Container,
        region: Region,
        image: &Image,
        from: [u32; 2],
        size: [u32; 2],
    ) {
        let page = container.atlases()[usize::from(region.atlas)].image();
        for y in 0..size[1] {
            for x in 0..size[0] {
                assert_eq!(
                    page.get(region.x + x, region.y + y),
                    image.get(from[0] + x, from[1] + y)
                );
            }
        }
    }

    /// Catches invalid pair definitions (pairing a ground with itself, naming
    /// an unknown ground, or repeating a pair) slipping into a container.
    #[test]
    fn invalid_ground_pairs_are_rejected() {
        let block = || vec![vec![Image::new(BLOCK_SIZE[0], BLOCK_SIZE[1])]];
        let grass = GroundId::new("grass").expect("valid");
        let water = GroundId::new("water").expect("valid");
        let sand = GroundId::new("sand").expect("valid");
        let grounds = vec![
            SourceGround {
                id: grass.clone(),
                variants: block(),
            },
            SourceGround {
                id: water.clone(),
                variants: block(),
            },
        ];

        let self_pair = Sources {
            grounds: grounds.clone(),
            pairs: vec![SourceGroundPair {
                higher: grass.clone(),
                lower: grass.clone(),
                variants: block(),
            }],
            sprites: Vec::new(),
        };
        assert!(matches!(
            cook(&self_pair),
            Err(CookError::Source(InvalidSource::PairWithItself(_)))
        ));

        let unknown = Sources {
            grounds: grounds.clone(),
            pairs: vec![SourceGroundPair {
                higher: grass.clone(),
                lower: sand,
                variants: block(),
            }],
            sprites: Vec::new(),
        };
        assert!(matches!(
            cook(&unknown),
            Err(CookError::Source(InvalidSource::UnknownPairGround { .. }))
        ));

        let duplicate = Sources {
            grounds,
            pairs: vec![
                SourceGroundPair {
                    higher: grass.clone(),
                    lower: water.clone(),
                    variants: block(),
                },
                SourceGroundPair {
                    higher: grass,
                    lower: water,
                    variants: block(),
                },
            ],
            sprites: Vec::new(),
        };
        assert!(matches!(
            cook(&duplicate),
            Err(CookError::Source(InvalidSource::Repeated(_)))
        ));
    }
}
