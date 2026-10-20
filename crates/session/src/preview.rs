//! Song select's previews, as both shells play them: a simfile's
//! `#SAMPLELENGTH` (12 s when it gives none, 30 s at most), faded in and
//! out.

/// Preview length when the simfile gives none, and the longest one played.
pub const DEFAULT_LENGTH: f64 = 12.0;
pub const MAX_LENGTH: f64 = 30.0;
/// The fades in and out, seconds.
pub const FADE: f64 = 0.5;
/// After the start is asked for, when it starts (context seconds).
pub const LEAD: f64 = 0.05;

/// How long a preview plays, from the simfile's `#SAMPLELENGTH`.
pub fn length(sample_length: f64) -> f64 {
    if sample_length > 1.0 {
        sample_length.min(MAX_LENGTH)
    } else {
        DEFAULT_LENGTH
    }
}
