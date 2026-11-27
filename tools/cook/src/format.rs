//! The container's file format: what `cook` writes and the game reads.
//!
//! All numbers are little-endian. After a magic number and the format
//! version come the atlas pages (size, then raw RGBA pixels), the grounds and
//! the sprites. A string is its length in bytes, then UTF-8. Reading checks
//! everything the game relies on, so a damaged file is an error, never a
//! panic or wrong art.

use std::error::Error;
use std::fmt;
use std::io::{self, Read, Write};

use expanse_core::asset::{GroundId, InvalidAssetName, SpriteId};

use crate::blob::{BLOCK_COLUMNS, BLOCK_ROWS};
use crate::container::{
    Atlas, CELL, Container, GroundEntry, GroundIndex, GroundPairEntry, PADDING, Region, SpriteEntry,
};
use crate::image::{Image, Rgba};
use crate::pack::MAX_ATLAS_SIZE;
use crate::source::SEASONS;

const MAGIC: [u8; 8] = *b"XPNSART\0";

/// The version of the format. Any change to what is written changes it.
pub const FORMAT_VERSION: u32 = 2;

/// The most frames an asset may have, a guard against damaged files.
const MAX_FRAMES: u32 = 64;

/// The longest name read, a guard against damaged files.
const MAX_NAME_BYTES: u32 = 256;

/// Writes `container` to `writer`.
///
/// # Errors
///
/// Returns any error from `writer`.
pub fn write_container(container: &Container, writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(&MAGIC)?;
    put_u32(writer, FORMAT_VERSION)?;
    put_count(writer, container.atlases().len())?;
    for atlas in container.atlases() {
        put_u32(writer, atlas.width())?;
        put_u32(writer, atlas.height())?;
        put_pixels(writer, atlas.image())?;
        put_pixels(writer, atlas.mipmap())?;
    }
    put_count(writer, container.grounds().len())?;
    for ground in container.grounds() {
        put_str(writer, ground.id().name())?;
        put_regions(writer, ground.variants())?;
    }
    put_count(writer, container.ground_pairs().len())?;
    for pair in container.ground_pairs() {
        writer.write_all(&pair.higher().0.to_le_bytes())?;
        writer.write_all(&pair.lower().0.to_le_bytes())?;
        put_regions(writer, pair.variants())?;
    }
    put_count(writer, container.sprites().len())?;
    for sprite in container.sprites() {
        put_str(writer, sprite.id().name())?;
        for value in sprite.size().into_iter().chain(sprite.anchor()) {
            put_u32(writer, value)?;
        }
        put_regions(writer, sprite.variants())?;
    }
    Ok(())
}

