//! Builds the placeholder world scene as quad instances.

use bytemuck::{Pod, Zeroable};

use crate::camera::{Camera, Viewport};
use crate::interpolation::Interpolation;

/// One axis-aligned rectangle in world-tile coordinates with a linear RGBA colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct QuadInstance {
    origin: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

// Placeholder M0 palette in linear RGBA. Assumes an sRGB surface, which
// `build_surface_config` prefers.
const GRASS_LIGHT: [f32; 4] = [0.165, 0.345, 0.145, 1.0];
const GRASS_DARK: [f32; 4] = [0.135, 0.295, 0.120, 1.0];
const ENTITY_SHADOW: [f32; 4] = [0.040, 0.075, 0.035, 1.0];
const ENTITY_BODY: [f32; 4] = [0.880, 0.560, 0.140, 1.0];

fn checkerboard_color(x: i32, y: i32) -> [f32; 4] {
    if (x + y).rem_euclid(2) == 0 {
        GRASS_LIGHT
    } else {
        GRASS_DARK
    }
}

/// Rebuilds the visible grid and entities in draw order, reusing `instances`.
pub(crate) fn build_instances(
    instances: &mut Vec<QuadInstance>,
    camera: &Camera,
    viewport: Viewport,
    interpolation: &Interpolation<'_>,
) {
    instances.clear();

    if let Some(visible) = camera.visible_tiles(viewport) {
        for y in visible.min_y..=visible.max_y {
            for x in visible.min_x..=visible.max_x {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "visible tile coordinates fit within f32"
                )]
                let origin = [x as f32, y as f32];
                instances.push(QuadInstance {
                    origin,
                    size: [1.0, 1.0],
                    color: checkerboard_color(x, y),
                });
            }
        }
    }

    for (_, position) in interpolation.entity_positions() {
        instances.push(QuadInstance {
            origin: [position.x + 0.12, position.y + 0.16],
            size: [0.76, 0.76],
            color: ENTITY_SHADOW,
        });
        instances.push(QuadInstance {
            origin: [position.x + 0.15, position.y + 0.10],
            size: [0.70, 0.70],
            color: ENTITY_BODY,
        });
    }
}

#[cfg(test)]
mod tests {
    use expanse_core::{grid::TilePos, time::Tick};
    use expanse_sim::{EntityId, EntityView, Motion, Snapshot};
    use expanse_terrain::{BiomeIndex, Fields, Terrain, Tile};

    use crate::camera::{CameraConfig, WorldPoint};

    use super::*;

    fn terrain() -> Terrain {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        Terrain::new(1, 1, |_| Tile::new(fields, BiomeIndex(0), false))
            .expect("a terrain with tiles")
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
        let camera = Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(2.0, 1.5),
                initial_pixels_per_tile: 10.0,
                min_pixels_per_tile: 5.0,
                max_pixels_per_tile: 20.0,
            },
            4,
            3,
        )
        .expect("valid camera");
        let viewport = Viewport::new(40.0, 30.0);
        let still_at = TilePos::new(2, 1);
        let with_entity = Snapshot {
            tick: Tick::ZERO,
            terrain: terrain(),
            entities: vec![EntityView {
                id: EntityId(0),
                motion: Motion {
                    from: still_at,
                    to: still_at,
                    start: Tick::ZERO,
                    end: Tick::new(2),
                },
            }],
        };
        let mut without_entity = with_entity.clone();
        without_entity.entities.clear();

        let mut reused = Vec::new();
        build_instances(&mut reused, &camera, viewport, &still(&with_entity));
        let with_entity_count = reused.len();
        build_instances(&mut reused, &camera, viewport, &still(&without_entity));

        let mut fresh = Vec::new();
        build_instances(&mut fresh, &camera, viewport, &still(&without_entity));
        assert!(with_entity_count > fresh.len(), "the entity is represented");
        assert_eq!(reused, fresh);
    }

    /// Catches entities drawn behind the grid, or at a position other than the
    /// interpolated one, such as the tile their motion started from.
    #[test]
    fn entities_are_drawn_at_their_interpolated_position_after_the_grid() {
        let camera = Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(10.0, 4.0),
                initial_pixels_per_tile: 10.0,
                min_pixels_per_tile: 5.0,
                max_pixels_per_tile: 20.0,
            },
            20,
            8,
        )
        .expect("valid camera");
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
        let mut grid = Vec::new();
        let mut empty_snapshot = current.clone();
        empty_snapshot.entities.clear();
        build_instances(&mut grid, &camera, viewport, &still(&empty_snapshot));

        let mut scene = Vec::new();
        let interpolation = Interpolation {
            previous: &previous,
            current: &current,
            progress: 1.0,
        };
        build_instances(&mut scene, &camera, viewport, &interpolation);
        assert!(
            scene.starts_with(&grid),
            "entities must be drawn after the grid"
        );
        let entity_quads = &scene[grid.len()..];
        assert!(!entity_quads.is_empty(), "the entity is represented");
        for quad in entity_quads {
            assert_eq!(quad.origin[0].floor(), 11.0);
            assert_eq!(quad.origin[1].floor(), 4.0);
        }
    }
}
