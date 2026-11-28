//! Running the world on its own thread.
//!
//! The simulation thread owns the [`World`]. Other threads reach it only through
//! two small shared slots, each behind a short-lived lock:
//!
//! - **controls** (speed, single steps, shutdown), where the latest value wins,
//!   so no backlog of commands can build up;
//! - **the snapshot feed**, whose latest [`SnapshotPair`] replaces any not yet
//!   taken.
//!
//! No lock is ever held while a tick runs or while presentation draws, so a slow
//! tick never stalls rendering.

use std::panic;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use expanse_core::time::Tick;
use expanse_sim::{Snapshot, World};

use crate::pacer::{Speed, TickPacer};

/// A handle to a world running on its own dedicated thread.
///
/// Dropping the handle stops the thread and waits for it.
#[derive(Debug)]
pub struct SimulationHandle {
    slots: Arc<Slots>,
    thread: Option<JoinHandle<()>>,
}

/// The state shared between the handle and the simulation thread: one slot per
/// direction, each a mutex with a condvar to wake the side that is waiting.
#[derive(Debug)]
struct Slots {
    controls: Mutex<Controls>,
    controls_changed: Condvar,
    feed: Mutex<SnapshotFeed>,
    feed_changed: Condvar,
}

/// Inbox, from the handle to the simulation. The latest value wins, so there is
/// never a backlog of commands.
#[derive(Debug)]
struct Controls {
    speed: Speed,
    pending_steps: u64,
    shutdown: bool,
}

/// Outbox, from the simulation to the handle: the latest snapshots, and whether
/// more will ever follow.
///
/// `closed` lives under the same lock as `latest` so that a waiter checking both
/// and then sleeping cannot miss the wake-up that closes the feed.
#[derive(Debug)]
struct SnapshotFeed {
    latest: SnapshotPair,
    closed: bool,
}

/// The snapshots of the world's last two ticks, published together.
///
/// Presentation draws one tick behind, between `previous` and `current`. The
/// pair is replaced as a whole, so the two ticks are always consecutive, even
/// when several ticks run between publications.
#[derive(Debug, Clone)]
pub struct SnapshotPair {
    /// The snapshot one tick before `current`. Before the first tick, the same
    /// as `current`.
    pub previous: Arc<Snapshot>,
    /// The most recent snapshot.
    pub current: Arc<Snapshot>,
    published_at: Instant,
    tick_length: Duration,
}

impl SnapshotPair {
    /// How far presentation should be from `previous` to `current` at `now`:
    /// 0 when the pair is published, or earlier, and 1 one tick later. It keeps
    /// growing past 1 if the next pair is late; the renderer's
    /// [`Interpolation`](expanse_render::Interpolation) clamps it, so nothing is
    /// drawn beyond `current`.
    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.published_at);
        elapsed.as_secs_f32() / self.tick_length.as_secs_f32()
    }
}

