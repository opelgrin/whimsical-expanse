//! Gathers, culls, sorts, and batches obstacle and entity sprites for a frame.
//!
//! Obstacles are drawn once, at their anchor tile, sorted with entities by
//! anchor `y`, then `x`, then a stable key. Culling expands the visible tile
//! range by the container's largest overhang so sprites anchored outside the
//! view whose image reaches into it are still drawn.

use std::iter;

use bytemuck::{Pod, Zeroable};
use expanse_assets::{Overhang, SpriteIndex, TILE_SIZE};
use expanse_core::grid::TilePos;
use expanse_terrain::{ObstacleCell, ObstacleIndex};

use crate::assets::Assets;
use crate::camera::{Camera, Viewport, VisibleTileRange};
use crate::interpolation::Interpolation;

/// One textured sprite instance in world-tile coordinates.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct SpriteInstance {
    /// Top-left corner of the sprite in world-tile coordinates.
    pub origin: [f32; 2],
    /// Width and height of the sprite in world tiles.
    pub size: [f32; 2],
    /// Top-left texel (inside padding) on its atlas page.
    pub texel_origin: [f32; 2],
    /// Width and height in texels on its atlas page.
    pub texel_size: [f32; 2],
}

/// A contiguous run of sorted [`SpriteInstance`]s that sample one atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpriteBatch {
    pub atlas: u16,
    pub start: u32,
    pub count: u32,
}

/// Stable tie-breaker when two sprites share the exact same anchor `(x, y)`.
/// Obstacles precede entities, and entities order by their ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SpriteKey {
    Obstacle(ObstacleIndex),
    Entity(u32),
}

#[derive(Debug, Clone, Copy)]
struct SortableSprite {
    y: f32,
    x: f32,
    key: SpriteKey,
    sprite: SpriteIndex,
}

/// Reusable per-frame sprite draw list: sorted instances and their atlas runs.
#[derive(Debug, Default)]
pub(crate) struct SpriteScene {
    entries: Vec<SortableSprite>,
    pub instances: Vec<SpriteInstance>,
    pub batches: Vec<SpriteBatch>,
}

impl SpriteScene {
    /// Rebuilds the visible sprites for the current frame, reusing allocations.
    ///
    /// When `include_obstacles` is `true`, obstacle sprites anchored in the
    /// overhang-expanded view are gathered alongside entities; otherwise only
    /// entities are emitted (for flat biome/field debug views).
    pub(crate) fn build(
        &mut self,
        camera: &Camera,
        viewport: Viewport,
        interpolation: &Interpolation<'_>,
        assets: &Assets,
        include_obstacles: bool,
    ) {
        self.entries.clear();
        self.instances.clear();
        self.batches.clear();

        let Some(visible) = camera.visible_tiles(viewport) else {
            return;
        };

        let terrain = &interpolation.current.terrain;
        let container = assets.container();
        let range = expanded_range(
            visible,
            container.largest_overhang(),
            terrain.grid().height(),
        );

        if include_obstacles {
            for y in range.min_y..=range.max_y {
                for x in range.min_x..=range.max_x {
                    let Some(tile) = terrain.tile(TilePos::new(x, y)) else {
                        continue;
                    };
                    let ObstacleCell::Anchor(kind) = tile.obstacle() else {
                        continue;
                    };
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "visible tile coordinates fit within f32"
                    )]
                    let (ax, ay) = (x as f32, y as f32);
                    self.entries.push(SortableSprite {
                        y: ay,
                        x: ax,
                        key: SpriteKey::Obstacle(kind),
                        sprite: assets.obstacle_sprite(kind),
                    });
                }
            }
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "world widths and visible tile coordinates fit within f32"
        )]
        let (width, min_y, end_y) = (
            terrain.grid().width() as f32,
            range.min_y as f32,
            (range.max_y + 1) as f32,
        );
        let wanderer = assets.wanderer_sprite();
        for (id, position) in interpolation.entity_positions() {
            if position.y + 1.0 <= min_y || position.y >= end_y {
                continue;
            }
            for x in repeats(position.x, width, range) {
                self.entries.push(SortableSprite {
                    y: position.y,
                    x,
                    key: SpriteKey::Entity(id.0),
                    sprite: wanderer,
                });
            }
        }

        self.entries.sort_unstable_by(|a, b| {
            a.y.total_cmp(&b.y)
                .then_with(|| a.x.total_cmp(&b.x))
                .then_with(|| a.key.cmp(&b.key))
        });

        #[expect(clippy::cast_precision_loss, reason = "TILE_SIZE is 16, exact in f32")]
        let tile = TILE_SIZE as f32;
        for entry in &self.entries {
            let sprite = container.sprite(entry.sprite);
            // Variant 0, frame 0 until M1 item 6 selects seasonal variants.
            let region = sprite.variants()[0][0];
            let [width_px, height_px] = sprite.size();
            let [anchor_x, anchor_y] = sprite.anchor();
            #[expect(
                clippy::cast_precision_loss,
                reason = "atlas coordinates and sprite dimensions are at most 2048 px, exact in f32"
            )]
            let instance = SpriteInstance {
                origin: [
                    entry.x - anchor_x as f32 / tile,
                    entry.y - anchor_y as f32 / tile,
                ],
                size: [width_px as f32 / tile, height_px as f32 / tile],
                texel_origin: [region.x as f32, region.y as f32],
                texel_size: [width_px as f32, height_px as f32],
            };
            let start =
                u32::try_from(self.instances.len()).expect("sprite instance count fits in u32");
            self.instances.push(instance);
            if let Some(last) = self.batches.last_mut()
                && last.atlas == region.atlas
            {
                last.count += 1;
            } else {
                self.batches.push(SpriteBatch {
                    atlas: region.atlas,
                    start,
                    count: 1,
                });
            }
        }
    }
}

