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

    fn ground_id(name: &str) -> GroundId {
        GroundId::new(name).expect("valid ground name")
    }

    fn sprite_id(name: &str) -> SpriteId {
        SpriteId::new(name).expect("valid sprite name")
    }

    /// A sprite fixture: `(name, size_px, anchor_px, region)`.
    struct TestSprite<'a> {
        name: &'a str,
        size: [u32; 2],
        anchor: [u32; 2],
        region: Region,
        footprint: &'a [(i32, i32)],
    }

    /// Builds resolved assets and a matching biome table with one ground, the
    /// given obstacle sprites, and a 1×1 `walker` entity sprite on `walker_region`.
    fn build_assets(obstacles: &[TestSprite<'_>], walker_region: Region) -> (BiomeTable, Assets) {
        let ground_region = Region {
            atlas: 0,
            x: PADDING,
            y: PADDING,
        };
        let mut sprites: Vec<SpriteEntry> = obstacles
            .iter()
            .map(|spec| {
                SpriteEntry::new(
                    sprite_id(spec.name),
                    spec.size,
                    spec.anchor,
                    vec![vec![spec.region]],
                )
            })
            .collect();
        sprites.push(SpriteEntry::new(
            sprite_id("walker"),
            [TILE_SIZE, TILE_SIZE],
            [0, 0],
            vec![vec![walker_region]],
        ));
        let container = Container::new(
            Vec::new(),
            vec![GroundEntry::new(
                ground_id("grass"),
                vec![vec![ground_region]],
            )],
            Vec::new(),
            sprites,
        );
        let obstacle_defs: Vec<ObstacleKindDefinition> = obstacles
            .iter()
            .map(|spec| ObstacleKindDefinition {
                name: spec.name.to_owned(),
                footprint: spec
                    .footprint
                    .iter()
                    .map(|&(dx, dy)| TileOffset::new(dx, dy))
                    .collect(),
                sprite: sprite_id(spec.name),
            })
            .collect();
        let table = BiomeTable::new(
            vec![BiomeDefinition {
                name: "plain".to_owned(),
                ranges: FieldRanges::default(),
                map_colour: [0, 0, 0],
                ground: ground_id("grass"),
                impassable: false,
                obstacles: Vec::new(),
            }],
            obstacle_defs,
        )
        .expect("valid table");
        let content = AssetContent {
            drawing_order: vec![ground_id("grass")],
            wanderer_sprite: sprite_id("walker"),
        };
        let assets = Assets::resolve(container, &table, &content).expect("assets resolve");
        (table, assets)
    }

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

    fn camera(terrain: &Terrain, x: f32, y: f32) -> Camera {
        let config = CameraConfig {
            initial_pixels_per_tile: 10.0,
            min_pixels_per_tile: 5.0,
            max_pixels_per_tile: 40.0,
        };
        Camera::new(config, terrain.grid(), WorldPoint::new(x, y)).expect("valid camera")
    }

    fn standing(id: u32, tile: TilePos) -> EntityView {
        EntityView {
            id: EntityId(id),
            motion: Motion {
                from: tile,
                to: tile,
                start: Tick::ZERO,
                end: Tick::new(2),
            },
        }
    }

    fn still(snapshot: &Snapshot) -> Interpolation<'_> {
        Interpolation {
            previous: snapshot,
            current: snapshot,
            progress: 0.0,
        }
    }

    /// Catches culling that drops sprites anchored outside the viewport whose
    /// image reaches into it (including across the east–west seam), or fails to
    /// cull sprites anchored strictly beyond the largest overhang.
    #[test]
    fn a_sprite_anchored_outside_the_view_but_reaching_into_it_is_drawn() {
        // An asymmetric sprite with distinct overhangs on every side:
        // left = 16 px (1 tile), right = 32 px (2 tiles),
        // up = 32 px (2 tiles), down = 16 px (1 tile).
        // Total size: 64×64 (4×4 tiles), anchor at (16, 32).
        let big_region = Region {
            atlas: 0,
            x: 10,
            y: 20,
        };
        let walker_region = Region {
            atlas: 0,
            x: 100,
            y: 20,
        };
        let (_, assets) = build_assets(
            &[TestSprite {
                name: "big",
                size: [64, 64],
                anchor: [16, 32],
                region: big_region,
                footprint: &[],
            }],
            walker_region,
        );

        let mut terrain = empty_terrain();
        // Viewport 60×60 at 10 px/tile around (15.0, 15.0) spans x in [12, 18),
        // y in [12, 18) -> visible tiles 12..=17 on both axes.
        // With overhang (left=1, right=2, up=2, down=1), the reaching anchor
        // bounds are x in 10..=18, y in 11..=19.
        let reaching = [
            TilePos::new(10, 15), // west edge: right overhang of 2 reaches x=12
            TilePos::new(18, 15), // east edge: left overhang of 1 reaches x=17
            TilePos::new(15, 11), // north edge: down overhang of 1 reaches y=12
            TilePos::new(15, 19), // south edge: up overhang of 2 reaches y=17
        ];
        let culled = [
            TilePos::new(9, 15),  // too far west
            TilePos::new(19, 15), // too far east
            TilePos::new(15, 10), // too far north
            TilePos::new(15, 20), // too far south
        ];
        for pos in reaching.into_iter().chain(culled) {
            terrain
                .place_obstacle(pos, ObstacleIndex(0), &Footprint::single_tile())
                .expect("free tile");
        }

        let snapshot = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities: vec![
                standing(1, TilePos::new(15, 10)), // north of expanded y (11..=19): culled
                standing(2, TilePos::new(15, 11)), // north edge of expanded y: drawn
                standing(3, TilePos::new(15, 19)), // south edge of expanded y: drawn
                standing(4, TilePos::new(15, 20)), // south of expanded y: culled
                standing(5, TilePos::new(9, 15)),  // west of expanded x (10..=18): culled
                standing(6, TilePos::new(10, 15)), // west edge of expanded x: drawn
                standing(7, TilePos::new(18, 15)), // east edge of expanded x: drawn
                standing(8, TilePos::new(19, 15)), // east of expanded x: culled
            ],
        };
        let mut scene = SpriteScene::default();
        scene.build(
            &camera(&terrain, 15.0, 15.0),
            Viewport::new(60.0, 60.0),
            &still(&snapshot),
            &assets,
            true,
        );

        let drawn_anchors: Vec<([f32; 2], f32)> = scene
            .instances
            .iter()
            .map(|inst| {
                let anchor = if inst.texel_origin[0] < 50.0 {
                    [inst.origin[0] + 1.0, inst.origin[1] + 2.0]
                } else {
                    inst.origin
                };
                (anchor, inst.texel_origin[0])
            })
            .collect();
        // Each reaching position draws the obstacle first, then the entity on
        // the exact same anchor, sorted by (y, x):
        // (15, 11), (10, 15), (18, 15), (15, 19).
        assert_eq!(
            drawn_anchors,
            vec![
                ([15.0, 11.0], 10.0),
                ([15.0, 11.0], 100.0),
                ([10.0, 15.0], 10.0),
                ([10.0, 15.0], 100.0),
                ([18.0, 15.0], 10.0),
                ([18.0, 15.0], 100.0),
                ([15.0, 19.0], 10.0),
                ([15.0, 19.0], 100.0),
            ]
        );
    }

    /// Catches culling that stops at column 0 instead of checking overhanging
    /// sprites anchored just west of the east–west seam.
    #[test]
    fn a_sprite_anchored_across_the_seam_whose_overhang_reaches_the_view_is_drawn() {
        let (_, assets) = build_assets(
            &[TestSprite {
                name: "big",
                size: [64, 64],
                anchor: [16, 32],
                region: Region {
                    atlas: 0,
                    x: 10,
                    y: 20,
                },
                footprint: &[],
            }],
            Region {
                atlas: 0,
                x: 100,
                y: 20,
            },
        );
        // Camera at (3.0, 15.0), viewport 60×60 -> visible x in 0..=5.
        // An obstacle anchored at canonical x = width - 2 (unwrapped x = -2)
        // has a right overhang of 2 tiles, reaching column 0 across the seam.
        let mut terrain = empty_terrain();
        let width = terrain.grid().width();
        terrain
            .place_obstacle(
                TilePos::new(width - 2, 15),
                ObstacleIndex(0),
                &Footprint::single_tile(),
            )
            .expect("free tile");
        let snapshot = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities: Vec::new(),
        };
        let mut scene = SpriteScene::default();
        scene.build(
            &camera(&terrain, 3.0, 15.0),
            Viewport::new(60.0, 60.0),
            &still(&snapshot),
            &assets,
            true,
        );
        assert_eq!(scene.instances.len(), 1);
        assert_eq!(scene.instances[0].origin, [-3.0, 13.0]);
    }

    /// Catches inverted `y` sorting, sorting by sprite top instead of anchor `y`,
    /// or unstable tie-breaking between obstacles and entities.
    #[test]
    fn entities_sort_before_obstacles_north_of_anchor_and_after_those_south() {
        let tree_region = Region {
            atlas: 0,
            x: 10,
            y: 20,
        };
        let walker_region = Region {
            atlas: 0,
            x: 100,
            y: 20,
        };
        // A 3×3 tree anchored at (1, 2): its top is two tiles north of its
        // anchor, so sorting by quad origin instead of anchor `y` would draw an
        // entity one tile north of the trunk in front of the tree.
        let (_, assets) = build_assets(
            &[TestSprite {
                name: "tree",
                size: [48, 48],
                anchor: [16, 32],
                region: tree_region,
                footprint: &[],
            }],
            walker_region,
        );

        let mut terrain = empty_terrain();
        terrain
            .place_obstacle(
                TilePos::new(10, 10),
                ObstacleIndex(0),
                &Footprint::single_tile(),
            )
            .expect("free tile");

        // Three entities: north of the tree's anchor (inside its canopy at y=9),
        // at the same row to its west (x=9, y=10) and east (x=11, y=10), and
        // south of its anchor (y=11), plus two entities sharing the exact same
        // tile to check EntityId tie-breaking.
        let snapshot = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities: vec![
                standing(5, TilePos::new(10, 11)),
                standing(2, TilePos::new(10, 9)),
                standing(4, TilePos::new(11, 10)),
                standing(3, TilePos::new(9, 10)),
                standing(9, TilePos::new(10, 11)),
            ],
        };

        let mut scene = SpriteScene::default();
        scene.build(
            &camera(&terrain, 10.0, 10.0),
            Viewport::new(120.0, 120.0),
            &still(&snapshot),
            &assets,
            true,
        );

        // Expected draw order:
        // 1. Entity 2 at (10, 9)   — north of anchor (under tree canopy)
        // 2. Entity 3 at (9, 10)   — same y, west of anchor
        // 3. Tree     at (10, 10)  — anchor (10, 10), origin (9, 8)
        // 4. Entity 4 at (11, 10)  — same y, east of anchor
        // 5. Entity 5 at (10, 11)  — south of anchor, lower EntityId
        // 6. Entity 9 at (10, 11)  — south of anchor, higher EntityId
        let summary: Vec<([f32; 2], f32)> = scene
            .instances
            .iter()
            .map(|inst| (inst.origin, inst.texel_origin[0]))
            .collect();
        assert_eq!(
            summary,
            vec![
                ([10.0, 9.0], 100.0),
                ([9.0, 10.0], 100.0),
                ([9.0, 8.0], 10.0),
                ([11.0, 10.0], 100.0),
                ([10.0, 11.0], 100.0),
                ([10.0, 11.0], 100.0),
            ]
        );
    }

    /// Catches multi-tile obstacles being drawn once per footprint tile instead
    /// of once at their anchor, or sprites drawn from the wrong anchor offset.
    #[test]
    fn a_multi_tile_obstacle_is_drawn_once_at_its_anchor() {
        let boulder_region = Region {
            atlas: 0,
            x: 12,
            y: 24,
        };
        let walker_region = Region {
            atlas: 0,
            x: 80,
            y: 24,
        };
        let (table, assets) = build_assets(
            &[TestSprite {
                name: "boulder",
                size: [32, 32],
                anchor: [0, 16],
                region: boulder_region,
                footprint: &[(0, -1), (1, -1), (1, 0)],
            }],
            walker_region,
        );
        let mut terrain = empty_terrain();
        let footprint = table.obstacle_kind(ObstacleIndex(0)).footprint();
        terrain
            .place_obstacle(TilePos::new(10, 10), ObstacleIndex(0), footprint)
            .expect("free tiles");

        let snapshot = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities: Vec::new(),
        };
        let mut scene = SpriteScene::default();
        scene.build(
            &camera(&terrain, 10.0, 10.0),
            Viewport::new(100.0, 100.0),
            &still(&snapshot),
            &assets,
            true,
        );

        assert_eq!(scene.instances.len(), 1);
        assert_eq!(
            scene.instances[0],
            SpriteInstance {
                origin: [10.0, 9.0],
                size: [2.0, 2.0],
                texel_origin: [12.0, 24.0],
                texel_size: [32.0, 32.0],
            }
        );

        // In debug views (`include_obstacles = false`), obstacle sprites are omitted.
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
        let (_, assets) = build_assets(
            &[TestSprite {
                name: "rock",
                size: [16, 16],
                anchor: [0, 0],
                region: Region {
                    atlas: 0,
                    x: 4,
                    y: 4,
                },
                footprint: &[],
            }],
            Region {
                atlas: 1,
                x: 8,
                y: 8,
            },
        );
        let mut terrain = empty_terrain();
        terrain
            .place_obstacle(
                TilePos::new(10, 8),
                ObstacleIndex(0),
                &Footprint::single_tile(),
            )
            .expect("free");
        terrain
            .place_obstacle(
                TilePos::new(10, 9),
                ObstacleIndex(0),
                &Footprint::single_tile(),
            )
            .expect("free");
        terrain
            .place_obstacle(
                TilePos::new(10, 11),
                ObstacleIndex(0),
                &Footprint::single_tile(),
            )
            .expect("free");

        // Two rocks on atlas 0 (y=8, 9), then entity on atlas 1 (y=10), then
        // one rock on atlas 0 (y=11) -> 3 batches: [atlas 0 × 2, atlas 1 × 1, atlas 0 × 1].
        let snapshot = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities: vec![standing(1, TilePos::new(10, 10))],
        };
        let mut scene = SpriteScene::default();
        scene.build(
            &camera(&terrain, 10.0, 10.0),
            Viewport::new(100.0, 100.0),
            &still(&snapshot),
            &assets,
            true,
        );

        assert_eq!(
            scene.batches,
            vec![
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

    /// Catches entities drawn at a stale snapshot position instead of the
    /// interpolated one, entities missing on seam wraps, or stale sprites left
    /// behind when rebuilding the reused scene buffer.
    #[test]
    fn entities_interpolate_wrap_across_the_seam_and_clear_on_rebuild() {
        let (_, assets) = build_assets(
            &[],
            Region {
                atlas: 0,
                x: 2,
                y: 2,
            },
        );
        let terrain = empty_terrain();
        let east_edge = terrain.grid().width() - 1;
        let at_tick = |tick| Snapshot {
            tick: Tick::new(tick),
            terrain: terrain.clone(),
            entities: vec![EntityView {
                id: EntityId(0),
                motion: Motion {
                    from: TilePos::new(east_edge, 4),
                    to: TilePos::new(0, 4),
                    start: Tick::ZERO,
                    end: Tick::new(2),
                },
            }],
        };
        let (previous, current) = (at_tick(0), at_tick(1));
        // Progress 0.5 between tick 0 (east_edge) and tick 1 (east_edge + 0.5)
        // places the entity at canonical east_edge + 0.25.
        let interpolation = Interpolation {
            previous: &previous,
            current: &current,
            progress: 0.5,
        };
        let mut scene = SpriteScene::default();
        // Viewport 680×40 at 10 px/tile centred on the seam spans x in [-34, 34]:
        // the entity shows at -32.75, -0.75, and 31.25.
        scene.build(
            &camera(&terrain, 0.0, 4.5),
            Viewport::new(680.0, 40.0),
            &interpolation,
            &assets,
            true,
        );
        let xs: Vec<f32> = scene.instances.iter().map(|inst| inst.origin[0]).collect();
        assert_eq!(xs, vec![-32.75, -0.75, 31.25]);

        // Check a standing entity at `east_edge` when `min_x == 0`: its
        // west-of-seam repeat at `-1.0` touches `0.0` without overlapping and
        // must advance to `east_edge` (31.0), not drop or wrap wrongly.
        let at_east = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain.clone(),
            entities: vec![standing(0, TilePos::new(east_edge, 4))],
        };
        // Camera at (18.0, 4.5), viewport 360×40 at 10 px/tile -> visible x in 0..=35.
        scene.build(
            &camera(&terrain, 18.0, 4.5),
            Viewport::new(360.0, 40.0),
            &still(&at_east),
            &assets,
            true,
        );
        let east_xs: Vec<f32> = scene.instances.iter().map(|inst| inst.origin[0]).collect();
        #[expect(clippy::cast_precision_loss, reason = "a small test width")]
        let expected_east = vec![east_edge as f32];
        assert_eq!(east_xs, expected_east);

        // Check a fractional entity halfway across the west boundary (`x = 9.5`
        // overlapping `min_x = 10`): it must still be drawn.
        let crossing_west = |tick| Snapshot {
            tick: Tick::new(tick),
            terrain: terrain.clone(),
            entities: vec![EntityView {
                id: EntityId(0),
                motion: Motion {
                    from: TilePos::new(9, 4),
                    to: TilePos::new(10, 4),
                    start: Tick::ZERO,
                    end: Tick::new(2),
                },
            }],
        };
        let (prev_w, curr_w) = (crossing_west(1), crossing_west(1));
        let halfway = Interpolation {
            previous: &prev_w,
            current: &curr_w,
            progress: 0.0,
        };
        // Camera at (13.0, 4.5), viewport 60×40 at 10 px/tile -> visible x in 10..=15.
        scene.build(
            &camera(&terrain, 13.0, 4.5),
            Viewport::new(60.0, 40.0),
            &halfway,
            &assets,
            true,
        );
        assert_eq!(scene.instances.len(), 1);
        assert_eq!(scene.instances[0].origin, [9.5, 4.0]);

        let empty = Snapshot {
            tick: Tick::new(1),
            terrain: terrain.clone(),
            entities: Vec::new(),
        };
        scene.build(
            &camera(&terrain, 0.0, 4.5),
            Viewport::new(680.0, 40.0),
            &still(&empty),
            &assets,
            true,
        );
        assert_eq!(scene.instances, []);
        assert_eq!(scene.batches, []);
    }
}
