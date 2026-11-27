//! The container: what the game loads. Atlases, and for each logical ID the
//! regions that hold its art.

use expanse_core::asset::{GroundId, SpriteId};

use crate::blob::Slot;
use crate::image::Image;
use crate::source::TILE_SIZE;

/// The pixels around each tile and sprite that repeat its edge, so that
/// sampling near an edge never reads a neighbouring region. Two pixels keep
/// regions apart in a mipmap level of half the size too.
pub const PADDING: u32 = 2;

/// The distance between two tiles of a blob block in an atlas: a tile and its
/// padding on both sides.
pub const CELL: u32 = TILE_SIZE + 2 * PADDING;

/// The position of a ground in a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroundIndex(pub u16);

/// The position of a sprite in a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpriteIndex(pub u16);

/// Where one image lies in the atlases: the page, and the top-left pixel of
/// the image itself, inside its padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub atlas: u16,
    pub x: u32,
    pub y: u32,
}

/// One atlas page, with its half-size mipmap level for zooms below 1×.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    image: Image,
    mipmap: Image,
}

impl Atlas {
    /// Creates an atlas page from its full-size `image` and its half-size
    /// `mipmap`.
    ///
    /// # Panics
    ///
    /// Panics if `mipmap` is not half the dimensions of `image`.
    pub(crate) fn new(image: Image, mipmap: Image) -> Self {
        assert!(
            image.width() > 0
                && image.height() > 0
                && image.width().is_multiple_of(2)
                && image.height().is_multiple_of(2)
                && mipmap.width() == image.width() / 2
                && mipmap.height() == image.height() / 2,
            "mipmap {}×{} does not match atlas {}×{}",
            mipmap.width(),
            mipmap.height(),
            image.width(),
            image.height(),
        );
        Self { image, mipmap }
    }

    /// Builds an atlas page from its full-size `image`, computing its
    /// half-size mipmap level.
    pub(crate) fn from_image(image: Image) -> Self {
        let mipmap = image.downsample();
        Self { image, mipmap }
    }

    /// The width of the full-size page, in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.image.width()
    }

    /// The height of the full-size page, in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.image.height()
    }

    /// Level 0: the full-size atlas image, in sRGB with straight alpha.
    #[must_use]
    pub const fn image(&self) -> &Image {
        &self.image
    }

    /// Level 1: the half-size mipmap image, in sRGB with straight alpha.
    #[must_use]
    pub const fn mipmap(&self) -> &Image {
        &self.mipmap
    }
}

/// A ground's blob blocks: per variant, per frame, all on one atlas page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundEntry {
    pub(crate) id: GroundId,
    /// The region of the first tile of each block. The tile in `slot` is
    /// [`CELL`] pixels further for each column and row.
    pub(crate) variants: Vec<Vec<Region>>,
}

impl GroundEntry {
    /// The logical ID.
    #[must_use]
    pub const fn id(&self) -> &GroundId {
        &self.id
    }

    /// The atlas page holding every variant and frame of this ground.
    #[must_use]
    pub fn atlas(&self) -> u16 {
        self.variants[0][0].atlas
    }

    /// The blocks: per variant, one or one per season, the frames.
    #[must_use]
    pub fn variants(&self) -> &[Vec<Region>] {
        &self.variants
    }

    /// The region of the tile in `slot` of `block`.
    #[must_use]
    pub const fn tile(block: Region, slot: Slot) -> Region {
        Region {
            atlas: block.atlas,
            x: block.x + slot.column * CELL,
            y: block.y + slot.row * CELL,
        }
    }
}

/// Blob blocks for a higher ground drawn over a particular lower ground: per
/// variant, per frame, all on one atlas page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundPairEntry {
    pub(crate) higher: GroundIndex,
    pub(crate) lower: GroundIndex,
    pub(crate) variants: Vec<Vec<Region>>,
}

impl GroundPairEntry {
    /// The ground drawn on top.
    #[must_use]
    pub const fn higher(&self) -> GroundIndex {
        self.higher
    }

    /// The ground immediately beneath it on the tile.
    #[must_use]
    pub const fn lower(&self) -> GroundIndex {
        self.lower
    }

    /// The atlas page holding every variant and frame of this pair.
    #[must_use]
    pub fn atlas(&self) -> u16 {
        self.variants[0][0].atlas
    }

    /// The blocks: per variant, one or one per season, the frames.
    #[must_use]
    pub fn variants(&self) -> &[Vec<Region>] {
        &self.variants
    }
}

/// A sprite's images: per variant, per frame, all of one size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteEntry {
    pub(crate) id: SpriteId,
    pub(crate) size: [u32; 2],
    pub(crate) anchor: [u32; 2],
    pub(crate) variants: Vec<Vec<Region>>,
}

impl SpriteEntry {
    /// The logical ID.
    #[must_use]
    pub const fn id(&self) -> &SpriteId {
        &self.id
    }