fn overhang_tiles(pixels: u32) -> i32 {
    i32::try_from(pixels.div_ceil(TILE_SIZE)).expect("sprite overhang in tiles fits in i32")
}

/// Expands `visible` on every side by the opposite `overhang`, so any sprite
/// anchored outside `visible` whose image reaches into it lies inside the
/// returned range. Rows stay within `0..height`.
fn expanded_range(visible: VisibleTileRange, overhang: Overhang, height: i32) -> VisibleTileRange {
    VisibleTileRange {
        min_x: visible.min_x.saturating_sub(overhang_tiles(overhang.right)),
        max_x: visible.max_x.saturating_add(overhang_tiles(overhang.left)),
        min_y: visible
            .min_y
            .saturating_sub(overhang_tiles(overhang.down))
            .max(0),
        max_y: visible
            .max_y
            .saturating_add(overhang_tiles(overhang.up))
            .min(height - 1),
    }
}

/// The unwrapped columns at which something one tile wide at canonical `x`, in
/// a world `width` tiles wide, overlaps `range`.
fn repeats(x: f32, width: f32, range: VisibleTileRange) -> impl Iterator<Item = f32> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "visible tile coordinates fit within f32"
    )]
    let (min_x, end_x) = (range.min_x as f32, (range.max_x + 1) as f32);
    let mut first = x + width * ((min_x - 1.0 - x) / width).ceil();
    if first + 1.0 <= min_x {
        first += width;
    }
    iter::successors(Some(first), move |&column| Some(column + width))
        .take_while(move |&column| column < end_x)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use expanse_assets::{Container, GroundEntry, PADDING, Region, SpriteEntry};
    use expanse_core::asset::{GroundId, SpriteId};
    use expanse_core::grid::TileOffset;
    use expanse_core::time::Tick;
    use expanse_sim::{EntityId, EntityView, Motion, Snapshot};
    use expanse_terrain::{
        BiomeDefinition, BiomeIndex, BiomeTable, FieldRanges, Fields, Footprint,
        ObstacleKindDefinition, Terrain, Tile,
    };

    use crate::assets::AssetContent;
    use crate::camera::{CameraConfig, WorldPoint};

    use super::*;

    /// Where the `tall` obstacle sprite sits on its atlas page. It is 4×5 tiles,
    /// anchored 1 tile right of and 3 tiles below its top-left corner, so it
    /// overhangs its anchor tile by 1 tile left, 2 right, 3 up and 1 down.
    const TALL: Region = Region {
        atlas: 0,
        x: 10,
        y: 20,
    };

    /// Where the 1×1 `walker` entity sprite sits, on the atlas page passed to
    /// [`build_assets`].
    const WALKER_X: u32 = 100;

    fn ground_id(name: &str) -> GroundId {
        GroundId::new(name).expect("valid ground name")
    }

    fn sprite_id(name: &str) -> SpriteId {
        SpriteId::new(name).expect("valid sprite name")
    }

    /// Builds a biome table with one obstacle kind, drawn with the `tall`
    /// sprite and covering `footprint` beside its anchor, and resolved assets
    /// whose `walker` entity sprite is on atlas page `walker_atlas`.
    fn build_assets(footprint: &[(i32, i32)], walker_atlas: u16) -> (BiomeTable, Assets) {
        let ground_region = Region {
            atlas: 0,
            x: PADDING,
            y: PADDING,
        };
        let walker = Region {
            atlas: walker_atlas,
            x: WALKER_X,
            y: TALL.y,
        };
        let container = Container::new(
            Vec::new(),
            vec![GroundEntry::new(
                ground_id("grass"),
                vec![vec![ground_region]],
            )],
            Vec::new(),
            vec![
                SpriteEntry::new(sprite_id("tall"), [64, 80], [16, 48], vec![vec![TALL]]),
                SpriteEntry::new(
                    sprite_id("walker"),
                    [TILE_SIZE, TILE_SIZE],
                    [0, 0],
                    vec![vec![walker]],
                ),
            ],
        );
        let table = BiomeTable::new(
            vec![BiomeDefinition {
                name: "plain".to_owned(),
                ranges: FieldRanges::default(),
                map_colour: [0, 0, 0],
                ground: ground_id("grass"),
                impassable: false,
                obstacles: Vec::new(),
            }],
            vec![ObstacleKindDefinition {
                name: "tall".to_owned(),
                footprint: footprint
                    .iter()
                    .map(|&(dx, dy)| TileOffset::new(dx, dy))
                    .collect(),
                sprite: sprite_id("tall"),
            }],
        )
        .expect("valid table");
        let content = AssetContent {
            drawing_order: vec![ground_id("grass")],
            wanderer_sprite: sprite_id("walker"),
        };
        let assets = Assets::resolve(container, &table, &content).expect("assets resolve");
        (table, assets)
    }

    /// A world one chunk (32×32 tiles) in size, without obstacles.
    fn empty_terrain() -> Terrain {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        Terrain::new(1, 1, |_| Tile::new(fields, BiomeIndex(0), false)).expect("valid terrain")
    }

    /// Places a single-tile obstacle of the only kind at each of `anchors`.
    fn place(terrain: &mut Terrain, anchors: &[(i32, i32)]) {
        for &(x, y) in anchors {
            terrain
                .place_obstacle(
                    TilePos::new(x, y),
                    ObstacleIndex(0),
                    &Footprint::single_tile(),
                )
                .expect("free tile");
        }
    }

    /// A snapshot of `terrain` with an entity standing still on each of
    /// `entities`.
    fn snapshot(terrain: &Terrain, entities: &[(i32, i32)]) -> Snapshot {
        let entities = (0..)
            .zip(entities)
            .map(|(id, &(x, y))| {
                let tile = TilePos::new(x, y);
                EntityView {
                    id: EntityId(id),
                    motion: Motion {
                        from: tile,
                        to: tile,
                        start: Tick::ZERO,
                        end: Tick::new(2),
                    },
                }
            })
            .collect();
        Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities,
        }
    }

    fn still(snapshot: &Snapshot) -> Interpolation<'_> {
        Interpolation {
            previous: snapshot,
            current: snapshot,
            progress: 0.0,
        }
    }

    /// A camera at 10 pixels per tile centred on `(x, y)`.
    fn camera(terrain: &Terrain, x: f32, y: f32) -> Camera {
        let config = CameraConfig {
            initial_pixels_per_tile: 10.0,
            min_pixels_per_tile: 5.0,
            max_pixels_per_tile: 40.0,
            art_pixels_per_tile: 10,
        };
        Camera::new(config, terrain.grid(), WorldPoint::new(x, y)).expect("valid camera")
    }

    /// The sprites drawn for `snapshot` through a camera centred on `(x, y)`
    /// with a `size`×`size` pixel viewport, obstacles included.
    fn draw(assets: &Assets, snapshot: &Snapshot, (x, y): (f32, f32), size: f32) -> SpriteScene {
        let mut scene = SpriteScene::default();
        scene.build(
            &camera(&snapshot.terrain, x, y),
            Viewport::new(size, size),
            &still(snapshot),
            assets,
            true,
        );
        scene
    }

    /// A drawn instance, as its sprite and the tile it is anchored at.
    #[derive(Debug, PartialEq)]
    enum Drawn {
        Tall([f32; 2]),
        Walker([f32; 2]),
    }

    /// The instances of `scene` in draw order.
    #[expect(
        clippy::cast_precision_loss,
        clippy::float_cmp,
        reason = "texel coordinates are small integers, exact in f32"
    )]
    fn drawn(scene: &SpriteScene) -> Vec<Drawn> {
        let tall = [TALL.x as f32, TALL.y as f32];
        scene
            .instances
            .iter()
            .map(|instance| {
                let [x, y] = instance.origin;
                if instance.texel_origin == tall {
                    Drawn::Tall([x + 1.0, y + 3.0])
                } else {
                    Drawn::Walker([x, y])
                }
            })
            .collect()
    }

    /// Catches culling that drops a sprite anchored outside the view whose
    /// image reaches into it, uses the wrong side's overhang, stops at the
    /// east–west seam, or keeps sprites anchored beyond the largest overhang.
    #[test]
    fn a_sprite_anchored_outside_the_view_but_reaching_into_it_is_drawn() {
        let (_, assets) = build_assets(&[], 0);
        // A 60×60 viewport centred on (3, 15) shows tiles x in 0..=5 and
        // y in 12..=17. With the `tall` overhangs, anchors at x in -2..=6 and
        // y in 11..=20 reach into it; x = -2 is canonical column 30.
        let reaching = [(30, 15), (6, 15), (3, 11), (3, 20)];
        let beyond = [(29, 15), (7, 15), (3, 10), (3, 21)];
        let mut terrain = empty_terrain();
        place(&mut terrain, &reaching);
        place(&mut terrain, &beyond);
        // Walkers have no overhang but are culled to the same range.
        let all: Vec<_> = reaching.into_iter().chain(beyond).collect();

        let scene = draw(&assets, &snapshot(&terrain, &all), (3.0, 15.0), 60.0);

        assert_eq!(
            drawn(&scene),
            [
                Drawn::Tall([3.0, 11.0]),
                Drawn::Walker([3.0, 11.0]),
                Drawn::Tall([-2.0, 15.0]),
                Drawn::Walker([-2.0, 15.0]),
                Drawn::Tall([6.0, 15.0]),
                Drawn::Walker([6.0, 15.0]),
                Drawn::Tall([3.0, 20.0]),
                Drawn::Walker([3.0, 20.0]),
            ]
        );
    }

    /// Catches sorting by the top of the image instead of the anchor, sorting
    /// by `x` before `y`, or drawing an entity before the obstacle anchored on
    /// its own tile.
    #[test]
    fn sprites_are_drawn_by_anchor_row_then_column_obstacles_first() {
        let (_, assets) = build_assets(&[], 0);
        let mut terrain = empty_terrain();
        place(&mut terrain, &[(10, 10)]);
        // The `tall` image reaches three rows north of its anchor. A walker one
        // row north stands behind the obstacle, so it is drawn first.
        let walkers = [(10, 11), (11, 10), (10, 10), (10, 9), (9, 10)];

        let scene = draw(&assets, &snapshot(&terrain, &walkers), (10.0, 10.0), 120.0);

        assert_eq!(
            drawn(&scene),
            [
                Drawn::Walker([10.0, 9.0]),
                Drawn::Walker([9.0, 10.0]),
                Drawn::Tall([10.0, 10.0]),
                Drawn::Walker([10.0, 10.0]),
                Drawn::Walker([11.0, 10.0]),
                Drawn::Walker([10.0, 11.0]),
            ]
        );
    }

    /// Catches a multi-tile obstacle drawn once per footprint tile instead of
    /// once at its anchor, a wrong size or atlas region, and a reused scene
    /// that keeps sprites from its previous build.
    #[test]
    fn a_multi_tile_obstacle_is_drawn_once_at_its_anchor() {
        let (table, assets) = build_assets(&[(0, -1), (1, -1), (1, 0)], 0);
        let mut terrain = empty_terrain();
        let footprint = table.obstacle_kind(ObstacleIndex(0)).footprint();
        terrain
            .place_obstacle(TilePos::new(10, 10), ObstacleIndex(0), footprint)
            .expect("free tiles");
        let snapshot = snapshot(&terrain, &[]);

        let mut scene = draw(&assets, &snapshot, (10.0, 10.0), 100.0);

        assert_eq!(
            scene.instances,
            [SpriteInstance {
                origin: [9.0, 7.0],
                size: [4.0, 5.0],
                texel_origin: [10.0, 20.0],
                texel_size: [64.0, 80.0],
            }]
        );

        // Debug views leave obstacles out.
        scene.build(
            &camera(&terrain, 10.0, 10.0),
            Viewport::new(100.0, 100.0),
            &still(&snapshot),
            &assets,
            false,
        );
        assert_eq!(scene.instances, []);
        assert_eq!(scene.batches, []);
    }

    /// Catches batching that reorders sprites across atlas pages (breaking
    /// painter's order) or fails to coalesce adjacent sprites on the same page.
    #[test]
    fn batches_preserve_sorted_order_across_atlas_pages() {
        let (_, assets) = build_assets(&[], 1);
        let mut terrain = empty_terrain();
        // In draw order: two obstacles on page 0, a walker on page 1, then one
        // more obstacle on page 0.
        place(&mut terrain, &[(10, 8), (10, 9), (10, 11)]);

        let scene = draw(
            &assets,
            &snapshot(&terrain, &[(10, 10)]),
            (10.0, 10.0),
            100.0,
        );

        assert_eq!(
            scene.batches,
            [
                SpriteBatch {
                    atlas: 0,
                    start: 0,
                    count: 2,
                },
                SpriteBatch {
                    atlas: 1,
                    start: 2,
                    count: 1,
                },
                SpriteBatch {
                    atlas: 0,
                    start: 3,
                    count: 1,
                },
            ]
        );
    }

    /// Catches seam repeats that are missing, duplicated or shifted, and edge
    /// checks that drop a sprite partly inside the range or keep one that only
    /// touches it.
    #[test]
    fn repeats_are_the_columns_where_a_sprite_overlaps_the_range() {
        let range = |min_x, max_x| VisibleTileRange {
            min_x,
            max_x,
            min_y: 0,
            max_y: 0,
        };
        let cases = [
            // A range wider than the world, around the seam.
            (31.25, range(-34, 33), vec![-32.75, -0.75, 31.25]),
            // The repeat at -1.0 only touches column 0.
            (31.0, range(0, 35), vec![31.0]),
            // Half inside the west edge, then touching it.
            (9.5, range(10, 15), vec![9.5]),
            (9.0, range(10, 15), vec![]),
            // Half inside the east edge, then touching it.
            (15.5, range(10, 15), vec![15.5]),
            (16.0, range(10, 15), vec![]),
        ];
        for (x, range, expected) in cases {
            // `take` turns a mistake that never ends the sequence into a failure.
            let columns: Vec<f32> = repeats(x, 32.0, range).take(4).collect();
            assert_eq!(columns, expected, "x = {x}, {range:?}");
        }
    }
}
