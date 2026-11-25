//! 2D camera math and `wgpu` presentation, with the UI drawn on top.
//!
//! Presentation reads snapshots and never touches simulation state directly.

pub mod camera;
mod interpolation;
mod palette;
pub mod renderer;
mod scene;

pub use camera::{Camera, CameraConfig, InvalidCameraConfig, ScreenPoint, Viewport, WorldPoint};
pub use interpolation::Interpolation;
pub use palette::{MapPalette, TerrainView};
pub use renderer::{RenderError, Renderer, UiFrame};
pub use scene::{TerrainDisplay, TerrainStyle};
