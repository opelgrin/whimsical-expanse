//! Running the simulation in real time.
//!
//! The simulation itself never reads the wall clock. This crate decides when
//! ticks are due in real time and runs the world on its own thread.

pub mod pacer;
pub mod simulation_thread;
