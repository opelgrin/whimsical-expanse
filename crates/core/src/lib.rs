//! Shared foundations used by every other crate: identities, coordinates,
//! deterministic hashing and randomness, and time.
//!
//! Nothing here may depend on windowing, rendering or the wall clock.

pub mod grid;
pub mod hash;
pub mod rng;
pub mod time;
