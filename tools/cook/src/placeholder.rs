//! Placeholder art, drawn from recipes: the source of art that every logical
//! ID has, so the game runs and looks readable without the asset pack.
//!
//! Placeholders follow the pack's format (blob blocks in its layout, seasonal
//! variants, frames, sprite proportions), never its content.

use std::error::Error;
use std::fmt;

use expanse_core::asset::{GroundId, SpriteId};

use crate::blob::{Neighbour, NeighbourMask, mask_of_slot, slots};
use crate::image::{Image, Rgba};
use crate::source::{BLOCK_SIZE, SEASONS, SourceGround, SourceSprite, Sources, TILE_SIZE};

/// A colour in sRGB.
pub type Rgb = [u8; 3];

/// The colours of an asset: one, or one per season in calendar order from
/// spring. Each colour gives the asset a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colours {
    Constant(Rgb),
    Seasonal([Rgb; SEASONS]),
}

impl Colours {
    fn variants(&self) -> Vec<Rgb> {
        match self {
            Self::Constant(colour) => vec![*colour],
            Self::Seasonal(colours) => colours.to_vec(),
        }
    }
}

/// The marks drawn over a ground's colour. Every pattern repeats every 8
/// pixels, so tiles join seamlessly, and its marks are at least 2 pixels
/// wide, so they survive a mipmap level of half the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Solid,
    Dots,
    Hatching,
    CrossHatching,
    Waves,
    Checks,
}

/// The period of every pattern, in pixels.
const PATTERN_PERIOD: u32 = 8;

impl Pattern {
    /// Whether the pattern marks the pixel at `(x, y)`.
    fn marks(self, x: u32, y: u32) -> bool {
        let (x, y) = (x % PATTERN_PERIOD, y % PATTERN_PERIOD);
        let diagonal = (x + y) % PATTERN_PERIOD < 2;
        match self {
            Self::Solid => false,
            Self::Dots => {
                let in_dot = |x: u32, y: u32| (2..4).contains(&x) && (2..4).contains(&y);
                in_dot(x, y) || in_dot((x + 4) % 8, (y + 4) % 8)
            }
            Self::Hatching => diagonal,
            Self::CrossHatching => diagonal || (x + PATTERN_PERIOD - y) % PATTERN_PERIOD < 2,
            Self::Waves => {
                const CREST: [u32; 8] = [0, 1, 1, 2, 2, 1, 1, 0];
                (y + CREST[x as usize]) % PATTERN_PERIOD < 2
            }
            Self::Checks => (x / 4 + y / 4) % 2 == 0,
        }
    }
}

/// How to draw a ground's placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundRecipe {
    pub id: GroundId,
    pub colours: Colours,
    pub pattern: Pattern,
    /// The frames of its animation: 1, 2, 4 or 8. Each frame shifts the
    /// pattern east by an equal step, so the last leads back to the first.
    pub frames: u32,
}

/// The shapes a sprite's placeholder can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A round crown over a trunk that stands on the anchor tile.
    Canopy,
    /// A pointed crown over a trunk that stands on the anchor tile.
    Cone,
    /// A rounded mass filling the sprite.
    Boulder,
    /// A rounded bar filling the sprite, lying along its longer side.
    Capsule,
}

/// How to draw a sprite's placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteRecipe {
    pub id: SpriteId,
    pub colours: Colours,
    pub shape: Shape,
    /// The size of the image, in tiles.
    pub tiles: [u32; 2],
    /// The anchor tile, in tiles from the image's top-left corner.
    pub anchor_tile: [u32; 2],
}

/// Every placeholder to draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recipes {
    pub grounds: Vec<GroundRecipe>,
    pub sprites: Vec<SpriteRecipe>,
}

/// Draws the source assets of every recipe.
///
/// # Errors
///
/// Returns an error naming the asset if a recipe has a frame count other than
/// 1, 2, 4 or 8, an empty size, or an anchor tile outside its size.
pub fn draw_placeholders(recipes: &Recipes) -> Result<Sources, InvalidRecipe> {
    let grounds = recipes
        .grounds
        .iter()
        .map(draw_ground)
        .collect::<Result<_, _>>()?;
    let sprites = recipes
        .sprites
        .iter()
        .map(draw_sprite)
        .collect::<Result<_, _>>()?;
    Ok(Sources { grounds, sprites })
}

