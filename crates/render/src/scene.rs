//! Builds the world scene as quad instances: the terrain, the obstacle layer
//! and the entities.

use std::iter;

use bytemuck::{Pod, Zeroable};
use expanse_core::grid::TilePos;
use expanse_terrain::ObstacleCell;

use crate::camera::{Camera, Viewport, VisibleTileRange};
use crate::interpolation::Interpolation;
use crate::palette::{MapPalette, TerrainView};

/// One axis-aligned rectangle in world-tile coordinates with a linear RGBA colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct QuadInstance {
    origin: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

/// What the terrain shows, as the developer chose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainDisplay {
    /// What colours the tiles.
    pub view: TerrainView,
    /// Whether the obstacle layer marks the tiles that obstacles block.
    pub obstacles: bool,
}

/// How the terrain is drawn.
#[derive(Debug, Clone, Copy)]
pub struct TerrainStyle<'a> {
    /// The colours of each view.
    pub palette: &'a MapPalette,
    /// What the terrain shows.
    pub display: TerrainDisplay,
}

// Placeholder palette in linear RGBA. Assumes an sRGB surface, which
// `build_surface_config` prefers.
const ENTITY_SHADOW: [f32; 4] = [0.040, 0.075, 0.035, 1.0];
const ENTITY_BODY: [f32; 4] = [0.880, 0.560, 0.140, 1.0];
/// Darkens every tile of an obstacle's footprint, so the footprint shows as
/// a shape over any view.
const OBSTACLE_SHADE: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
/// Marks the anchor of a footprint, inside its shade.
const ANCHOR_MARK: [f32; 4] = [0.85, 0.85, 0.80, 1.0];

/// Rebuilds the visible terrain and entities in draw order, reusing
/// `instances`.
pub(crate) fn build_instances(
    instances: &mut Vec<QuadInstance>,
    camera: &Camera,
    viewport: Viewport,
    interpolation: &Interpolation<'_>,
    style: &TerrainStyle<'_>,
) {
    instances.clear();
    let Some(visible) = camera.visible_tiles(viewport) else {
        return;
    };
    push_terrain(instances, visible, interpolation, style);
    push_entities(instances, visible, interpolation);
}

/// Pushes each visible tile, wrapping across the seam, with its obstacle mark
/// if the obstacle layer is shown.
fn push_terrain(
    instances: &mut Vec<QuadInstance>,
    visible: VisibleTileRange,
    interpolation: &Interpolation<'_>,
    style: &TerrainStyle<'_>,
) {
    let terrain = &interpolation.current.terrain;
    for y in visible.min_y..=visible.max_y {
        for x in visible.min_x..=visible.max_x {
            let Some(tile) = terrain.tile(TilePos::new(x, y)) else {
                continue;
            };
            #[expect(
                clippy::cast_precision_loss,
                reason = "visible tile coordinates fit within f32"
            )]
            let origin = [x as f32, y as f32];
            instances.push(QuadInstance {
                origin,
                size: [1.0, 1.0],
                color: style.palette.colour(style.display.view, tile),
            });
            let obstacle = tile.obstacle();
            if !style.display.obstacles || obstacle == ObstacleCell::Empty {
                continue;
            }
            instances.push(QuadInstance {
                origin,
                size: [1.0, 1.0],
                color: OBSTACLE_SHADE,
            });
            if let ObstacleCell::Anchor(_) = obstacle {
                instances.push(QuadInstance {
                    origin: [origin[0] + 0.3, origin[1] + 0.3],
                    size: [0.4, 0.4],
                    color: ANCHOR_MARK,
                });
            }
        }
    }
}

/// Pushes each entity wherever it shows: on both sides of the seam when the
/// view spans it, and once per repeat of a view wider than the world.
fn push_entities(
    instances: &mut Vec<QuadInstance>,
    visible: VisibleTileRange,
    interpolation: &Interpolation<'_>,
) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "world widths in tiles are exact in f32"
    )]
    let width = interpolation.current.terrain.grid().width() as f32;
    for (_, position) in interpolation.entity_positions() {
        for x in repeats(position.x, width, visible) {
            instances.push(QuadInstance {
                origin: [x + 0.12, position.y + 0.16],
                size: [0.76, 0.76],
                color: ENTITY_SHADOW,
            });
            instances.push(QuadInstance {
                origin: [x + 0.15, position.y + 0.10],
                size: [0.70, 0.70],
                color: ENTITY_BODY,
            });
        }
    }
}