impl SimulationHandle {
    /// Starts running `world` at `speed`, pacing ticks with `pacer`.
    ///
    /// # Panics
    ///
    /// Panics if the operating system cannot start a thread.
    #[must_use]
    pub fn spawn(world: World, pacer: TickPacer, speed: Speed) -> Self {
        let initial = Arc::new(world.snapshot());
        let slots = Arc::new(Slots {
            controls: Mutex::new(Controls {
                speed,
                pending_steps: 0,
                shutdown: false,
            }),
            controls_changed: Condvar::new(),
            feed: Mutex::new(SnapshotFeed {
                latest: SnapshotPair {
                    previous: Arc::clone(&initial),
                    current: initial,
                    published_at: now(),
                    tick_length: pacer.tick_length_on_screen(speed),
                },
                closed: false,
            }),
            feed_changed: Condvar::new(),
        });
        let thread_slots = Arc::clone(&slots);
        #[expect(
            clippy::disallowed_methods,
            reason = "the simulation thread lives as long as the world, unlike pool work"
        )]
        let thread = thread::Builder::new()
            .name("simulation".into())
            .spawn(move || {
                let _close = CloseFeedOnExit(&thread_slots);
                run(&thread_slots, world, pacer);
            })
            .expect("the simulation thread can be started");
        Self {
            slots,
            thread: Some(thread),
        }
    }

    /// Selects the simulation speed.
    pub fn set_speed(&self, speed: Speed) {
        self.update_controls(|controls| controls.speed = speed);
    }

    /// Requests one more tick. Only has an effect while paused.
    pub fn step(&self) {
        self.update_controls(|controls| controls.pending_steps += 1);
    }

    /// The most recently published snapshots.
    #[must_use]
    pub fn latest_snapshots(&self) -> SnapshotPair {
        lock(&self.slots.feed).latest.clone()
    }

    /// Waits until a pair whose current snapshot is at `tick` or later is
    /// published, and returns it. Returns `None` if the simulation stopped
    /// before reaching `tick`.
    #[must_use]
    pub fn wait_for_tick(&self, tick: Tick) -> Option<SnapshotPair> {
        let mut feed = lock(&self.slots.feed);
        loop {
            if feed.latest.current.tick >= tick {
                return Some(feed.latest.clone());
            }
            if feed.closed {
                return None;
            }
            feed = self
                .slots
                .feed_changed
                .wait(feed)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Stops the simulation and waits for its thread to finish.
    ///
    /// # Panics
    ///
    /// Re-raises a panic that happened on the simulation thread.
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.update_controls(|controls| controls.shutdown = true);
        if let Some(thread) = self.thread.take()
            && let Err(payload) = thread.join()
            && !thread::panicking()
        {
            panic::resume_unwind(payload);
        }
    }

    fn update_controls(&self, change: impl FnOnce(&mut Controls)) {
        change(&mut lock(&self.slots.controls));
        self.slots.controls_changed.notify_one();
    }
}

impl Drop for SimulationHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Closes the snapshot feed when the thread exits, even by panicking, so that
/// waiters never block forever.
struct CloseFeedOnExit<'a>(&'a Slots);

impl Drop for CloseFeedOnExit<'_> {
    fn drop(&mut self) {
        lock(&self.0.feed).closed = true;
        self.0.feed_changed.notify_all();
    }
}

/// The simulation thread's loop.
fn run(slots: &Slots, mut world: World, mut pacer: TickPacer) {
    let mut speed = lock(&slots.controls).speed;
    let mut last_update = now();
    loop {
        // Read the controls, sleeping while paused or until the next tick is due.
        let (new_speed, steps) = {
            let mut controls = lock(&slots.controls);
            if controls.speed == speed && !controls.shutdown && controls.pending_steps == 0 {
                controls = match pacer.time_until_next_tick(speed) {
                    Some(wait) => {
                        slots
                            .controls_changed
                            .wait_timeout(controls, wait)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0
                    }
                    None => slots
                        .controls_changed
                        .wait(controls)
                        .unwrap_or_else(PoisonError::into_inner),
                };
            }
            if controls.shutdown {
                return;
            }
            let steps = if controls.speed == Speed::Paused {
                std::mem::take(&mut controls.pending_steps)
            } else {
                controls.pending_steps = 0;
                0
            };
            (controls.speed, steps)
        };

        let update = now();
        let elapsed = update - last_update;
        last_update = update;
        if new_speed != speed {
            // Time owed at the old speed, or spent paused, is not caught up.
            pacer.reset();
            speed = new_speed;
        }

        let due = if speed == Speed::Paused {
            steps
        } else {
            pacer.due_ticks(elapsed, speed)
        };
        if due == 0 {
            continue;
        }
        // Only this thread publishes, so the feed still holds the snapshot of
        // `world` as it is now.
        let published = Arc::clone(&lock(&slots.feed).latest.current);
        let (previous, current) = advance(&mut world, due, published);
        lock(&slots.feed).latest = SnapshotPair {
            previous,
            current,
            published_at: now(),
            tick_length: pacer.tick_length_on_screen(speed),
        };
        slots.feed_changed.notify_all();
    }
}

