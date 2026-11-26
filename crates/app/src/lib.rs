//! Running the simulation in real time and presenting it in a window.
//!
//! The simulation itself never reads the wall clock. This crate paces ticks in
//! real time, runs the world on its own thread, and connects window input to
//! presentation and the debug overlay.

pub mod assets;
pub mod camera_controls;
pub mod genesis;
pub mod input;
pub mod inspector;
pub mod overlay;
pub mod pacer;
pub mod simulation_thread;
pub mod window;
