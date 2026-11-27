//! Where a play's song came from, when a song source found it
//! (`docs/plans/source-plugins.md`, "Plays of found songs"; desktop's
//! plugins and This computer): a small note on the play ([`PlayOrigin`],
//! [`super::PlayRecord::origin`], copied into its summary) so that the
//! game can list the songs played from a source while they are not
//! loaded ([`super::History::played_before`]) and fetch one again.
//!
//! It says only what the source told the game already (its id, the
//! item's id and revision, which of its songs) and what a list needs
//! without the song: the title, the artist and the chart played, as data.
//! Bounded ([`MAX_ORIGIN_ID`], [`MAX_ORIGIN_TEXT`]); an origin that is not
//! is read as none. It stays on the device: nothing the game sends
//! carries a play.
//!
//! The web never makes one, and keeps one it reads (a play imported from
//! desktop's export) untouched: the record's text is kept as it came, its
//! summary made from it with the origin in it.

use std::collections::HashMap;

use ddi_chart::{ChartDigest, Difficulty};
use ddi_engine::rules::ScoreView;
use serde::{Deserialize, Serialize};

use super::{History, Lamp};

/// The longest id an origin keeps (a source's, an item's, a revision),
/// bytes: a song source's own bound (`ddi_source_protocol::limits::
/// MAX_ID_BYTES`). An origin with a longer one is none: an id cut short
/// would name another item.
pub const MAX_ORIGIN_ID: usize = 256;
/// The longest text an origin keeps (a title, an artist, a chart's name
/// or style), bytes: cut there at a character's edge.
pub const MAX_ORIGIN_TEXT: usize = 256;
/// The most songs an item has (`ddi_source_protocol::limits::MAX_SONGS`).
pub const MAX_ORIGIN_SONGS: u32 = 256;

/// Where a play's song came from: a song source's item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayOrigin {
    /// The source: a plugin's id, or `this-computer`.
    pub source: String,
    /// The item, by the id its source gave it.
    pub item: String,
    /// The item's revision as its source stated it (another revision is
    /// another item for the game's memory, `src:` ids included).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Which song of the item, from 0 (its id's last part).
    #[serde(default)]
    pub song: u32,
    /// The song's title and artist as its simfile says (data).
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    /// The chart played.
    #[serde(default)]
    pub chart: OriginChart,
}

/// The chart a play of a found song was of, as a list names it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OriginChart {
    /// Its own name (`#CHARTNAME`, else its description; data).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Its style, a layout's id (`dance-single`).
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub difficulty: Difficulty,
    #[serde(default)]
    pub meter: u32,
}

/// `text` cut to [`MAX_ORIGIN_TEXT`] bytes at a character's edge.
fn cut(mut text: String) -> String {
    if text.len() > MAX_ORIGIN_TEXT {
        let mut at = MAX_ORIGIN_TEXT;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
    }
    text
}

impl PlayOrigin {
    /// Within the bounds: its ids as they are (none where one is longer
    /// than [`MAX_ORIGIN_ID`] or empty, or its song past
    /// [`MAX_ORIGIN_SONGS`]), its texts cut.
    pub fn bounded(self) -> Option<PlayOrigin> {
        let id_ok = |s: &str| !s.is_empty() && s.len() <= MAX_ORIGIN_ID;
        if !id_ok(&self.source)
            || !id_ok(&self.item)
            || self
                .revision
                .as_deref()
                .is_some_and(|r| r.len() > MAX_ORIGIN_ID)
            || self.song >= MAX_ORIGIN_SONGS
        {
            return None;
        }
        Some(PlayOrigin {
            title: cut(self.title),
            artist: cut(self.artist),
            chart: OriginChart {
                name: cut(self.chart.name),
                style: cut(self.chart.style),
                ..self.chart
            },
            ..self
        })
    }
}

/// A field that does not read, or reads out of bounds, is none.
pub(super) fn lenient_origin<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<PlayOrigin>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value::<PlayOrigin>(v)
        .ok()
        .and_then(PlayOrigin::bounded))
}

/// A song played from a source ([`History::played_before`]): the latest
/// play's origin and chart, how often and when it was last played, and
/// that chart's best.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayedBefore {
    /// The latest play's: the item's revision then, the chart played.
    pub origin: PlayOrigin,
    /// The latest play's chart, by content.
    pub digest: ChartDigest,
    /// Milliseconds since the Unix epoch.
    pub last_played: f64,
    /// The plays kept of it (of any of its charts, autoplayed ones too).
    pub plays: u32,
    /// The latest play's chart's best under the rules it was last played
    /// by, as song select's chart row has it (its plays of any song id:
    /// a library copy's too).
    pub best: Option<PlayedBest>,
}

/// A best as a list writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayedBest {
    pub score: ScoreView,
    pub grade: String,
    pub lamp: Lamp,
}

impl History {
    /// The sources plays were kept from (by the origins), in their ids'
    /// order.
    pub fn origin_sources(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .iter()
            .filter_map(|(_, s)| s.origin.as_ref().map(|o| o.source.clone()))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The songs played from source `source`, a row each (an item's song,
    /// whatever its revision), the most recently played first: at most
    /// `max`, and how many there are in all. From the origins alone: no
    /// song is read and no source asked.
    pub fn played_before(&self, source: &str, max: usize) -> (Vec<PlayedBefore>, usize) {
        // Oldest first: the latest play of each song comes last.
        let mut songs: HashMap<(&str, u32), (&str, u32)> = HashMap::new();
        for (id, s) in self.iter() {
            let Some(o) = s.origin.as_ref().filter(|o| o.source == source) else {
                continue;
            };
            let e = songs.entry((o.item.as_str(), o.song)).or_insert((id, 0));
            e.0 = id;
            e.1 += 1;
        }
        let total = songs.len();
        let mut latest: Vec<(&str, u32)> = songs.into_values().collect();
        // Play ids begin with their time: the newest first.
        latest.sort_by(|a, b| b.0.cmp(a.0));
        latest.truncate(max);
        let rows = latest
            .into_iter()
            .filter_map(|(id, plays)| {
                let s = self.get(id)?;
                let (ruleset, worked, _) = s.view(None);
                let assist = worked.is_some_and(|w| w.assist);
                let best = self
                    .best(s.chart.digest, ruleset, assist)
                    .map(|b| PlayedBest {
                        score: b.score,
                        grade: b.grade,
                        lamp: b.lamp,
                    });
                Some(PlayedBefore {
                    origin: s.origin.clone()?,
                    digest: s.chart.digest,
                    last_played: s.time,
                    plays,
                    best,
                })
            })
            .collect();
        (rows, total)
    }
}
