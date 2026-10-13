//! wgpu renderer for the gameplay scene.
//!
//! Draws an `engine::Frame` with instanced procedural quads (`sprite`) and
//! glyphon text (`text`). Receives a `wgpu::Device`, `Queue` and target view
//! from the platform shell; never touches web-sys or winit, so it runs
//! unchanged on web and desktop.
//!
//! See `docs/PLAN.md` §3.4.

pub mod art;
pub mod background;
pub mod color;
pub mod model;
pub mod note_colors;
pub mod noteskin;
pub mod renderer;
pub mod scene;
pub mod sprite;
pub mod text;
pub mod textured;

pub use background::{Backdrop, BackgroundPipeline};
pub use renderer::Renderer;
pub use scene::{ColorScheme, FieldGeometry, RenderOptions, Skin};
pub use sprite::{Instance, Shape, SpritePipeline, Symbol, SymbolMode};
pub use text::{Align, TextItem, TextPipeline};
