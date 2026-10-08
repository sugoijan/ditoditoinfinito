//! Lyrics of Dancing☆Onigiri works (`word_data`), shown as danoniplus
//! shows them (`updateWord`, `js/lib/mainWindow.js` 1365–1450): one line
//! per depth, even depths at the top edge of the field and odd ones at the
//! bottom. A text cue replaces its line; `[fadein]` and `[fadeout]` fade it
//! over a number of frames; `[left]`, `[center]` and `[right]` align it;
//! `[fontSize=n]` sizes it.
//!
//! The importer turns `word_data` into `danoni:lyric` effect events
//! (`[depth, text, fade frames]`, timed in seconds, layer = chart).

use std::rc::Rc;

use ddi_chart::{EffectTime, Song};

/// Effect kind of a lyric cue.
pub const LYRIC_EVENT: &str = "danoni:lyric";

/// danoniplus's lyric size, px, on its 500 px tall field (`g_limitObj.mainSiz`).
pub const DEFAULT_SIZE: f32 = 14.0;

/// danoniplus's field height the sizes refer to, px.
pub const FIELD_HEIGHT: f32 = 500.0;

/// Default fade, seconds (`C_WOD_FRAME`, 30 frames).
const DEFAULT_FADE: f64 = 0.5;

/// Largest font size taken from a chart, px on the 500 px field.
const MAX_SIZE: f32 = 200.0;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LyricAlign {
    /// danoniplus's default (`lblWord`).
    #[default]
    Left,
    Center,
    Right,
}

/// One lyric line as it looks at some instant.
#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub depth: u8,
    /// At the top edge of the field, else at the bottom.
    pub top: bool,
    /// Plain text; `\n` breaks lines.
    pub text: Rc<str>,
    pub align: LyricAlign,
    /// Font size in px of danoniplus's 500 px tall field ([`FIELD_HEIGHT`]).
    pub size: f32,
    pub opacity: f32,
}

#[derive(Clone, Debug, PartialEq)]
enum Cue {
    Text(Rc<str>),
    Fade { fade_in: bool, seconds: f64 },
    Align(LyricAlign),
    Size(f32),
}

impl Cue {
    fn parse(text: &str, fade_frames: Option<&str>) -> Cue {
        let fade = || {
            fade_frames
                .and_then(|f| f.trim().parse::<f64>().ok())
                .filter(|f| f.is_finite() && *f >= 0.0)
                .map_or(DEFAULT_FADE, |f| f / 60.0)
        };
        match text {
            "[fadein]" => Cue::Fade {
                fade_in: true,
                seconds: fade(),
            },
            "[fadeout]" => Cue::Fade {
                fade_in: false,
                seconds: fade(),
            },
            "[left]" => Cue::Align(LyricAlign::Left),
            "[center]" => Cue::Align(LyricAlign::Center),
            "[right]" => Cue::Align(LyricAlign::Right),
            _ => match text
                .strip_prefix("[fontSize=")
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|n| n.parse::<u32>().ok())
            {
                Some(n) => Cue::Size((n as f32).min(MAX_SIZE)),
                None => Cue::Text(Rc::from(text)),
            },
        }
    }
}

/// A line's state after a cue (cheap to copy: the text is shared).
#[derive(Clone, Debug, PartialEq)]
struct State {
    text: Rc<str>,
    align: LyricAlign,
    size: f32,
    /// `(start, seconds, fade_in)` of the fade in effect.
    fade: Option<(f64, f64, bool)>,
}

impl State {
    fn opacity(&self, t: f64) -> f32 {
        match self.fade {
            None => 1.0,
            Some((start, seconds, fade_in)) => {
                let p = if seconds <= 0.0 {
                    1.0
                } else {
                    ((t - start) / seconds).clamp(0.0, 1.0)
                };
                (if fade_in { p } else { 1.0 - p }) as f32
            }
        }
    }
}

/// The lyrics of one chart, ready to be looked up by song time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LyricTrack {
    /// Per depth, the state after each of its cues, by time.
    lines: Vec<Vec<(f64, State)>>,
    /// Swap top and bottom (Reverse).
    flip: bool,
}

