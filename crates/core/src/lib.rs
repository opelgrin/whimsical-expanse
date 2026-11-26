//! Shared foundations used by every other crate: identities, logical asset IDs,
//! coordinates, deterministic hashing and randomness, and time.
//!
//! Nothing here may depend on windowing, rendering or the wall clock.

pub mod asset;
pub mod grid;
pub mod hash;
pub mod region;
pub mod rng;
pub mod time;
