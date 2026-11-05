//! Running the world on its own thread.
//!
//! The simulation thread owns the [`World`]. Other threads reach it only through
//! two small shared slots, each behind a short-lived lock:
//!
//! - **controls** (speed, single steps, shutdown), where the latest value wins,
//!   so no backlog of commands can build up;
//! - **the latest snapshot**, which replaces any snapshot not yet taken.
//!
//! No lock is ever held while a tick runs or while presentation draws, so a slow
//! tick never stalls rendering.

use std::panic;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use expanse_core::time::Tick;
use expanse_sim::{Snapshot, World};

use crate::pacer::{Speed, TickPacer};

/// Controls and runs a world on a dedicated thread.
///
/// Dropping the handle stops the thread and waits for it.
#[derive(Debug)]
pub struct SimulationThread {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct Shared {
    controls: Mutex<Controls>,
    controls_changed: Condvar,
    published: Mutex<Published>,
    snapshot_published: Condvar,
}

#[derive(Debug)]
struct Controls {
    speed: Speed,
    pending_steps: u64,
    shutdown: bool,
}

#[derive(Debug)]
struct Published {
    latest: Arc<Snapshot>,
    stopped: bool,
}

impl SimulationThread {
    /// Starts running `world` at `speed`, pacing ticks with `pacer`.
    ///
    /// # Panics
    ///
    /// Panics if the operating system cannot start a thread.
    #[must_use]
    pub fn spawn(world: World, pacer: TickPacer, speed: Speed) -> Self {
        let shared = Arc::new(Shared {
            controls: Mutex::new(Controls {
                speed,
                pending_steps: 0,
                shutdown: false,
            }),
            controls_changed: Condvar::new(),
            published: Mutex::new(Published {
                latest: Arc::new(world.snapshot()),
                stopped: false,
            }),
            snapshot_published: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("simulation".into())
            .spawn(move || {
                let _stopped = MarkStoppedOnExit(&thread_shared);
                run(&thread_shared, world, pacer);
            })
            .expect("the simulation thread can be started");
        Self {
            shared,
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

    /// The most recently published snapshot.
    #[must_use]
    pub fn latest_snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&lock(&self.shared.published).latest)
    }

    /// Waits until a snapshot at `tick` or later is published, and returns it.
    /// Returns `None` if the simulation stopped before reaching `tick`.
    #[must_use]
    pub fn wait_for_tick(&self, tick: Tick) -> Option<Arc<Snapshot>> {
        let mut published = lock(&self.shared.published);
        loop {
            if published.latest.tick >= tick {
                return Some(Arc::clone(&published.latest));
            }
            if published.stopped {
                return None;
            }
            published = self
                .shared
                .snapshot_published
                .wait(published)
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
        change(&mut lock(&self.shared.controls));
        self.shared.controls_changed.notify_one();
    }
}

impl Drop for SimulationThread {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Marks the simulation as stopped when the thread exits, even by panicking, so
/// that waiters never block forever.
struct MarkStoppedOnExit<'a>(&'a Shared);

impl Drop for MarkStoppedOnExit<'_> {
    fn drop(&mut self) {
        lock(&self.0.published).stopped = true;
        self.0.snapshot_published.notify_all();
    }
}

/// The simulation thread's loop.
#[expect(
    clippy::disallowed_methods,
    reason = "real-time pacing is the one place that reads the wall clock"
)]
fn run(shared: &Shared, mut world: World, mut pacer: TickPacer) {
    let mut speed = lock(&shared.controls).speed;
    let mut last_update = Instant::now();
    loop {
        // Read the controls, sleeping while paused or until the next tick is due.
        let (new_speed, steps) = {
            let mut controls = lock(&shared.controls);
            if controls.speed == speed && !controls.shutdown && controls.pending_steps == 0 {
                controls = match pacer.time_until_next_tick(speed) {
                    Some(wait) => {
                        shared
                            .controls_changed
                            .wait_timeout(controls, wait)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0
                    }
                    None => shared
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

        let now = Instant::now();
        let elapsed = now - last_update;
        last_update = now;
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
        for _ in 0..due {
            world.advance();
        }
        lock(&shared.published).latest = Arc::new(world.snapshot());
        shared.snapshot_published.notify_all();
    }
}

/// Locks a mutex, ignoring poisoning. Shared state is only ever replaced whole,
/// so a panic elsewhere cannot leave it half-updated.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use expanse_sim::WorldConfig;

    use super::*;

    fn world() -> World {
        World::new(WorldConfig {
            seed: 11,
            width: 8,
            height: 8,
            ticks_per_step: 3,
        })
        .expect("valid config")
    }

    /// Catches a thread that advances the world differently from running it
    /// directly: ticks run while paused, steps lost or doubled, or a snapshot of
    /// the wrong state.
    #[test]
    fn stepping_while_paused_matches_running_the_world_directly() {
        let steps = 25;
        let simulation = SimulationThread::spawn(
            world(),
            TickPacer::new(20, Duration::from_millis(250)),
            Speed::Paused,
        );
        for _ in 0..steps {
            simulation.step();
        }
        let snapshot = simulation
            .wait_for_tick(Tick::new(steps))
            .expect("the simulation is running");

        let mut direct = world();
        for _ in 0..steps {
            direct.advance();
        }
        assert_eq!(*snapshot, direct.snapshot());
        simulation.shutdown();
    }
}
