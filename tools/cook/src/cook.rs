//! Cooking: from source assets to a container, through packing and padding.

use std::error::Error;
use std::fmt;

use crate::blob::{BLOCK_COLUMNS, BLOCK_ROWS, slots};
use crate::container::{CELL, Container, GroundEntry, PADDING, Region, SpriteEntry};
use crate::image::Image;
use crate::pack::{Placement, pack};
use crate::source::{InvalidSource, Sources, TILE_SIZE};

/// One image to place on an atlas page.
enum Item<'a> {
    /// A blob block, whose tiles are padded one by one.
    Block(&'a Image),
    /// A sprite's image, padded as a whole.
    Sprite(&'a Image),
}

impl Item<'_> {
    /// The rectangle the image takes with its padding. Sizes are rounded up to
    /// even, so that every region starts on an even pixel and stays whole in a
    /// mipmap level of half the size.
    fn packed_size(&self) -> [u32; 2] {
        match self {
            Self::Block(_) => [BLOCK_COLUMNS * CELL, BLOCK_ROWS * CELL],
            Self::Sprite(image) => [
                (image.width() + 2 * PADDING).next_multiple_of(2),
                (image.height() + 2 * PADDING).next_multiple_of(2),
            ],
        }
    }
}

/// Builds the container from `sources`: places every image on atlas pages,
/// pads each tile and sprite with copies of its edge pixels, and records each
/// logical ID's regions.
///
/// The work is small (a few hundred images), so it runs sequentially.
///
/// # Errors
///
/// Returns an error naming the asset if a source asset is invalid or an image
/// is larger than an atlas page.
pub fn cook(sources: &Sources) -> Result<Container, CookError> {
    sources.check().map_err(CookError::Source)?;
    let mut items = Vec::new();
    let mut owners = Vec::new();
    for ground in &sources.grounds {
        for image in ground.variants.iter().flatten() {
            items.push(Item::Block(image));
            owners.push(format!("ground `{}`", ground.id));
        }
    }
    for sprite in &sources.sprites {
        for image in sprite.variants.iter().flatten() {
            items.push(Item::Sprite(image));
            owners.push(format!("sprite `{}`", sprite.id));
        }
    }
    let sizes: Vec<[u32; 2]> = items.iter().map(Item::packed_size).collect();
    let packing = pack(&sizes).map_err(|err| CookError::TooLarge(owners[err.index].clone()))?;

    let mut atlases: Vec<Image> = packing
        .atlas_sizes
        .iter()
        .map(|&[width, height]| Image::new(width, height))
        .collect();
    let regions: Vec<Region> = items
        .iter()
        .zip(&packing.placements)
        .map(|(item, &placement)| draw(&mut atlases, item, placement))
        .collect();

    let mut regions = regions.into_iter();
    let mut take = |variants: &[Vec<Image>]| -> Vec<Vec<Region>> {
        variants
            .iter()
            .map(|frames| frames.iter().map(|_| next(&mut regions)).collect())
            .collect()
    };
    let grounds = sources
        .grounds
        .iter()
        .map(|ground| GroundEntry {
            id: ground.id.clone(),
            variants: take(&ground.variants),
        })
        .collect();
    let sprites = sources
        .sprites
        .iter()
        .map(|sprite| {
            let first = &sprite.variants[0][0];
            SpriteEntry {
                id: sprite.id.clone(),
                size: [first.width(), first.height()],
                anchor: sprite.anchor,
                variants: take(&sprite.variants),
            }
        })
        .collect();
    Ok(Container::new(atlases, grounds, sprites))
}

fn next(regions: &mut impl Iterator<Item = Region>) -> Region {
    regions
        .next()
        .expect("one region was drawn for every image, in the same order")
}

/// Draws `item` where it was placed, and returns the region of the image
/// itself, inside its padding.
fn draw(atlases: &mut [Image], item: &Item<'_>, placement: Placement) -> Region {
    let atlas = &mut atlases[usize::from(placement.atlas)];
    match item {
        Item::Block(block) => {
            for slot in slots() {
                copy_padded(
                    atlas,
                    block,
                    [slot.column * TILE_SIZE, slot.row * TILE_SIZE],
                    [TILE_SIZE, TILE_SIZE],
                    [
                        placement.x + slot.column * CELL,
                        placement.y + slot.row * CELL,
                    ],
                );
            }
        }
        Item::Sprite(image) => copy_padded(
            atlas,
            image,
            [0, 0],
            [image.width(), image.height()],
            [placement.x, placement.y],
        ),
    }
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
pub(crate) mod tests {
    use expanse_core::asset::{GroundId, SpriteId};
    use expanse_core::rng::Rng;

    use super::*;
    use crate::source::{BLOCK_SIZE, SEASONS, SourceGround, SourceSprite};

    /// An image of random pixels, so that a misplaced copy shows.
    pub(crate) fn noise(rng: &mut Rng, width: u32, height: u32) -> Image {
        let pixels = (0..width * height)
            .map(|_| rng.next_u32().to_le_bytes())
            .collect();
        Image::from_pixels(width, height, pixels).expect("one pixel per position")
    }

    /// Grounds with seasons and frames, and sprites of many sizes: enough to
    /// fill more than one atlas page.
    pub(crate) fn sources(seed: u64) -> Sources {
        let mut rng = Rng::from_stream(seed, &[]);
        let grounds = (0..3)
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
        Sources { grounds, sprites }
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
    /// of the space an image takes, and copies to the wrong place, all of
    /// which make the game sample the wrong art.
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
        let page = &container.atlases()[usize::from(region.atlas)];
        for y in 0..size[1] {
            for x in 0..size[0] {
                assert_eq!(
                    page.get(region.x + x, region.y + y),
                    image.get(from[0] + x, from[1] + y)
                );
            }
        }
    }
}
