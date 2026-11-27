//! Source assets: the art of each logical ID, in memory, whatever it came
//! from. Every source of art (the placeholder recipes now, the asset pack and
//! our own art later) becomes source assets, and `cook` works from those.

use std::error::Error;
use std::fmt;

use expanse_core::asset::{GroundId, SpriteId};

use crate::blob::{BLOCK_COLUMNS, BLOCK_ROWS};
use crate::image::Image;

/// The side of a tile, in pixels.
pub const TILE_SIZE: u32 = 16;

/// The number of seasonal variants of an asset that has them, in calendar
/// order from spring.
pub const SEASONS: usize = 4;

/// The art of one ground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceGround {
    pub id: GroundId,
    /// One variant, or one per season. Each holds the animation's frames, at
    /// least one, as blob blocks of 12×4 tiles.
    pub variants: Vec<Vec<Image>>,
}

/// The art of one ground drawn directly over a particular lower ground, such
/// as grass over shallow water. A pair is not a logical ID: content never
/// names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceGroundPair {
    pub higher: GroundId,
    pub lower: GroundId,
    /// One variant, or one per season. Each holds the animation's frames, at
    /// least one, as blob blocks of 12×4 tiles.
    pub variants: Vec<Vec<Image>>,
}

/// The art of one sprite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSprite {
    pub id: SpriteId,
    /// Where the top-left corner of the anchor tile lies in the image, in
    /// pixels. The image may reach beyond the anchor tile on every side.
    pub anchor: [u32; 2],
    /// One variant, or one per season. Each holds the animation's frames, at
    /// least one, all of one size.
    pub variants: Vec<Vec<Image>>,
}

/// Every source asset to cook.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sources {
    pub grounds: Vec<SourceGround>,
    pub pairs: Vec<SourceGroundPair>,
    pub sprites: Vec<SourceSprite>,
}

/// The size of a blob block, in pixels.
pub const BLOCK_SIZE: [u32; 2] = [BLOCK_COLUMNS * TILE_SIZE, BLOCK_ROWS * TILE_SIZE];

impl Sources {
    /// Checks what `cook` relies on.
    ///
    /// # Errors
    ///
    /// Returns the first problem found, naming the asset.
    pub fn check(&self) -> Result<(), InvalidSource> {
        if self.grounds.len().saturating_add(self.pairs.len()) >= usize::from(u16::MAX)
            || self.sprites.len() >= usize::from(u16::MAX)
        {
            return Err(InvalidSource::TooMany);
        }
        for (index, ground) in self.grounds.iter().enumerate() {
            let asset = || format!("ground `{}`", ground.id);
            if self.grounds[..index]
                .iter()
                .any(|earlier| earlier.id == ground.id)
            {
                return Err(InvalidSource::Repeated(asset()));
            }
            check_ground_blocks(&ground.variants, asset)?;
        }
        for (index, pair) in self.pairs.iter().enumerate() {
            let asset = || format!("ground pair `{}` over `{}`", pair.higher, pair.lower);
            if pair.higher == pair.lower {
                return Err(InvalidSource::PairWithItself(pair.higher.clone()));
            }
            if !self.grounds.iter().any(|g| g.id == pair.higher) {
                return Err(InvalidSource::UnknownPairGround {
                    pair: asset(),
                    ground: pair.higher.clone(),
                });
            }
            if !self.grounds.iter().any(|g| g.id == pair.lower) {
                return Err(InvalidSource::UnknownPairGround {
                    pair: asset(),
                    ground: pair.lower.clone(),
                });
            }
            if self.pairs[..index]
                .iter()
                .any(|earlier| earlier.higher == pair.higher && earlier.lower == pair.lower)
            {
                return Err(InvalidSource::Repeated(asset()));
            }
            check_ground_blocks(&pair.variants, asset)?;
        }
        for (index, sprite) in self.sprites.iter().enumerate() {
            let asset = || format!("sprite `{}`", sprite.id);
            if self.sprites[..index].iter().any(|s| s.id == sprite.id) {
                return Err(InvalidSource::Repeated(asset()));
            }
            check_variants(&sprite.variants, asset)?;
            let first = &sprite.variants[0][0];
            let size = [first.width(), first.height()];
            if size.contains(&0) {
                return Err(InvalidSource::EmptySprite(asset()));
            }
            if sprite
                .variants
                .iter()
                .flatten()
                .any(|image| [image.width(), image.height()] != size)
            {
                return Err(InvalidSource::SpriteSizesDiffer(asset()));
            }
            if sprite.anchor[0] > size[0] || sprite.anchor[1] > size[1] {
                return Err(InvalidSource::AnchorOutside(asset()));
            }
        }
        Ok(())
    }
}