fn draw_ground(recipe: &GroundRecipe) -> Result<SourceGround, InvalidRecipe> {
    if !matches!(recipe.frames, 1 | 2 | 4 | 8) {
        return Err(InvalidRecipe::FrameCount(format!("ground `{}`", recipe.id)));
    }
    let variants = recipe
        .colours
        .variants()
        .into_iter()
        .map(|colour| {
            (0..recipe.frames)
                .map(|frame| {
                    let shift = frame * PATTERN_PERIOD / recipe.frames;
                    draw_block(colour, recipe.pattern, shift)
                })
                .collect()
        })
        .collect();
    Ok(SourceGround {
        id: recipe.id.clone(),
        variants,
    })
}

/// How deep the edge of a ground is set into its tile.
const INSET: i32 = 2;

fn draw_block(colour: Rgb, pattern: Pattern, shift: u32) -> Image {
    let mut block = Image::new(BLOCK_SIZE[0], BLOCK_SIZE[1]);
    let tile = TILE_SIZE.cast_signed();
    for slot in slots() {
        let Some(mask) = mask_of_slot(slot) else {
            continue;
        };
        let inside = |x: i32, y: i32| covers(mask, x, y);
        for y in 0..tile {
            for x in 0..tile {
                if !inside(x, y) {
                    continue;
                }
                let edge =
                    !(inside(x - 1, y) && inside(x + 1, y) && inside(x, y - 1) && inside(x, y + 1));
                let (x, y) = (x.cast_unsigned(), y.cast_unsigned());
                let pixel = if edge {
                    outline(colour)
                } else if pattern.marks(x + PATTERN_PERIOD - shift, y) {
                    mark(colour)
                } else {
                    opaque(colour)
                };
                block.set(slot.column * TILE_SIZE + x, slot.row * TILE_SIZE + y, pixel);
            }
        }
    }
    block
}

/// Whether a tile of shape `mask` covers its pixel `(x, y)`. Outside the
/// tile, the pixel belongs to the neighbour in that direction, covered if the
/// neighbour is present.
fn covers(mask: NeighbourMask, x: i32, y: i32) -> bool {
    let tile = TILE_SIZE.cast_signed();
    let column = if x < 0 { -1 } else { i32::from(x >= tile) };
    let row = if y < 0 { -1 } else { i32::from(y >= tile) };
    if (column, row) != (0, 0) {
        return Neighbour::ALL
            .into_iter()
            .find(|neighbour| neighbour.offset() == (column, row))
            .is_some_and(|neighbour| mask.has(neighbour));
    }
    let far = tile - 1;
    let north = mask.has(Neighbour::North);
    let south = mask.has(Neighbour::South);
    let west = mask.has(Neighbour::West);
    let east = mask.has(Neighbour::East);
    let open_edge = (!north && y < INSET)
        || (!south && y > far - INSET)
        || (!west && x < INSET)
        || (!east && x > far - INSET);
    // Where two open edges meet, the corner is cut off diagonally.
    let chamfer = INSET * 2 + 2;
    let outer_corner = (!north && !west && x + y < chamfer)
        || (!north && !east && (far - x) + y < chamfer)
        || (!south && !west && x + (far - y) < chamfer)
        || (!south && !east && (far - x) + (far - y) < chamfer);
    // Where two present edges meet without their corner, a notch is cut.
    let notch = |corner, a: bool, b: bool, near_x: bool, near_y: bool| {
        a && b && !mask.has(corner) && near_x && near_y
    };
    let inner_corner = notch(Neighbour::NorthWest, north, west, x < INSET, y < INSET)
        || notch(
            Neighbour::NorthEast,
            north,
            east,
            x > far - INSET,
            y < INSET,
        )
        || notch(
            Neighbour::SouthWest,
            south,
            west,
            x < INSET,
            y > far - INSET,
        )
        || notch(
            Neighbour::SouthEast,
            south,
            east,
            x > far - INSET,
            y > far - INSET,
        );
    !(open_edge || outer_corner || inner_corner)
}

