//! Builds flat-colour world quads: flat biome/field terrain tiles and the
//! debug obstacle layer (footprint shading and anchor marks).

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use expanse_core::grid::TilePos;
use expanse_terrain::{ObstacleCell, Terrain};

use crate::camera::{Camera, Viewport, VisibleTileRange};
use crate::palette::{MapPalette, TerrainView};

/// One axis-aligned rectangle in world-tile coordinates with a linear RGBA colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct QuadInstance {
    origin: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

/// Ranges within the quad instance buffer, drawn in separate passes around the
/// sprite layer: flat terrain beneath sprites, and obstacle marks above them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuadRanges {
    pub flat_tiles: Range<u32>,
    pub obstacle_marks: Range<u32>,
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

/// Darkens every tile of an obstacle's footprint, so the footprint shows as
/// a shape over any view.
const OBSTACLE_SHADE: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
/// Marks the anchor of a footprint, inside its shade.
const ANCHOR_MARK: [f32; 4] = [0.85, 0.85, 0.80, 1.0];

/// Rebuilds the visible flat terrain tiles and obstacle layer marks, reusing
/// `instances`, and returns their disjoint ranges inside `instances`.
pub(crate) fn build_instances(
    instances: &mut Vec<QuadInstance>,
    camera: &Camera,
    viewport: Viewport,
    terrain: &Terrain,
    style: &TerrainStyle<'_>,
) -> QuadRanges {
    instances.clear();
    let Some(visible) = camera.visible_tiles(viewport) else {
        return QuadRanges {
            flat_tiles: 0..0,
            obstacle_marks: 0..0,
        };
    };
    if style.display.view != TerrainView::Terrain {
        push_flat_tiles(instances, visible, terrain, style);
    }
    let flat_end = u32::try_from(instances.len()).expect("quad instance count fits in u32");
    if style.display.obstacles {
        push_obstacle_marks(instances, visible, terrain);
    }
    let marks_end = u32::try_from(instances.len()).expect("quad instance count fits in u32");
    QuadRanges {
        flat_tiles: 0..flat_end,
        obstacle_marks: flat_end..marks_end,
    }
}

/// Pushes each visible tile in flat biome or field colour, wrapping across the
/// seam.
fn push_flat_tiles(
    instances: &mut Vec<QuadInstance>,
    visible: VisibleTileRange,
    terrain: &Terrain,
    style: &TerrainStyle<'_>,
) {
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
        }
    }
}

/// Pushes footprint shading and anchor marks for every visible obstacle tile,
/// wrapping across the seam.
fn push_obstacle_marks(
    instances: &mut Vec<QuadInstance>,
    visible: VisibleTileRange,
    terrain: &Terrain,
) {
    for y in visible.min_y..=visible.max_y {
        for x in visible.min_x..=visible.max_x {
            let Some(tile) = terrain.tile(TilePos::new(x, y)) else {
                continue;
            };
            let obstacle = tile.obstacle();
            if obstacle == ObstacleCell::Empty {
                continue;
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "visible tile coordinates fit within f32"
            )]
            let origin = [x as f32, y as f32];
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

#[cfg(test)]
mod tests {
    use expanse_core::asset::{GroundId, SpriteId};
    use expanse_core::grid::TileOffset;
    use expanse_terrain::{
        BiomeDefinition, BiomeIndex, BiomeTable, FieldRanges, Fields, Footprint, ObstacleIndex,
        ObstacleKindDefinition, Terrain, Tile,
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
            vec![ObstacleKindDefinition {
                name: "bar".to_owned(),
                footprint: vec![TileOffset::new(1, 0)],
                sprite: SpriteId::new("bar").expect("a valid name"),
            }],
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

    fn style(palette: &MapPalette, view: TerrainView, obstacles: bool) -> TerrainStyle<'_> {
        TerrainStyle {
            palette,
            display: TerrainDisplay { view, obstacles },
        }
    }

    /// Catches old quads remaining in the reused buffer when switching views or
    /// turning off the obstacle layer, and verifies flat tiles and obstacle
    /// marks occupy disjoint ranges so sprites can draw between them.
    #[test]
    fn rebuilding_discards_previous_quads_and_splits_flat_tiles_from_obstacle_marks() {
        let palette = MapPalette::new(&table());
        let camera = camera(2.0, 1.5);
        let viewport = Viewport::new(40.0, 30.0);
        let mut terrain = terrain();
        let footprint = Footprint::new([TileOffset::new(1, 0)]).expect("valid footprint");
        terrain
            .place_obstacle(TilePos::new(1, 1), ObstacleIndex(0), &footprint)
            .expect("free tiles");

        let mut reused = Vec::new();
        let both = build_instances(
            &mut reused,
            &camera,
            viewport,
            &terrain,
            &style(&palette, TerrainView::Biomes, true),
        );
        // 2-tile obstacle produces 2 shade quads + 1 anchor mark = 3 quads.
        assert!(!both.flat_tiles.is_empty());
        assert_eq!(both.obstacle_marks.len(), 3);
        assert_eq!(both.flat_tiles.end, both.obstacle_marks.start);
        assert_eq!(
            usize::try_from(both.obstacle_marks.end).expect("fits"),
            reused.len()
        );
        let marks = &reused[usize::try_from(both.obstacle_marks.start).expect("fits")..];
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(
            marks.iter().any(|quad| {
                close(quad.origin[0], 1.3)
                    && close(quad.origin[1], 1.3)
                    && close(quad.size[0], 0.4)
                    && close(quad.size[1], 0.4)
            }),
            "anchor mark must sit centred inside anchor tile (1, 1)"
        );

        let textured_only = build_instances(
            &mut reused,
            &camera,
            viewport,
            &terrain,
            &style(&palette, TerrainView::Terrain, false),
        );
        assert_eq!(textured_only.flat_tiles, 0..0);
        assert_eq!(textured_only.obstacle_marks, 0..0);
        assert_eq!(reused, []);
    }

    /// Catches a view across the seam that draws only one side, or the wrong
    /// tiles beyond it. The view is centred on the seam and two worlds wide, so
    /// the east edge shows three times: one world west, just west of the seam,
    /// and where it is.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "positions and colours are copied exactly, never computed differently"
    )]
    fn a_view_across_the_seam_shows_flat_tiles_on_both_sides() {
        let palette = MapPalette::new(&table());
        let terrain = terrain();
        let east_edge = terrain.grid().width() - 1;
        // At 10 px/tile, 680 px spans x from -34 to 34, around the seam.
        let viewport = Viewport::new(680.0, 20.0);
        let mut scene = Vec::new();
        let ranges = build_instances(
            &mut scene,
            &camera(0.0, 4.5),
            viewport,
            &terrain,
            &style(&palette, TerrainView::Biomes, false),
        );
        assert_eq!(
            ranges.obstacle_marks,
            ranges.flat_tiles.end..ranges.flat_tiles.end
        );

        let east_edge_colour = palette.colour(
            TerrainView::Biomes,
            terrain.tile(TilePos::new(east_edge, 4)).expect("a tile"),
        );
        #[expect(clippy::cast_precision_loss, reason = "a small test world")]
        let east_edge_columns = [-33.0, -1.0, east_edge as f32];
        for quad in &scene {
            let is_east_edge = east_edge_columns.contains(&quad.origin[0]);
            assert_eq!(
                quad.color == east_edge_colour,
                is_east_edge,
                "the tile drawn at {:?}",
                quad.origin
            );
        }
    }
}