/// Reads a container from `reader`.
///
/// # Errors
///
/// Returns an error if the data is not a container, was written in another
/// version of the format, ends early, or describes something impossible,
/// such as a region outside its atlas.
///
/// # Panics
///
/// Never: an atlas page is checked to be at most [`MAX_ATLAS_SIZE`] on each
/// side before its pixels are read.
pub fn read_container(reader: &mut impl Read) -> Result<Container, ReadError> {
    let mut magic = [0; MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(ReadError::NotAContainer);
    }
    let version = get_u32(reader)?;
    if version != FORMAT_VERSION {
        return Err(ReadError::Version { found: version });
    }

    let atlas_count = get_count(reader, u32::from(u16::MAX) + 1)?;
    let mut atlases = Vec::with_capacity(atlas_count);
    for _ in 0..atlas_count {
        let width = get_u32(reader)?;
        let height = get_u32(reader)?;
        if width == 0
            || height == 0
            || width > MAX_ATLAS_SIZE
            || height > MAX_ATLAS_SIZE
            || !width.is_multiple_of(2)
            || !height.is_multiple_of(2)
        {
            return Err(ReadError::Invalid("an atlas page has invalid dimensions"));
        }
        let image = get_image(reader, width, height)?;
        let mipmap = get_image(reader, width / 2, height / 2)?;
        atlases.push(Atlas::new(image, mipmap));
    }
    let sizes: Vec<[u32; 2]> = atlases.iter().map(|a| [a.width(), a.height()]).collect();

    let ground_count = get_count(reader, u32::from(u16::MAX))?;
    let mut grounds = Vec::with_capacity(ground_count);
    let block = [
        BLOCK_COLUMNS * CELL - 2 * PADDING,
        BLOCK_ROWS * CELL - 2 * PADDING,
    ];
    for _ in 0..ground_count {
        let id = GroundId::new(get_str(reader)?)?;
        let variants = get_regions(reader, &sizes, block, true)?;
        if grounds.iter().any(|g: &GroundEntry| g.id == id) {
            return Err(ReadError::Invalid("a ground is listed twice"));
        }
        grounds.push(GroundEntry { id, variants });
    }

    let pair_count = get_count(reader, u32::from(u16::MAX))?;
    if ground_count.saturating_add(pair_count) >= usize::from(u16::MAX) {
        return Err(ReadError::Invalid("too many grounds and ground pairs"));
    }
    let mut ground_pairs = Vec::with_capacity(pair_count);
    for _ in 0..pair_count {
        let higher = GroundIndex(get_u16(reader)?);
        let lower = GroundIndex(get_u16(reader)?);
        if usize::from(higher.0) >= ground_count
            || usize::from(lower.0) >= ground_count
            || higher == lower
        {
            return Err(ReadError::Invalid("a ground pair names an invalid ground"));
        }
        if ground_pairs
            .iter()
            .any(|p: &GroundPairEntry| p.higher == higher && p.lower == lower)
        {
            return Err(ReadError::Invalid("a ground pair is listed twice"));
        }
        let variants = get_regions(reader, &sizes, block, true)?;
        ground_pairs.push(GroundPairEntry {
            higher,
            lower,
            variants,
        });
    }

    let sprite_count = get_count(reader, u32::from(u16::MAX))?;
    let mut sprites = Vec::with_capacity(sprite_count);
    for _ in 0..sprite_count {
        let id = SpriteId::new(get_str(reader)?)?;
        let size = [get_u32(reader)?, get_u32(reader)?];
        let anchor = [get_u32(reader)?, get_u32(reader)?];
        if size.contains(&0) || anchor[0] > size[0] || anchor[1] > size[1] {
            return Err(ReadError::Invalid(
                "a sprite has no pixels or its anchor outside",
            ));
        }
        let variants = get_regions(reader, &sizes, size, false)?;
        if sprites.iter().any(|s: &SpriteEntry| s.id == id) {
            return Err(ReadError::Invalid("a sprite is listed twice"));
        }
        sprites.push(SpriteEntry {
            id,
            size,
            anchor,
            variants,
        });
    }
    Ok(Container::new(atlases, grounds, ground_pairs, sprites))
}

fn put_pixels(writer: &mut impl Write, image: &Image) -> io::Result<()> {
    let bytes: Vec<u8> = image.pixels().iter().flatten().copied().collect();
    writer.write_all(&bytes)
}

fn get_image(reader: &mut impl Read, width: u32, height: u32) -> Result<Image, ReadError> {
    let mut bytes = vec![0; 4 * usize::try_from(width * height).expect("fits in memory")];
    reader.read_exact(&mut bytes)?;
    let pixels: Vec<Rgba> = bytes.as_chunks::<4>().0.to_vec();
    Ok(Image::from_pixels(width, height, pixels).expect("one pixel per position"))
}

