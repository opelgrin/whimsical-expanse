//! 2D camera math and `wgpu` presentation.
//!
//! Presentation reads snapshots and never touches simulation state directly.

pub mod camera;
mod interpolation;
pub mod renderer;
mod scene;

pub use camera::{Camera, CameraConfig, InvalidCameraConfig, ScreenPoint, Viewport, WorldPoint};
pub use interpolation::Interpolation;
pub use renderer::{RenderError, Renderer};
