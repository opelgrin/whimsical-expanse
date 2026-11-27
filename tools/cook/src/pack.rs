//! Placing rectangles on atlas pages, without overlap.

use std::error::Error;
use std::fmt;

use expanse_assets::MAX_ATLAS_SIZE;

/// Where a rectangle was placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The page.
    pub atlas: u16,
    /// The top-left corner on the page.
    pub x: u32,
    pub y: u32,
}

/// Where each rectangle went, and how large each page came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packing {
    /// One placement per rectangle, in the order given.
    pub placements: Vec<Placement>,
    /// The width and height of each page.
    pub atlas_sizes: Vec<[u32; 2]>,
}

/// Places rectangles of `sizes` in shelves: rows filled left to right, the
/// tallest rectangles first. A page grows only as tall as its shelves, and a
/// new page starts when one would grow past [`MAX_ATLAS_SIZE`].
///
/// # Errors
///
/// Returns an error if a rectangle is larger than a page.
pub fn pack(sizes: &[[u32; 2]]) -> Result<Packing, TooLarge> {
    if let Some(index) = sizes
        .iter()
        .position(|&[width, height]| width > MAX_ATLAS_SIZE || height > MAX_ATLAS_SIZE)
    {
        return Err(TooLarge { index });
    }
    // Tallest first, ties broken by width then position, so the order depends
    // only on the sizes.
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&index| {
        let [width, height] = sizes[index];
        (std::cmp::Reverse(height), std::cmp::Reverse(width), index)
    });

    let mut placements = vec![
        Placement {
            atlas: 0,
            x: 0,
            y: 0
        };
        sizes.len()
    ];
    let mut atlas_sizes: Vec<[u32; 2]> = Vec::new();
    let mut shelf = Shelf::default();
    for index in order {
        let [width, height] = sizes[index];
        if shelf.x + width > MAX_ATLAS_SIZE {
            shelf = shelf.next();
        }
        if atlas_sizes.is_empty() || shelf.y + height > MAX_ATLAS_SIZE {
            atlas_sizes.push([0, 0]);
            shelf = Shelf::default();
        }
        let atlas = atlas_sizes.len() - 1;
        placements[index] = Placement {
            atlas: u16::try_from(atlas).expect("fewer than 2^16 atlas pages"),
            x: shelf.x,
            y: shelf.y,
        };
        shelf.x += width;
        shelf.height = shelf.height.max(height);
        let size = &mut atlas_sizes[atlas];
        size[0] = size[0].max(shelf.x);
        size[1] = size[1].max(shelf.y + shelf.height);
    }
    Ok(Packing {
        placements,
        atlas_sizes,
    })
}

/// The row being filled on the current page.
#[derive(Debug, Clone, Copy, Default)]
struct Shelf {
    /// Where the next rectangle goes.
    x: u32,
    /// The top of the row.
    y: u32,
    /// The tallest rectangle in the row so far.
    height: u32,
}

impl Shelf {
    /// The row below this one.
    const fn next(self) -> Self {
        Self {
            x: 0,
            y: self.y + self.height,
            height: 0,
        }
    }
}

/// A rectangle larger than an atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    /// Its position in the sizes given.
    pub index: usize,
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "an image is larger than an atlas page ({MAX_ATLAS_SIZE}×{MAX_ATLAS_SIZE} px)"
        )
    }
}

impl Error for TooLarge {}