fn draw_sprite(recipe: &SpriteRecipe) -> Result<SourceSprite, InvalidRecipe> {
    let asset = || format!("sprite `{}`", recipe.id);
    let [columns, rows] = recipe.tiles;
    let [anchor_column, anchor_row] = recipe.anchor_tile;
    if columns == 0 || rows == 0 {
        return Err(InvalidRecipe::EmptySize(asset()));
    }
    if anchor_column >= columns || anchor_row >= rows {
        return Err(InvalidRecipe::AnchorOutside(asset()));
    }
    let size = [columns * TILE_SIZE, rows * TILE_SIZE];
    let anchor = [anchor_column * TILE_SIZE, anchor_row * TILE_SIZE];
    let variants = recipe
        .colours
        .variants()
        .into_iter()
        .map(|colour| vec![draw_shape(recipe.shape, colour, size, anchor)])
        .collect();
    Ok(SourceSprite {
        id: recipe.id.clone(),
        anchor,
        variants,
    })
}

/// The colour of a trunk, whatever the crown's.
const TRUNK: Rgb = [104, 72, 44];

/// The ground shadow's darkness.
const SHADOW_ALPHA: u8 = 80;

/// A part of a sprite's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Main,
    Trunk,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "sprite sizes are a few hundred pixels at most"
)]
fn draw_shape(shape: Shape, colour: Rgb, size: [u32; 2], anchor: [u32; 2]) -> Image {
    let (width, height) = (size[0] as f32, size[1] as f32);
    let tile = TILE_SIZE as f32;
    let (anchor_x, anchor_y) = (anchor[0] as f32, anchor[1] as f32);
    let part_at = |x: f32, y: f32| -> Option<Part> {
        match shape {
            Shape::Canopy | Shape::Cone => {
                let trunk_x = anchor_x + tile / 2.0;
                let crown_bottom = anchor_y + tile * 0.4;
                let crown = if shape == Shape::Canopy {
                    let centre = (width / 2.0, crown_bottom / 2.0);
                    within_ellipse(x, y, centre, (width / 2.0 - 1.0, crown_bottom / 2.0 - 1.0))
                } else {
                    let depth = (y - 1.0) / (crown_bottom - 1.0);
                    (0.0..=1.0).contains(&depth)
                        && (x - width / 2.0).abs() <= depth * (width / 2.0 - 1.0)
                };
                let trunk = (x - trunk_x).abs() < tile * 0.15
                    && y >= crown_bottom - tile * 0.5
                    && y < anchor_y + tile * 0.85;
                if crown {
                    Some(Part::Main)
                } else {
                    trunk.then_some(Part::Trunk)
                }
            }
            Shape::Boulder => within_ellipse(
                x,
                y,
                (width / 2.0, height / 2.0 + 1.0),
                (width / 2.0 - 1.0, height / 2.0 - 2.0),
            )
            .then_some(Part::Main),
            Shape::Capsule => {
                let along_x = width >= height;
                let (long, short, a, b) = if along_x {
                    (width, height, x, y)
                } else {
                    (height, width, y, x)
                };
                let radius = short / 2.0 - 2.0;
                let centre = short / 2.0;
                let along = a.clamp(radius + 1.0, long - radius - 1.0);
                ((a - along).powi(2) + (b - centre).powi(2)).sqrt() <= radius
            }
            .then_some(Part::Main),
        }
    };

    let mut image = Image::new(size[0], size[1]);
    // The shadow lies on the ground under the foot of the shape.
    let foot = match shape {
        Shape::Canopy | Shape::Cone => (anchor_x + tile / 2.0, anchor_y + tile * 0.85, tile * 0.45),
        Shape::Boulder | Shape::Capsule => (width / 2.0, height - 2.5, width / 2.0 - 1.0),
    };
    for y in 0..size[1] {
        for x in 0..size[0] {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            if within_ellipse(px, py, (foot.0, foot.1), (foot.2, 2.5)) {
                image.set(x, y, [0, 0, 0, SHADOW_ALPHA]);
            }
            let Some(part) = part_at(px, py) else {
                continue;
            };
            let edge = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
                .iter()
                .any(|&(dx, dy)| part_at(px + dx, py + dy) != Some(part));
            let base = match part {
                Part::Main => colour,
                Part::Trunk => TRUNK,
            };
            image.set(x, y, if edge { outline(base) } else { opaque(base) });
        }
    }
    image
}

