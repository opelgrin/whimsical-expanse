//! 2D camera math and `wgpu` presentation, with the UI drawn on top.
//!
//! Presentation reads snapshots and never touches simulation state directly.

mod assets;
pub mod camera;
mod chunk_cache;
mod interpolation;
mod palette;
pub mod renderer;
mod scene;
mod sprite_pipeline;
mod sprites;
mod terrain_mesh;
mod terrain_pipeline;

pub use assets::{
    AssetContent, AssetProblem, Assets, BlockId, GroundBlock, GroundRank, MissingAssets,
};
pub use camera::{Camera, CameraConfig, InvalidCameraConfig, ScreenPoint, Viewport, WorldPoint};
pub use interpolation::Interpolation;
pub use palette::{MapPalette, TerrainView};
pub use renderer::{AtlasTexture, RenderError, Renderer, UiFrame};
pub use scene::{TerrainDisplay, TerrainStyle};