/// Advances `world` by `ticks`, at least one, and returns the snapshots of its
/// last two ticks. `published` is the snapshot of `world` before advancing, so
/// a single tick needs only one new snapshot.
fn advance(
    world: &mut World,
    ticks: u64,
    published: Arc<Snapshot>,
) -> (Arc<Snapshot>, Arc<Snapshot>) {
    debug_assert!(ticks > 0, "advancing by zero ticks publishes nothing new");
    for _ in 1..ticks {
        world.advance();
    }
    let previous = if ticks > 1 {
        Arc::new(world.snapshot())
    } else {
        published
    };
    world.advance();
    (previous, Arc::new(world.snapshot()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "real-time pacing and the publication times of snapshots read the wall clock"
)]
fn now() -> Instant {
    Instant::now()
}

/// Locks a mutex, ignoring poisoning. Slots state is only ever replaced whole,
/// so a panic elsewhere cannot leave it half-updated.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use expanse_sim::WorldConfig;
    use expanse_terrain::{BiomeIndex, Fields, Terrain, Tile};

    use super::*;

    fn world() -> World {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        let surface = Terrain::new(1, 1, |_| Tile::new(fields, BiomeIndex(0), false))
            .expect("a terrain with tiles");
        let config = WorldConfig {
            seed: 11,
            ticks_per_step: 3,
        };
        World::new(config, surface).expect("valid config")
    }

    /// Catches a thread that advances the world differently from running it
    /// directly: ticks run while paused, steps lost or doubled, or a snapshot of
    /// the wrong state.
    #[test]
    fn stepping_while_paused_matches_running_the_world_directly() {
        let steps = 25;
        let simulation = SimulationHandle::spawn(
            world(),
            TickPacer::new(20, Duration::from_millis(250)),
            Speed::Paused,
        );
        for _ in 0..steps {
            simulation.step();
        }
        let snapshots = simulation
            .wait_for_tick(Tick::new(steps))
            .expect("the simulation is running");

        let mut direct = world();
        for _ in 0..steps {
            direct.advance();
        }
        assert_eq!(*snapshots.current, direct.snapshot());
        simulation.shutdown();
    }

    /// Catches a previous snapshot taken at the wrong tick when several ticks
    /// run at once, which would make entities jump instead of moving smoothly.
    #[test]
    fn the_published_pair_is_the_last_two_ticks() {
        for ticks in [1, 2, 5] {
            let mut advanced = world();
            let published = Arc::new(advanced.snapshot());
            let (previous, current) = advance(&mut advanced, ticks, published);

            let mut direct = world();
            for _ in 1..ticks {
                direct.advance();
            }
            assert_eq!(*previous, direct.snapshot(), "{ticks} ticks");
            direct.advance();
            assert_eq!(*current, direct.snapshot(), "{ticks} ticks");
        }
    }

    /// Catches progress measured against the wrong tick length or from the
    /// wrong instant, or negative for a frame timed just before publication.
    #[test]
    fn progress_is_the_fraction_of_a_tick_since_publication() {
        let snapshot = Arc::new(world().snapshot());
        let published_at = now();
        let pair = SnapshotPair {
            previous: Arc::clone(&snapshot),
            current: snapshot,
            published_at,
            tick_length: Duration::from_secs(2),
        };
        let after = |millis| published_at + Duration::from_millis(millis);
        assert_eq!(pair.progress(published_at), 0.0);
        assert_eq!(pair.progress(after(500)), 0.25);
        assert_eq!(pair.progress(after(2000)), 1.0);

        let early = SnapshotPair {
            published_at: after(500),
            ..pair
        };
        assert_eq!(early.progress(published_at), 0.0);
    }
}
