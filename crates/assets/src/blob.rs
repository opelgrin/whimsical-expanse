//! The 47-tile blob scheme: which shape a tile of a ground takes from its 8
//! neighbours, and where that shape sits in a 12×4 block.
//!
//! The order of the slots is the asset pack's (see
//! `dev/findings/asset-pack.md`), so that the pack's blocks and the
//! placeholders are interchangeable.

/// The columns of a blob block.
pub const BLOCK_COLUMNS: u32 = 12;
/// The rows of a blob block.
pub const BLOCK_ROWS: u32 = 4;

/// A neighbour of a tile, by direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neighbour {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl Neighbour {
    /// Every neighbour, clockwise from north.
    pub const ALL: [Self; 8] = [
        Self::North,
        Self::NorthEast,
        Self::East,
        Self::SouthEast,
        Self::South,
        Self::SouthWest,
        Self::West,
        Self::NorthWest,
    ];

    /// The offset to the neighbour, with `y` growing southwards.
    #[must_use]
    pub const fn offset(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::NorthEast => (1, -1),
            Self::East => (1, 0),
            Self::SouthEast => (1, 1),
            Self::South => (0, 1),
            Self::SouthWest => (-1, 1),
            Self::West => (-1, 0),
            Self::NorthWest => (-1, -1),
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::North => 1,
            Self::NorthEast => 2,
            Self::East => 4,
            Self::SouthEast => 8,
            Self::South => 16,
            Self::SouthWest => 32,
            Self::West => 64,
            Self::NorthWest => 128,
        }
    }
}

/// Which of a tile's 8 neighbours belong to the same layer.
///
/// The bits are N = 1, NE = 2, E = 4, SE = 8, S = 16, SW = 32, W = 64 and
/// NW = 128.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NeighbourMask(u8);

/// Each corner, with the two edges it lies between.
const CORNERS: [(Neighbour, Neighbour, Neighbour); 4] = [
    (Neighbour::NorthEast, Neighbour::North, Neighbour::East),
    (Neighbour::SouthEast, Neighbour::South, Neighbour::East),
    (Neighbour::SouthWest, Neighbour::South, Neighbour::West),
    (Neighbour::NorthWest, Neighbour::North, Neighbour::West),
];

impl NeighbourMask {
    /// The mask with these bits.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// The mask in which the neighbours for which `present` holds are set.
    #[must_use]
    pub fn from_fn(mut present: impl FnMut(Neighbour) -> bool) -> Self {
        Self(
            Neighbour::ALL
                .into_iter()
                .filter(|&neighbour| present(neighbour))
                .map(Neighbour::bit)
                .fold(0, |bits, bit| bits | bit),
        )
    }

    /// The bits of the mask.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether `neighbour` is set.
    #[must_use]
    pub const fn has(self, neighbour: Neighbour) -> bool {
        self.0 & neighbour.bit() != 0
    }

    /// The mask that gives the same shape with only the corners that matter:
    /// a corner counts only when both of its edges are present.
    #[must_use]
    pub const fn canonical(self) -> Self {
        let mut bits = self.0 & EDGE_BITS;
        let mut i = 0;
        while i < CORNERS.len() {
            let (corner, a, b) = CORNERS[i];
            if self.has(corner) && self.has(a) && self.has(b) {
                bits |= corner.bit();
            }
            i += 1;
        }
        Self(bits)
    }
}

const EDGE_BITS: u8 = 1 | 4 | 16 | 64;

/// A tile's position in a blob block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot {
    /// From 0 to [`BLOCK_COLUMNS`], exclusive.
    pub column: u32,
    /// From 0 to [`BLOCK_ROWS`], exclusive.
    pub row: u32,
}

/// The slot that holds no shape.
pub const EMPTY_SLOT: Slot = Slot { column: 10, row: 1 };