/// The columns at which something one tile wide at canonical `x`, in a world
/// `width` tiles wide, overlaps the `visible` columns.
fn repeats(x: f32, width: f32, visible: VisibleTileRange) -> impl Iterator<Item = f32> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "visible tile coordinates fit within f32"
    )]
    let (min_x, end_x) = (visible.min_x as f32, (visible.max_x + 1) as f32);
    let first = x + width * ((min_x - 1.0 - x) / width).ceil();
    iter::successors(Some(first), move |&column| Some(column + width))
        .take_while(move |&column| column < end_x)
}

#[cfg(test)]
mod tests {
    use expanse_core::asset::GroundId;
    use expanse_core::time::Tick;
    use expanse_sim::{EntityId, EntityView, Motion, Snapshot};
    use expanse_terrain::{
        BiomeDefinition, BiomeIndex, BiomeTable, FieldRanges, Fields, Terrain, Tile,
    };

    use crate::camera::{CameraConfig, WorldPoint};

    use super::*;

    /// A biome that marks the east edge, and one for every other tile.
    fn table() -> BiomeTable {
        let biome = |name: &str, map_colour, ranges| BiomeDefinition {
            name: name.to_owned(),
            ranges,
            map_colour,
            ground: GroundId::new(name).expect("a valid name"),
            impassable: false,
            obstacles: Vec::new(),
        };
        let marked = FieldRanges {
            elevation: Some(0.0..1.0),
            ..FieldRanges::default()
        };
        BiomeTable::new(
            vec![
                biome("east_edge", [200, 40, 40], marked),
                biome("elsewhere", [40, 40, 200], FieldRanges::default()),
            ],
            Vec::new(),
        )
        .expect("a valid table")
    }

    /// One chunk, whose east edge has its own biome.
    fn terrain() -> Terrain {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        Terrain::new(1, 1, |tile| {
            let east_edge = tile.x == expanse_terrain::CHUNK_SIZE - 1;
            Tile::new(fields, BiomeIndex(u16::from(!east_edge)), false)
        })
        .expect("a terrain with tiles")
    }

    fn camera(x: f32, y: f32) -> Camera {
        let config = CameraConfig {
            initial_pixels_per_tile: 10.0,
            min_pixels_per_tile: 5.0,
            max_pixels_per_tile: 20.0,
        };
        Camera::new(config, terrain().grid(), WorldPoint::new(x, y)).expect("valid camera")
    }

