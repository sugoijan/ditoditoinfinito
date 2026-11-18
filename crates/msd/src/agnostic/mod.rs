//! Hand-agnostic data, sequencing and pattern mods (upstream's
//! `Agnostic/`): they look at whole rows, whichever hand.

pub mod ha_pattern_mods;
pub mod ha_sequencers;
pub mod ha_sequencing;
pub mod interval_info;
pub mod meta_interval_info;
pub mod meta_row_info;