    /// The width and height of each image, in pixels.
    #[must_use]
    pub const fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Where the top-left corner of the anchor tile lies in each image.
    #[must_use]
    pub const fn anchor(&self) -> [u32; 2] {
        self.anchor
    }

    /// The images: per variant, one or one per season, the frames.
    #[must_use]
    pub fn variants(&self) -> &[Vec<Region>] {
        &self.variants
    }

    /// How far the image reaches beyond its anchor tile.
    #[must_use]
    pub const fn overhang(&self) -> Overhang {
        let [width, height] = self.size;
        let [x, y] = self.anchor;
        Overhang {
            left: x,
            up: y,
            right: width.saturating_sub(x + TILE_SIZE),
            down: height.saturating_sub(y + TILE_SIZE),
        }
    }
}

/// How far a sprite's image reaches beyond its anchor tile on each side, in
/// pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overhang {
    pub left: u32,
    pub right: u32,
    pub up: u32,
    pub down: u32,
}

impl Overhang {
    /// The larger reach on each side.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            right: self.right.max(other.right),
            up: self.up.max(other.up),
            down: self.down.max(other.down),
        }
    }
}

/// Cooked art: the atlas pages, and the regions of each logical ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    atlases: Vec<Atlas>,
    grounds: Vec<GroundEntry>,
    ground_pairs: Vec<GroundPairEntry>,
    sprites: Vec<SpriteEntry>,
    largest_overhang: Overhang,
}

impl Container {
    pub(crate) fn new(
        atlases: Vec<Atlas>,
        grounds: Vec<GroundEntry>,
        ground_pairs: Vec<GroundPairEntry>,
        sprites: Vec<SpriteEntry>,
    ) -> Self {
        let largest_overhang = sprites
            .iter()
            .map(SpriteEntry::overhang)
            .fold(Overhang::default(), Overhang::max);
        Self {
            atlases,
            grounds,
            ground_pairs,
            sprites,
            largest_overhang,
        }
    }

    /// The atlas pages, each with its half-size mipmap, in sRGB with straight
    /// alpha.
    #[must_use]
    pub fn atlases(&self) -> &[Atlas] {
        &self.atlases
    }

    /// Every ground.
    #[must_use]
    pub fn grounds(&self) -> &[GroundEntry] {
        &self.grounds
    }

    /// Every ground-over-ground pair block.
    #[must_use]
    pub fn ground_pairs(&self) -> &[GroundPairEntry] {
        &self.ground_pairs
    }

    /// The pair block for `higher` drawn directly over `lower`, if the
    /// container defines one.
    #[must_use]
    pub fn ground_pair(&self, higher: GroundIndex, lower: GroundIndex) -> Option<&GroundPairEntry> {
        self.ground_pairs
            .iter()
            .find(|pair| pair.higher == higher && pair.lower == lower)
    }

    /// Every sprite.
    #[must_use]
    pub fn sprites(&self) -> &[SpriteEntry] {
        &self.sprites
    }

    /// The ground with `id`, if the container holds it.
    ///
    /// # Panics
    ///
    /// Never: cooking and reading both limit a container to fewer than 2^16
    /// grounds.
    #[must_use]
    pub fn ground_index(&self, id: &GroundId) -> Option<GroundIndex> {
        let position = self.grounds.iter().position(|entry| &entry.id == id)?;
        Some(GroundIndex(
            u16::try_from(position).expect("a container holds fewer than 2^16 grounds"),
        ))
    }

    /// The sprite with `id`, if the container holds it.
    ///
    /// # Panics
    ///
    /// Never: cooking and reading both limit a container to fewer than 2^16
    /// sprites.
    #[must_use]
    pub fn sprite_index(&self, id: &SpriteId) -> Option<SpriteIndex> {
        let position = self.sprites.iter().position(|entry| &entry.id == id)?;
        Some(SpriteIndex(
            u16::try_from(position).expect("a container holds fewer than 2^16 sprites"),
        ))
    }

    /// The ground at `index`.
    ///
    /// # Panics
    ///
    /// Panics if the container has no ground at `index`.
    #[must_use]
    pub fn ground(&self, index: GroundIndex) -> &GroundEntry {
        &self.grounds[usize::from(index.0)]
    }

    /// The sprite at `index`.
    ///
    /// # Panics
    ///
    /// Panics if the container has no sprite at `index`.
    #[must_use]
    pub fn sprite(&self, index: SpriteIndex) -> &SpriteEntry {
        &self.sprites[usize::from(index.0)]
    }

    /// The farthest any sprite reaches beyond its anchor tile on each side.
    /// Culling keeps a sprite anchored this far outside the view.
    #[must_use]
    pub const fn largest_overhang(&self) -> Overhang {
        self.largest_overhang
    }
}