    fn style(palette: &MapPalette) -> TerrainStyle<'_> {
        TerrainStyle {
            palette,
            display: TerrainDisplay {
                view: TerrainView::Biomes,
                obstacles: false,
            },
        }
    }

    /// A snapshot at `tick` of an entity standing still on `tile`.
    fn standing(tick: u64, tile: TilePos) -> Snapshot {
        Snapshot {
            tick: Tick::new(tick),
            terrain: terrain(),
            entities: vec![EntityView {
                id: EntityId(0),
                motion: Motion {
                    from: tile,
                    to: tile,
                    start: Tick::ZERO,
                    end: Tick::new(2),
                },
            }],
        }
    }

    /// Draws `snapshot` as if no time had passed since the previous tick.
    fn still(snapshot: &Snapshot) -> Interpolation<'_> {
        Interpolation {
            previous: snapshot,
            current: snapshot,
            progress: 0.0,
        }
    }

    /// Catches old quads remaining in the reused buffer after an entity disappears.
    #[test]
    fn rebuilding_a_scene_discards_instances_from_the_previous_snapshot() {
        let palette = MapPalette::new(&table());
        let camera = camera(2.0, 1.5);
        let viewport = Viewport::new(40.0, 30.0);
        let with_entity = standing(0, TilePos::new(2, 1));
        let mut without_entity = with_entity.clone();
        without_entity.entities.clear();

        let mut reused = Vec::new();
        let style = style(&palette);
        build_instances(&mut reused, &camera, viewport, &still(&with_entity), &style);
        let with_entity_count = reused.len();
        build_instances(
            &mut reused,
            &camera,
            viewport,
            &still(&without_entity),
            &style,
        );

        let mut fresh = Vec::new();
        build_instances(
            &mut fresh,
            &camera,
            viewport,
            &still(&without_entity),
            &style,
        );
        assert!(with_entity_count > fresh.len(), "the entity is represented");
        assert_eq!(reused, fresh);
    }

    /// Catches entities drawn behind the terrain, or at a position other than
    /// the interpolated one, such as the tile their motion started from.
    #[test]
    fn entities_are_drawn_at_their_interpolated_position_after_the_terrain() {
        let palette = MapPalette::new(&table());
        let style = style(&palette);
        let camera = camera(10.0, 4.0);
        // A step east from tile 10 to tile 11, completed at the current tick.
        let at_tick = |tick| Snapshot {
            tick: Tick::new(tick),
            terrain: terrain(),
            entities: vec![EntityView {
                id: EntityId(0),
                motion: Motion {
                    from: TilePos::new(10, 4),
                    to: TilePos::new(11, 4),
                    start: Tick::ZERO,
                    end: Tick::new(2),
                },
            }],
        };
        let (previous, current) = (at_tick(1), at_tick(2));
        let viewport = Viewport::new(200.0, 80.0);
        let mut ground = Vec::new();
        let mut empty_snapshot = current.clone();
        empty_snapshot.entities.clear();
        build_instances(
            &mut ground,
            &camera,
            viewport,
            &still(&empty_snapshot),
            &style,
        );

        let mut scene = Vec::new();
        let interpolation = Interpolation {
            previous: &previous,
            current: &current,
            progress: 1.0,
        };
        build_instances(&mut scene, &camera, viewport, &interpolation, &style);
        assert!(
            scene.starts_with(&ground),
            "entities must be drawn after the terrain"
        );
        let entity_quads = &scene[ground.len()..];
        assert!(!entity_quads.is_empty(), "the entity is represented");
        for quad in entity_quads {
            assert_eq!(quad.origin[0].floor(), 11.0);
            assert_eq!(quad.origin[1].floor(), 4.0);
        }
    }

    /// Catches a view across the seam that draws only one side, or the
    /// wrong tiles beyond it, and entities drawn only where they are
    /// canonical. The view is centred on the seam and two worlds wide, so
    /// the east edge shows three times: one world west, just west of the
    /// seam, and where it is.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "positions and colours are copied exactly, never computed differently"
    )]
    fn a_view_across_the_seam_shows_both_sides_with_their_entities() {
        let palette = MapPalette::new(&table());
        let style = style(&palette);
        let terrain = terrain();
        let east_edge = terrain.grid().width() - 1;
        let snapshot = standing(0, TilePos::new(east_edge, 4));
        // At 10 px/tile, 680 px spans x from -34 to 34, around the seam.
        let viewport = Viewport::new(680.0, 20.0);
        let mut scene = Vec::new();
        build_instances(
            &mut scene,
            &camera(0.0, 4.5),
            viewport,
            &still(&snapshot),
            &style,
        );

        let (tiles, entities): (Vec<&QuadInstance>, Vec<&QuadInstance>) =
            scene.iter().partition(|quad| quad.size == [1.0, 1.0]);
        let east_edge_colour = palette.colour(
            TerrainView::Biomes,
            terrain.tile(TilePos::new(east_edge, 4)).expect("a tile"),
        );
        #[expect(clippy::cast_precision_loss, reason = "a small test world")]
        let east_edge_columns = [-33.0, -1.0, east_edge as f32];
        for quad in &tiles {
            let is_east_edge = east_edge_columns.contains(&quad.origin[0]);
            assert_eq!(
                quad.color == east_edge_colour,
                is_east_edge,
                "the tile drawn at {:?}",
                quad.origin
            );
        }
        let mut entity_columns: Vec<f32> =
            entities.iter().map(|quad| quad.origin[0].floor()).collect();
        entity_columns.dedup();
        assert_eq!(entity_columns, east_edge_columns);
    }
}