fn within_ellipse(x: f32, y: f32, centre: (f32, f32), radii: (f32, f32)) -> bool {
    let dx = (x - centre.0) / radii.0;
    let dy = (y - centre.1) / radii.1;
    dx * dx + dy * dy <= 1.0
}

const fn opaque(colour: Rgb) -> Rgba {
    [colour[0], colour[1], colour[2], 255]
}

/// The outline of a shape in `colour`: much darker.
fn outline(colour: Rgb) -> Rgba {
    opaque(colour.map(|c| scale(c, 0.55)))
}

/// A pattern's mark on `colour`: darker on light colours, lighter on dark
/// ones, so that it shows on both.
fn mark(colour: Rgb) -> Rgba {
    let luma =
        0.3 * f32::from(colour[0]) + 0.59 * f32::from(colour[1]) + 0.11 * f32::from(colour[2]);
    if luma < 80.0 {
        opaque(colour.map(|c| scale(c, 1.45).max(c.saturating_add(18))))
    } else {
        opaque(colour.map(|c| scale(c, 0.8)))
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=255 first"
)]
fn scale(channel: u8, factor: f32) -> u8 {
    (f32::from(channel) * factor).round().clamp(0.0, 255.0) as u8
}

/// Why a recipe cannot be drawn. Each names the asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidRecipe {
    /// A ground's frame count is not 1, 2, 4 or 8.
    FrameCount(String),
    /// A sprite has no tiles.
    EmptySize(String),
    /// A sprite's anchor tile lies outside its size.
    AnchorOutside(String),
}

impl fmt::Display for InvalidRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameCount(asset) => write!(f, "{asset} needs 1, 2, 4 or 8 frames"),
            Self::EmptySize(asset) => write!(f, "{asset} has no tiles"),
            Self::AnchorOutside(asset) => write!(f, "{asset} has its anchor tile outside its size"),
        }
    }
}

impl Error for InvalidRecipe {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::Slot;

    /// The mask that the pack's convention reads from a tile's transparency
    /// (see `dev/findings/asset-pack.md#tiles`): a side is present when the
    /// two middle pixels of that edge are opaque, a corner when its pixel is.
    fn read_mask(block: &Image, slot: Slot) -> NeighbourMask {
        let (left, top) = (slot.column * TILE_SIZE, slot.row * TILE_SIZE);
        let (far, middle) = (TILE_SIZE - 1, TILE_SIZE / 2);
        let opaque = |x: u32, y: u32| block.get(left + x, top + y)[3] == u8::MAX;
        let edge_opaque = |a: (u32, u32), b: (u32, u32)| opaque(a.0, a.1) && opaque(b.0, b.1);
        NeighbourMask::from_fn(|neighbour| match neighbour {
            Neighbour::North => edge_opaque((middle - 1, 0), (middle, 0)),
            Neighbour::East => edge_opaque((far, middle - 1), (far, middle)),
            Neighbour::South => edge_opaque((middle - 1, far), (middle, far)),
            Neighbour::West => edge_opaque((0, middle - 1), (0, middle)),
            Neighbour::NorthEast => opaque(far, 0),
            Neighbour::SouthEast => opaque(far, far),
            Neighbour::SouthWest => opaque(0, far),
            Neighbour::NorthWest => opaque(0, 0),
        })
        .canonical()
    }

    /// Catches a placeholder block that the pack's blocks could not replace:
    /// an open edge drawn on the wrong side, an inner corner without its
    /// notch, an inset so deep that a present side's middle is cut, or a shape
    /// drawn in the empty slot.
    #[test]
    fn every_slot_reads_as_its_shape_under_the_packs_transparency_convention() {
        let block = draw_block([90, 140, 60], Pattern::Solid, 0);
        for slot in slots() {
            if let Some(mask) = mask_of_slot(slot) {
                assert_eq!(read_mask(&block, slot), mask, "slot {slot:?}");
                continue;
            }
            let (left, top) = (slot.column * TILE_SIZE, slot.row * TILE_SIZE);
            for y in top..top + TILE_SIZE {
                for x in left..left + TILE_SIZE {
                    assert_eq!(block.get(x, y)[3], 0, "empty slot pixel ({x}, {y})");
                }
            }
        }
    }
}
