//! StepMania's actor model, shared by noteskins and, later, our screens
//! and themes (`docs/plans/ui-core.md`, P1). Actors as StepMania runs
//! them: the tween queue, effects on their clocks, sprite states, commands,
//! and what is drawn (`src/Actor.cpp`, `src/Tween.cpp`, `src/Sprite.cpp`,
//! `src/ActorFrame.cpp`, `Themes/_fallback/Scripts/02 Actor.lua`,
//! 5_1-new). Positions are in StepMania pixels, a lane being 64; rotations
//! in degrees, positive clockwise on screen (y grows downwards).
//!
//! Commands as metrics write them are parsed here ([`commands`]); a
//! model's meshes, bones and animated textures are [`model`]'s. Pure: the
//! files that build actors (noteskins' metrics, Lua, MilkShape models) are
//! read by `ddi_noteskin`, and what is drawn is `ddi_render`'s.

mod actor;
pub mod commands;
pub mod model;

pub use actor::*;
