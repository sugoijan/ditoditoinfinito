//! State the screens need, out of the web app (`docs/plans/ui-core.md`,
//! P12): plain data, kept through [`ddi_platform::storage`] under the
//! web's keys, so both shells read and write the same values.

pub mod battle_plan;
pub mod bind;
pub mod cpu_roster;
pub mod editor;
pub mod history;
pub mod online;
pub mod place;
pub mod room;
pub mod select_view;
pub mod settings;