fn check_ground_blocks(
    variants: &[Vec<Image>],
    asset: impl Fn() -> String,
) -> Result<(), InvalidSource> {
    check_variants(variants, &asset)?;
    for image in variants.iter().flatten() {
        if [image.width(), image.height()] != BLOCK_SIZE {
            return Err(InvalidSource::WrongBlockSize(asset()));
        }
    }
    Ok(())
}

fn check_variants(
    variants: &[Vec<Image>],
    asset: impl Fn() -> String,
) -> Result<(), InvalidSource> {
    if variants.len() != 1 && variants.len() != SEASONS {
        return Err(InvalidSource::VariantCount(asset()));
    }
    let frames = variants[0].len();
    if frames == 0 || variants.iter().any(|frames_of| frames_of.len() != frames) {
        return Err(InvalidSource::FrameCount(asset()));
    }
    Ok(())
}

/// Why source assets cannot be cooked. Each names the asset, such as
/// "ground `grass`".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidSource {
    /// Two assets of one kind share an ID.
    Repeated(String),
    /// A ground pair has the same ground on top and beneath.
    PairWithItself(GroundId),
    /// A ground pair names a ground that is not in the sources.
    UnknownPairGround { pair: String, ground: GroundId },
    /// An asset has neither one variant nor one per season.
    VariantCount(String),
    /// An asset has no frames, or its variants have different numbers of
    /// frames.
    FrameCount(String),
    /// A ground's image is not a block of 12×4 tiles.
    WrongBlockSize(String),
    /// A sprite's image has no pixels.
    EmptySprite(String),
    /// A sprite's images are not all of one size.
    SpriteSizesDiffer(String),
    /// A sprite's anchor lies outside its image.
    AnchorOutside(String),
    /// There are 2^16 grounds and pairs, or sprites, or more.
    TooMany,
}

impl fmt::Display for InvalidSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repeated(asset) => write!(f, "{asset} is defined twice"),
            Self::PairWithItself(ground) => {
                write!(f, "ground pair `{ground}` cannot pair with itself")
            }
            Self::UnknownPairGround { pair, ground } => {
                write!(f, "{pair} names unknown ground `{ground}`")
            }
            Self::VariantCount(asset) => {
                write!(
                    f,
                    "{asset} needs one variant, or one per season ({SEASONS})"
                )
            }
            Self::FrameCount(asset) => write!(
                f,
                "{asset} needs at least one frame, and the same number in every variant"
            ),
            Self::WrongBlockSize(asset) => write!(
                f,
                "{asset} must be a block of {BLOCK_COLUMNS}×{BLOCK_ROWS} tiles of {TILE_SIZE} px"
            ),
            Self::EmptySprite(asset) => write!(f, "{asset} has an empty image"),
            Self::SpriteSizesDiffer(asset) => {
                write!(f, "{asset} has images of different sizes")
            }
            Self::AnchorOutside(asset) => write!(f, "{asset} has its anchor outside its image"),
            Self::TooMany => write!(f, "there are too many grounds or sprites"),
        }
    }
}

impl Error for InvalidSource {}
