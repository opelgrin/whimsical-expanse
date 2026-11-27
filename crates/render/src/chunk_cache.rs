//! Per-chunk LRU cache, invalidated by chunk pointer identity.
//!
//! A cached chunk depends on its own tiles and the border tiles of the 8 chunks
//! around it (wrapping across the seam). Storing the 9 [`ChunkStamp`]s lets the
//! cache detect any change to that chunk or its neighbours in O(1), invalidating
//! exactly the affected entries when a chunk is replaced.

use expanse_core::grid::Grid;
use expanse_core::hash::FixedHashMap;
use expanse_terrain::{ChunkPos, ChunkStamp, Terrain};

/// The identities of a chunk and the 8 chunks around it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NeighbourhoodStamps([Option<ChunkStamp>; 9]);

impl NeighbourhoodStamps {
    fn of(terrain: &Terrain, chunk: ChunkPos) -> Self {
        let mut stamps: [Option<ChunkStamp>; 9] = std::array::from_fn(|_| None);
        let mut index = 0_usize;
        for dy in -1..=1 {
            for dx in -1..=1 {
                stamps[index] = terrain.chunk_stamp(ChunkPos {
                    x: chunk.x + dx,
                    y: chunk.y + dy,
                });
                index += 1;
            }
        }
        Self(stamps)
    }
}

struct Entry<V> {
    stamps: NeighbourhoodStamps,
    last_used: u64,
    value: V,
}

/// An LRU cache of per-chunk presentation data, bounded by a chunk budget.
pub(crate) struct ChunkCache<V> {
    budget: usize,
    grid: Option<Grid>,
    frame: u64,
    entries: FixedHashMap<ChunkPos, Entry<V>>,
}