fn put_u32(writer: &mut impl Write, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn put_count(writer: &mut impl Write, count: usize) -> io::Result<()> {
    put_u32(writer, u32::try_from(count).expect("counts fit in u32"))
}

fn put_str(writer: &mut impl Write, value: &str) -> io::Result<()> {
    put_count(writer, value.len())?;
    writer.write_all(value.as_bytes())
}

fn put_regions(writer: &mut impl Write, variants: &[Vec<Region>]) -> io::Result<()> {
    put_count(writer, variants.len())?;
    put_count(writer, variants[0].len())?;
    for region in variants.iter().flatten() {
        writer.write_all(&region.atlas.to_le_bytes())?;
        put_u32(writer, region.x)?;
        put_u32(writer, region.y)?;
    }
    Ok(())
}

fn get_u16(reader: &mut impl Read) -> Result<u16, ReadError> {
    let mut bytes = [0; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn get_u32(reader: &mut impl Read) -> Result<u32, ReadError> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn get_count(reader: &mut impl Read, max: u32) -> Result<usize, ReadError> {
    let count = get_u32(reader)?;
    if count > max {
        return Err(ReadError::Invalid("a count is too large"));
    }
    Ok(usize::try_from(count).expect("u32 fits in usize"))
}

fn get_str(reader: &mut impl Read) -> Result<String, ReadError> {
    let length = get_count(reader, MAX_NAME_BYTES)?;
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| ReadError::Invalid("a name is not UTF-8"))
}

/// Reads per-variant, per-frame regions of `size` pixels, checking that each
/// lies on its page with its padding, and if `same_atlas`, that all lie on one
/// atlas page.
fn get_regions(
    reader: &mut impl Read,
    atlas_sizes: &[[u32; 2]],
    size: [u32; 2],
    same_atlas: bool,
) -> Result<Vec<Vec<Region>>, ReadError> {
    let variant_count = get_count(reader, u32::try_from(SEASONS).expect("small"))?;
    let frame_count = get_count(reader, MAX_FRAMES)?;
    if (variant_count != 1 && variant_count != SEASONS) || frame_count == 0 {
        return Err(ReadError::Invalid(
            "an asset has the wrong number of variants or frames",
        ));
    }
    let mut first_atlas = None;
    let mut variants = Vec::with_capacity(variant_count);
    for _ in 0..variant_count {
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            let region = Region {
                atlas: get_u16(reader)?,
                x: get_u32(reader)?,
                y: get_u32(reader)?,
            };
            if same_atlas && *first_atlas.get_or_insert(region.atlas) != region.atlas {
                return Err(ReadError::Invalid(
                    "a ground's variants span several atlas pages",
                ));
            }
            let [width, height] = *atlas_sizes
                .get(usize::from(region.atlas))
                .ok_or(ReadError::Invalid("a region names a missing atlas page"))?;
            let inside = region.x >= PADDING
                && region.y >= PADDING
                && u64::from(region.x) + u64::from(size[0] + PADDING) <= u64::from(width)
                && u64::from(region.y) + u64::from(size[1] + PADDING) <= u64::from(height);
            if !inside {
                return Err(ReadError::Invalid("a region lies outside its atlas page"));
            }
            frames.push(region);
        }
        variants.push(frames);
    }
    Ok(variants)
}

/// Why a container could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// The data does not start as a container does.
    NotAContainer,
    /// The container was written in another version of the format.
    Version { found: u32 },
    /// The data ends early, or reading it failed.
    Io(io::Error),
    /// The data describes something impossible.
    Invalid(&'static str),
    /// A logical ID is not a valid name.
    Name(InvalidAssetName),
}

impl From<io::Error> for ReadError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<InvalidAssetName> for ReadError {
    fn from(err: InvalidAssetName) -> Self {
        Self::Name(err)
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAContainer => write!(f, "not a cooked asset container"),
            Self::Version { found } => write!(
                f,
                "the container is in format version {found}, but this build reads version \
                 {FORMAT_VERSION}; cook the assets again (`just cook`)"
            ),
            Self::Io(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                write!(
                    f,
                    "the container ends early; cook the assets again (`just cook`)"
                )
            }
            Self::Io(err) => write!(f, "cannot read the container: {err}"),
            Self::Invalid(problem) => write!(f, "the container is damaged: {problem}"),
            Self::Name(err) => write!(f, "the container is damaged: {err}"),
        }
    }
}

impl Error for ReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Name(err) => Some(err),
            Self::NotAContainer | Self::Version { .. } | Self::Invalid(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cook::cook;
    use crate::cook::tests::sources;

    fn written(container: &Container) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_container(container, &mut bytes).expect("writing to memory succeeds");
        bytes
    }

    /// Catches a field written and read in different orders or sizes, or left
    /// out: the game would then draw other art than was cooked.
    #[test]
    fn a_container_reads_back_as_it_was_written() {
        let container = cook(&sources(3)).expect("valid sources");
        let read = read_container(&mut written(&container).as_slice()).expect("a valid file");
        assert_eq!(read, container);
    }

    /// Catches a reader that trusts a file cooked by an older build, whose
    /// layout it would misread.
    #[test]
    fn a_container_of_another_format_version_is_refused() {
        let container = cook(&sources(3)).expect("valid sources");
        let mut bytes = written(&container);
        let version = MAGIC.len()..MAGIC.len() + 4;
        bytes[version].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(
            read_container(&mut bytes.as_slice()),
            Err(ReadError::Version { found }) if found == FORMAT_VERSION + 1
        ));
    }
}
