//! Where entities are drawn between two ticks.
//!
//! Presentation draws the world one tick behind the simulation, between its two
//! most recent consecutive snapshots. The positions are exact, not estimates: a
//! motion starts and ends on whole ticks, so between two consecutive ticks an
//! entity follows one motion, in a straight line at constant speed.
//!
//! Every path between two points goes the short way round the world, so a
//! step across the seam is drawn as a one-tile move. Positions are canonical:
//! `x` lies in `0..width`.

use expanse_core::grid::TilePos;
use expanse_core::hash::FixedHashMap;
use expanse_core::time::Tick;
use expanse_sim::{EntityId, Motion, Snapshot};

use crate::camera::WorldPoint;

/// Two consecutive snapshots, and how far presentation has moved from the
/// first to the second.
#[derive(Debug, Clone, Copy)]
pub struct Interpolation<'a> {
    /// The snapshot one tick before `current`, or `current` itself when the
    /// world has no earlier tick.
    pub previous: &'a Snapshot,
    /// The most recent snapshot.
    pub current: &'a Snapshot,
    /// From 0, at `previous`, to 1, at `current`. Values outside that range are
    /// clamped, so nothing is ever drawn beyond `current`.
    pub progress: f32,
}

impl Interpolation<'_> {
    /// Each entity of the current snapshot, with the position it is drawn at.
    ///
    /// Positions are in tiles: a whole-number position is exactly on a tile.
    /// An entity missing from the previous snapshot is drawn where it is at the
    /// current tick. An entity missing from the current snapshot no longer
    /// exists and is not drawn.
    pub(crate) fn entity_positions(&self) -> impl Iterator<Item = (EntityId, WorldPoint)> + '_ {
        let previous_tick = self.previous.tick;
        let previous: FixedHashMap<EntityId, Motion> = self
            .previous
            .entities
            .iter()
            .map(|entity| (entity.id, entity.motion))
            .collect();
        let progress = self.progress.clamp(0.0, 1.0);
        #[expect(
            clippy::cast_precision_loss,
            reason = "world widths in tiles are exact in f32"
        )]
        let width = self.current.terrain.grid().width() as f32;
        self.current.entities.iter().map(move |entity| {
            let to = position_at(entity.motion, self.current.tick, width);
            let position = previous.get(&entity.id).map_or(to, |&motion| {
                lerp(
                    position_at(motion, previous_tick, width),
                    to,
                    progress,
                    width,
                )
            });
            (entity.id, position)
        })
    }
}

/// Where `motion` places its entity at `tick`, in a world `width` tiles wide:
/// at `from` until the motion starts, at `to` once it ends, and in a straight
/// line between them.
fn position_at(motion: Motion, tick: Tick, width: f32) -> WorldPoint {
    let duration = motion.end.count().saturating_sub(motion.start.count());
    let elapsed = tick
        .count()
        .saturating_sub(motion.start.count())
        .min(duration);
    #[expect(
        clippy::cast_precision_loss,
        reason = "motion durations are small enough to be exact in f32"
    )]
    let fraction = if duration == 0 {
        1.0
    } else {
        elapsed as f32 / duration as f32
    };
    lerp(
        tile_point(motion.from),
        tile_point(motion.to),
        fraction,
        width,
    )
}

/// The corner of `tile` with the smallest coordinates.
fn tile_point(tile: TilePos) -> WorldPoint {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tile coordinates are small enough to be exact in f32"
    )]
    WorldPoint::new(tile.x as f32, tile.y as f32)
}

/// The point `fraction` of the way from `from` to `to`, going the short way
/// round a world `width` tiles wide. The result is canonical.
fn lerp(from: WorldPoint, to: WorldPoint, fraction: f32, width: f32) -> WorldPoint {
    let eastwards = (to.x - from.x).rem_euclid(width);
    let dx = if eastwards > width * 0.5 {
        eastwards - width
    } else {
        eastwards
    };
    WorldPoint::new(
        (from.x + dx * fraction).rem_euclid(width),
        from.y + (to.y - from.y) * fraction,
    )
}

#[cfg(test)]
mod tests {
    use expanse_sim::EntityView;
    use expanse_terrain::{BiomeIndex, Fields, Terrain, Tile};

    use super::*;

    const ID: EntityId = EntityId(7);

    /// The terrain of a small world. Only its width matters here.
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

    fn motion(from: (i32, i32), to: (i32, i32), start: u64, end: u64) -> Motion {
        Motion {
            from: TilePos::new(from.0, from.1),
            to: TilePos::new(to.0, to.1),
            start: Tick::new(start),
            end: Tick::new(end),
        }
    }