impl<V> ChunkCache<V> {
    /// Creates an empty cache that retains up to `budget` chunks across frames.
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            budget,
            grid: None,
            frame: 0,
            entries: FixedHashMap::default(),
        }
    }

    /// Drops every cached entry, such as after new assets are uploaded.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.grid = None;
    }

    /// Advances to a new frame for `grid`. Clears the cache if the terrain's
    /// grid dimensions changed (for example after regeneration).
    pub(crate) fn begin_frame(&mut self, grid: Grid) {
        if self.grid != Some(grid) {
            self.entries.clear();
            self.grid = Some(grid);
        }
        self.frame += 1;
    }

    /// Returns the cached value for canonical `chunk` if it and its 8
    /// neighbours are unchanged, or builds and stores a fresh one.
    pub(crate) fn get_or_build(
        &mut self,
        terrain: &Terrain,
        chunk: ChunkPos,
        build: impl FnOnce() -> V,
    ) -> &V {
        let stamps = NeighbourhoodStamps::of(terrain, chunk);
        let hit = self
            .entries
            .get(&chunk)
            .is_some_and(|entry| entry.stamps == stamps);
        if hit {
            let entry = self.entries.get_mut(&chunk).expect("checked entry exists");
            entry.last_used = self.frame;
            return &entry.value;
        }
        let value = build();
        self.entries.insert(
            chunk,
            Entry {
                stamps,
                last_used: self.frame,
                value,
            },
        );
        &self
            .entries
            .get(&chunk)
            .expect("entry was just inserted")
            .value
    }

    /// Returns the cached value for canonical `chunk` if it is present in the
    /// cache. Used when drawing entries already prepared in the current frame.
    pub(crate) fn get_current(&self, chunk: ChunkPos) -> Option<&V> {
        self.entries.get(&chunk).map(|entry| &entry.value)
    }

    /// Evicts the least recently used entries from earlier frames until the
    /// cache fits within its budget. Entries used during the current frame are
    /// never evicted, so a very wide viewport never thrashes within one frame.
    pub(crate) fn evict(&mut self) {
        while self.entries.len() > self.budget {
            let victim = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.last_used < self.frame)
                .min_by_key(|&(&pos, entry)| (entry.last_used, pos))
                .map(|(&pos, _)| pos);
            let Some(pos) = victim else {
                break;
            };
            self.entries.remove(&pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use expanse_core::grid::TilePos;
    use expanse_terrain::{BiomeIndex, CHUNK_SIZE, Fields, Tile};

    use super::*;

    fn terrain(width_chunks: u16, height_chunks: u16) -> Terrain {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        Terrain::new(width_chunks, height_chunks, |_| {
            Tile::new(fields, BiomeIndex(0), false)
        })
        .expect("valid terrain")
    }

    /// Catches a cache that fails to invalidate a replaced chunk or any of its
    /// 8 neighbours (including across the east–west seam), or that invalidates
    /// unrelated chunks further away.
    #[test]
    fn replacing_a_chunk_invalidates_it_and_its_eight_neighbours_only() {
        let mut terrain = terrain(4, 3);
        let mut cache = ChunkCache::new(64);
        let all_chunks: Vec<ChunkPos> = (0..3)
            .flat_map(|y| (0..4).map(move |x| ChunkPos { x, y }))
            .collect();

        // Populate every chunk in frame 1.
        cache.begin_frame(terrain.grid());
        for &chunk in &all_chunks {
            let _ = cache.get_or_build(&terrain, chunk, || 1_u32);
        }

        // Modify one tile inside chunk (0, 1) — on the west side of the seam.
        let edited = TilePos::new(2, CHUNK_SIZE + 2);
        assert!(terrain.set_biome(edited, BiomeIndex(1), false));

        // Frame 2: only chunk (0, 1) and its 8 neighbours (columns 3, 0, 1
        // across rows 0, 1, 2) must rebuild; column 2 must remain a cache hit.
        cache.begin_frame(terrain.grid());
        let mut rebuilt = Vec::new();
        for &chunk in &all_chunks {
            let mut built = false;
            let _ = cache.get_or_build(&terrain, chunk, || {
                built = true;
                2_u32
            });
            if built {
                rebuilt.push(chunk);
            }
        }
        rebuilt.sort();

        let mut expected: Vec<ChunkPos> = (0..3)
            .flat_map(|y| [0, 1, 3].into_iter().map(move |x| ChunkPos { x, y }))
            .collect();
        expected.sort();
        assert_eq!(rebuilt, expected);
    }

    /// Catches LRU eviction dropping the most recently used chunk instead of
    /// the oldest, or evicting chunks still needed in the current frame.
    #[test]
    fn eviction_drops_least_recently_used_entries_and_keeps_current_frame() {
        let terrain = terrain(4, 1);
        let mut cache = ChunkCache::new(2);
        let c = |x| ChunkPos { x, y: 0 };

        // Frame 1: load 0 and 1.
        cache.begin_frame(terrain.grid());
        let _ = cache.get_or_build(&terrain, c(0), || 1_u32);
        let _ = cache.get_or_build(&terrain, c(1), || 1_u32);
        cache.evict();

        // Frame 2: touch 0 so 1 becomes least recently used.
        cache.begin_frame(terrain.grid());
        assert_eq!(*cache.get_or_build(&terrain, c(0), || 2_u32), 1);
        cache.evict();

        // Frame 3: load 2 -> 1 must be evicted, 0 and 2 retained.
        cache.begin_frame(terrain.grid());
        let _ = cache.get_or_build(&terrain, c(2), || 3_u32);
        cache.evict();

        // Frame 4: 0 and 2 are hits, 1 must rebuild; all three used in this
        // frame stay cached until a later frame drops below budget.
        cache.begin_frame(terrain.grid());
        assert_eq!(*cache.get_or_build(&terrain, c(0), || 4_u32), 1);
        assert_eq!(*cache.get_or_build(&terrain, c(2), || 4_u32), 3);
        assert_eq!(*cache.get_or_build(&terrain, c(1), || 4_u32), 4);
        cache.evict();
        assert_eq!(cache.entries.len(), 3);

        // Frame 5: touching only 1 lets 0 and 2 evict down to budget (leaving 2).
        cache.begin_frame(terrain.grid());
        assert_eq!(*cache.get_or_build(&terrain, c(1), || 5_u32), 4);
        cache.evict();
        assert_eq!(cache.entries.len(), 2);
    }
}