impl LyricTrack {
    /// The lyrics of `song.charts[chart]`. Under Reverse, danoniplus moves
    /// the lines to the other edge on one-row key modes (`makeWordData`,
    /// `dataLoader.js` 1045–1078; `wordAutoReverse` overrides, and lyrics
    /// with line breaks never move).
    pub fn new(song: &Song, chart: usize, reverse: bool, single_row: bool) -> LyricTrack {
        let mut cues: Vec<(f64, u8, Cue)> = song
            .effects
            .iter()
            .filter(|e| e.kind == LYRIC_EVENT && usize::from(e.layer) == chart)
            .filter_map(|e| {
                let EffectTime::Seconds(at) = e.at else {
                    return None;
                };
                let depth = e.fields.first()?.parse::<u8>().ok()?;
                let text = e.fields.get(1)?;
                Some((
                    at,
                    depth,
                    Cue::parse(text, e.fields.get(2).map(String::as_str)),
                ))
            })
            .filter(|(at, ..)| at.is_finite())
            .collect();
        // Stable: cues of one instant keep their order.
        cues.sort_by(|a, b| a.0.total_cmp(&b.0));

        let auto = song
            .charts
            .get(chart)
            .and_then(|c| c.danoni.as_ref())
            .and_then(|d| {
                d.headers
                    .iter()
                    .rev()
                    .find(|(k, _)| k == "wordAutoReverse")
                    .map(|(_, v)| v.trim().to_ascii_uppercase())
            });
        let breaks = cues
            .iter()
            .any(|(_, _, c)| matches!(c, Cue::Text(t) if t.contains('\n')));
        let flip = reverse
            && match auto.as_deref() {
                Some("ON") => true,
                Some("OFF") => false,
                _ => single_row && !breaks,
            };

        let mut lines: Vec<Vec<(f64, State)>> = Vec::new();
        let mut current: Vec<State> = Vec::new();
        for (at, depth, cue) in cues {
            let d = usize::from(depth);
            if current.len() <= d {
                current.resize(
                    d + 1,
                    State {
                        text: Rc::from(""),
                        align: LyricAlign::Left,
                        size: DEFAULT_SIZE,
                        fade: None,
                    },
                );
                lines.resize(d + 1, Vec::new());
            }
            let s = &mut current[d];
            match cue {
                Cue::Text(text) => {
                    // A finished fade is cleared, so the new text shows.
                    if s.fade
                        .is_some_and(|(start, seconds, _)| at - start >= seconds)
                    {
                        s.fade = None;
                    }
                    s.text = text;
                }
                Cue::Fade { fade_in, seconds } => s.fade = Some((at, seconds, fade_in)),
                Cue::Align(a) => s.align = a,
                Cue::Size(px) => s.size = px,
            }
            lines[d].push((at, s.clone()));
        }
        LyricTrack { lines, flip }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(Vec::is_empty)
    }

    /// The lines at song second `t` (those with text so far).
    pub fn at(&self, t: f64) -> Vec<LyricLine> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(depth, states)| {
                let i = states.partition_point(|(at, _)| *at <= t);
                let (_, s) = states.get(i.checked_sub(1)?)?;
                if s.text.is_empty() {
                    return None;
                }
                Some(LyricLine {
                    depth: depth as u8,
                    top: (depth % 2 == 0) != self.flip,
                    text: s.text.clone(),
                    align: s.align,
                    size: s.size,
                    opacity: s.opacity(t),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::danoni::{self, Dos, DosFlags};

    /// A work whose frame `F` sounds at `(F − 140) / 60` s.
    fn song(words: &str) -> Song {
        let text = format!("|difData=5,N|blankFrame=140|left_data=300|word_data={words}|");
        let i = danoni::import(&Dos::parse(&text, DosFlags::default())).unwrap();
        i.songs.into_iter().next().unwrap()
    }

    #[test]
    fn text_fades_and_alignment() {
        let s = song(
            "200,0,hello,200,0,[center]\n260,1,below\n320,0,[fadeout],60\n\
             350,0,still fading\n440,0,again\n500,0,[fontSize=20]",
        );
        let track = LyricTrack::new(&s, 0, false, true);
        assert!(track.at(0.5).is_empty());
        let l = track.at(1.5);
        assert_eq!(l.len(), 1);
        assert_eq!(
            (&*l[0].text, l[0].align, l[0].top),
            ("hello", LyricAlign::Center, true)
        );
        let l = track.at(2.5);
        assert_eq!(l.len(), 2);
        assert!(!l[1].top);
        // Halfway through a one-second fade-out, a new text keeps fading.
        let l = track.at(3.75);
        assert_eq!(&*l[0].text, "still fading");
        assert!((l[0].opacity - 0.25).abs() < 1e-3);
        assert_eq!(track.at(4.5)[0].opacity, 0.0);
        // After the fade, a new text shows again.
        assert_eq!(track.at(5.5)[0].opacity, 1.0);
        assert_eq!(track.at(6.5)[0].size, 20.0);
        assert_eq!(track.at(6.5)[0].align, LyricAlign::Center);
    }

    #[test]
    fn reverse_moves_lines_on_one_row_modes() {
        let s = song("200,0,a");
        assert!(!LyricTrack::new(&s, 0, true, true).at(2.0)[0].top);
        assert!(LyricTrack::new(&s, 0, true, false).at(2.0)[0].top);
        assert!(LyricTrack::new(&s, 0, false, true).at(2.0)[0].top);
        let s = song("200,0,a<br>b");
        assert!(LyricTrack::new(&s, 0, true, true).at(2.0)[0].top);
        // Other charts' lyrics are not this chart's.
        assert!(LyricTrack::new(&s, 1, false, true).is_empty());
    }
}