    fn snapshot(tick: u64, entities: &[(EntityId, Motion)]) -> Snapshot {
        Snapshot {
            tick: Tick::new(tick),
            terrain: terrain(),
            entities: entities
                .iter()
                .map(|&(id, motion)| EntityView { id, motion })
                .collect(),
        }
    }

    fn positions(
        previous: &Snapshot,
        current: &Snapshot,
        progress: f32,
    ) -> Vec<(EntityId, WorldPoint)> {
        Interpolation {
            previous,
            current,
            progress,
        }
        .entity_positions()
        .collect()
    }

    /// Catches interpolation along the wrong span of the motion, such as from
    /// its start rather than from the previous tick.
    #[test]
    fn moves_along_the_motion_between_the_two_ticks() {
        // One tile south over ticks 10 to 14. Halfway from tick 11 to tick 12
        // is tick 11.5: 1.5 of 4 ticks along.
        let step = motion((2, 3), (2, 4), 10, 14);
        let drawn = positions(
            &snapshot(11, &[(ID, step)]),
            &snapshot(12, &[(ID, step)]),
            0.5,
        );
        assert_eq!(drawn, vec![(ID, WorldPoint::new(2.0, 3.375))]);
    }

    /// Catches interpolation that reads only the current snapshot. When a new
    /// motion starts at the current tick, the entity must still finish the
    /// previous one instead of waiting for a tick at the end of it.
    #[test]
    fn finishes_the_previous_motion_when_a_new_one_starts_at_the_current_tick() {
        let east = motion((2, 3), (3, 3), 10, 12);
        let south = motion((3, 3), (3, 4), 12, 14);
        let drawn = positions(
            &snapshot(11, &[(ID, east)]),
            &snapshot(12, &[(ID, south)]),
            0.5,
        );
        assert_eq!(drawn, vec![(ID, WorldPoint::new(2.75, 3.0))]);
    }

    /// Catches a step across the seam drawn as a sweep across the whole world,
    /// whether inside one motion or between the positions of two ticks, and
    /// positions left outside the world. The previous tick is halfway through
    /// the step and the current one at its end; drawn a quarter of the way
    /// between them, the entity is five eighths of the way across, worked out
    /// by hand. A progress of a half would hide a path taken the long way
    /// round, which wraps to the same point.
    #[test]
    fn a_step_across_the_seam_is_drawn_as_a_one_tile_move_both_ways() {
        let east_edge = terrain().grid().width() - 1;
        #[expect(clippy::cast_precision_loss, reason = "a small test width")]
        let seam = (east_edge + 1) as f32;
        let eastwards = motion((east_edge, 3), (0, 3), 10, 14);
        let westwards = motion((0, 3), (east_edge, 3), 10, 14);
        for (step, expected) in [(eastwards, seam - 0.375), (westwards, seam - 0.625)] {
            let drawn = positions(
                &snapshot(12, &[(ID, step)]),
                &snapshot(14, &[(ID, step)]),
                0.25,
            );
            assert_eq!(drawn, vec![(ID, WorldPoint::new(expected, 3.0))]);
        }
    }

    /// The never-extrapolate rule: catches progress that is not clamped, which
    /// would draw entities where the simulation has not yet put them.
    #[test]
    fn never_draws_beyond_the_two_ticks() {
        let step = motion((2, 3), (3, 3), 10, 14);
        let (previous, current) = (snapshot(11, &[(ID, step)]), snapshot(12, &[(ID, step)]));
        assert_eq!(
            positions(&previous, &current, 1.5),
            vec![(ID, WorldPoint::new(2.5, 3.0))]
        );
        assert_eq!(
            positions(&previous, &current, -0.5),
            vec![(ID, WorldPoint::new(2.25, 3.0))]
        );
    }

    /// Catches entities that appear or disappear between the two ticks being
    /// drawn at a made-up position, or drawn after they ceased to exist.
    #[test]
    fn entities_in_only_one_snapshot_are_drawn_only_while_they_exist() {
        let gone = EntityId(1);
        let new = EntityId(2);
        let previous = snapshot(11, &[(gone, motion((0, 0), (1, 0), 10, 14))]);
        let current = snapshot(12, &[(new, motion((5, 5), (5, 6), 10, 14))]);
        assert_eq!(
            positions(&previous, &current, 0.5),
            vec![(new, WorldPoint::new(5.0, 5.5))]
        );
    }
}