/// The canonical mask of each slot, row by row, in the pack's order. The
/// empty slot holds 0, the same as the isolated tile in the first column;
/// [`mask_of_slot`] tells them apart.
const SLOT_MASKS: [[u8; BLOCK_COLUMNS as usize]; BLOCK_ROWS as usize] = [
    [16, 20, 84, 80, 213, 92, 116, 87, 28, 125, 124, 112],
    [17, 21, 85, 81, 29, 127, 253, 113, 31, 119, 0, 245],
    [1, 5, 69, 65, 23, 223, 247, 209, 95, 255, 221, 241],
    [0, 4, 68, 64, 117, 71, 197, 93, 7, 199, 215, 193],
];

/// For each of the 256 masks, the index of its slot, row-major.
const SLOT_OF_MASK: [u8; 256] = slot_of_every_mask();

const fn slot_of_every_mask() -> [u8; 256] {
    let mut table = [u8::MAX; 256];
    let mut mask = 0;
    while mask < 256 {
        #[expect(clippy::cast_possible_truncation, reason = "mask is below 256")]
        let canonical = NeighbourMask(mask as u8).canonical().0;
        let mut row = 0;
        while row < BLOCK_ROWS as usize {
            let mut column = 0;
            while column < BLOCK_COLUMNS as usize {
                let is_empty =
                    row == EMPTY_SLOT.row as usize && column == EMPTY_SLOT.column as usize;
                if !is_empty && SLOT_MASKS[row][column] == canonical {
                    #[expect(clippy::cast_possible_truncation, reason = "48 slots fit in u8")]
                    {
                        table[mask] = (row * BLOCK_COLUMNS as usize + column) as u8;
                    }
                }
                column += 1;
            }
            row += 1;
        }
        mask += 1;
    }
    table
}

/// The slot that holds the shape for `mask`.
#[must_use]
pub fn slot(mask: NeighbourMask) -> Slot {
    let index = u32::from(SLOT_OF_MASK[usize::from(mask.0)]);
    Slot {
        column: index % BLOCK_COLUMNS,
        row: index / BLOCK_COLUMNS,
    }
}

/// The canonical mask whose shape `slot` holds, or `None` for the empty slot.
#[must_use]
pub fn mask_of_slot(slot: Slot) -> Option<NeighbourMask> {
    (slot != EMPTY_SLOT).then(|| NeighbourMask(SLOT_MASKS[slot.row as usize][slot.column as usize]))
}

/// Every slot, row by row.
pub fn slots() -> impl Iterator<Item = Slot> {
    (0..BLOCK_ROWS).flat_map(|row| (0..BLOCK_COLUMNS).map(move |column| Slot { column, row }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Catches a canonical form that keeps a corner whose edges are not both
    /// present, or drops one that matters: either changes the number of
    /// distinct shapes from the scheme's 47.
    #[test]
    fn the_256_masks_give_47_shapes() {
        let shapes: BTreeSet<u8> = (0..=u8::MAX)
            .map(|bits| NeighbourMask(bits).canonical().0)
            .collect();
        assert_eq!(shapes.len(), 47);
    }

    /// Catches a mistyped slot table: a shape missing, a shape in two slots,
    /// or a mask sent to a slot that holds another shape.
    #[test]
    fn every_mask_maps_to_the_one_slot_holding_its_shape() {
        let mut shapes_in_slots: Vec<u8> = slots()
            .filter_map(mask_of_slot)
            .map(NeighbourMask::bits)
            .collect();
        shapes_in_slots.sort_unstable();
        let mut distinct = shapes_in_slots.clone();
        distinct.dedup();
        assert_eq!(distinct, shapes_in_slots, "a shape is in two slots");
        assert_eq!(shapes_in_slots.len(), 47);

        for bits in 0..=u8::MAX {
            let mask = NeighbourMask(bits);
            assert_eq!(
                mask_of_slot(slot(mask)),
                Some(mask.canonical()),
                "mask {bits}"
            );
        }
    }
}
