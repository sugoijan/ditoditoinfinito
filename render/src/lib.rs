//! wgpu renderer for the gameplay scene and the 2D pass.
//!
//! Draws an `engine::Frame` with instanced procedural quads (`sprite`) and
//! glyphon text (`text`), and a screen's draw list (`draw2d`) with the
//! `painter`, its textures cached by source (`texture_cache`) and its
//! text runs on atlas pages (`text_atlas`). Receives a `wgpu::Device`,
//! `Queue` and target view from the platform shell; never touches web-sys
//! or winit, so it runs unchanged on web and desktop.
//!
//! See `docs/PLAN.md` §3.4 and `docs/plans/ui-core.md`.

pub mod art;
pub mod atlas;
pub mod background;
pub mod color;
pub mod cosmic;
pub mod draw2d;
mod instances;
mod mipmaps;
pub mod model;
pub mod note_colors;
pub mod noteskin;
mod offscreen;
pub mod painter;
pub mod renderer;
pub mod scene;
pub mod sprite;
pub mod stage;
pub mod text;
pub mod text_atlas;
pub mod texture_cache;
pub mod textured;

pub use background::{Backdrop, BackgroundPipeline};
pub use draw2d::{Affine, Blend, Command, DrawList, Quad, TextRun};
pub use painter::{Painter, PainterStats};
pub use renderer::{Renderer, SkinBlending};
pub use scene::{ColorScheme, FieldGeometry, Preview, RenderOptions, Skin, field_span};
pub use sprite::{Instance, Shape, SpritePipeline, Symbol, SymbolMode};
pub use text::{Align, TextItem, TextPipeline};
pub use text_atlas::RunRasterizer;
pub use texture_cache::{TextureId, TextureKey};
