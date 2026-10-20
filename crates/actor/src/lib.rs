//! StepMania's actor model, shared by noteskins and, later, our screens
//! and themes (`docs/plans/ui-core.md`, P1). Actors as StepMania runs
//! them: the tween queue, effects on their clocks, sprite states, commands,
//! and what is drawn (`src/Actor.cpp`, `src/Tween.cpp`, `src/Sprite.cpp`,
//! `src/ActorFrame.cpp`, `Themes/_fallback/Scripts/02 Actor.lua`,
//! 5_1-new). Positions are in StepMania pixels, a lane being 64; rotations
//! in degrees, positive clockwise on screen (y grows downwards).
//!
//! Commands as metrics write them are parsed here ([`commands`]); a
//! model's meshes, bones and animated textures are [`model`]'s. Besides
//! StepMania's actors: runs of text ([`Kind::Text`], measured through
//! [`text::TextMeasure`]), clipping frames ([`Actor::clip`]), and the
//! virtual screen they are placed on ([`screen`]). Pure: the files that
//! build actors (noteskins' metrics, Lua, MilkShape models) are read by
//! `ddi_noteskin`, and what is drawn ([`Drawn`]) is `ddi_render`'s.
//!
//! Actors are held as a tree: a frame owns its children, a screen or a
//! skin owns its roots. Lua reaches them by handle through a map of
//! handles to places in the tree (`ddi_noteskin`'s stage), kept current
//! when a frame sorts its children.

mod actor;
pub mod background;
pub mod commands;
pub mod model;
pub mod screen;
pub mod text;

pub use actor::*;
