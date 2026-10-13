//! Source ratings and native motion, independent of the player's settings.
use crate::{Error, Result, model::*};
use serde_json::Value;

fn numeric_rating(value: &Value) -> Result<Option<u16>> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_u64()
        .filter(|n| *n <= 999)
        .map(|n| Some(n as u16))
        .ok_or_else(|| {
            Error::new(
                "invalid-rating",
                "Rating must be an integer from 0 to 999, or null",
            )
        })
}
pub fn source_stars(chart: &Chart, metadata: &Metadata) -> Result<Option<u16>> {
    if let Some(value) = chart
        .source
        .get("difficultyStars")
        .or_else(|| metadata.get("difficultyStars"))
        && let Some(stars) = numeric_rating(value)?
    {
        return Ok(Some(stars));
    }
    let raw = metadata
        .get("rawDifficulty")
        .or_else(|| metadata.get("difficulty"))
        .and_then(Value::as_str);
    match raw {
        Some(raw) if !raw.is_empty() && raw.chars().all(|c| c == '★') => {
            let count = raw.chars().count();
            if count > 999 {
                return Err(Error::new("invalid-rating", "Star count exceeds 999"));
            }
            Ok(Some(count as u16))
        }
        _ => Ok(None),
    }
}
pub fn tier_for_stars(stars: Option<u16>, bounds: [u16; 4]) -> Result<Tier> {
    if bounds.windows(2).any(|w| w[0] >= w[1]) {
        return Err(Error::new(
            "invalid-tier-bounds",
            "Tier bounds must be four increasing nonnegative integers",
        ));
    }
    let Some(stars) = stars else {
        return Ok(Tier::Edit);
    };
    if stars > 999 {
        return Err(Error::new("invalid-rating", "Star count exceeds 999"));
    }
    Ok([Tier::Beginner, Tier::Easy, Tier::Medium, Tier::Hard]
        .into_iter()
        .zip(bounds)
        .find_map(|(tier, maximum)| (stars <= maximum).then_some(tier))
        .unwrap_or(Tier::Challenge))
}
pub fn base_ratio(motion: Option<&NativeMotion>, bpm: f64, factor: f64) -> Result<f64> {
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err(Error::new(
            "invalid-bpm",
            "Timing BPM must be finite and positive",
        ));
    }
    if !factor.is_finite() || !(0.0 < factor && factor <= 100.0) {
        return Err(Error::new(
            "invalid-scroll-factor",
            "Scroll factor must be finite, positive and at most 100",
        ));
    }
    let Some(motion) = motion else {
        return if factor == 1.0 {
            Ok(1.0)
        } else {
            Err(Error::new(
                "missing-native-motion",
                "A scroll correction requires native motion evidence",
            ))
        };
    };
    let rate = motion.base_arrow_heights_per_second;
    if !rate.is_finite() || !(0.0 < rate && rate <= 1000.0) {
        return Err(Error::new(
            "invalid-native-motion",
            "Native velocity must be positive and at most 1000 arrow heights/second",
        ));
    }
    let ratio = rate * factor * 60.0 / bpm;
    if !ratio.is_finite() {
        return Err(Error::new("invalid-scroll-ratio", "Nonfinite scroll ratio"));
    }
    Ok(ratio)
}
