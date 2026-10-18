//! Pickup preview selection from supplied PCM16; no decoding or media mutation.
use crate::{Error, Result, model::Chart};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PreviewSettings {
    pub seconds: f64,
    pub quiet_ratio: f64,
    pub minimum_notes: usize,
    pub maximum_gap_seconds: f64,
    pub note_window_seconds: f64,
    pub lead_in_seconds: f64,
}
impl Default for PreviewSettings {
    fn default() -> Self {
        Self {
            seconds: 12.0,
            quiet_ratio: 0.35,
            minimum_notes: 4,
            maximum_gap_seconds: 1.0,
            note_window_seconds: 3.0,
            lead_in_seconds: 0.08,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PcmEnvelope {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: usize,
    pub energy: Vec<f64>,
    pub step_seconds: f64,
    pub duration_seconds: f64,
}
fn invalid(message: &str) -> Error {
    Error::new("invalid-pcm", message)
}
fn u16le(body: &[u8]) -> u16 {
    u16::from_le_bytes(body[..2].try_into().unwrap())
}
fn u32le(body: &[u8]) -> u32 {
    u32::from_le_bytes(body[..4].try_into().unwrap())
}

pub fn pcm_envelope(
    wav: &[u8],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<PcmEnvelope> {
    if wav.len() < 12
        || &wav[..4] != b"RIFF"
        || &wav[8..12] != b"WAVE"
        || u32le(&wav[4..]) as usize + 8 != wav.len()
    {
        return Err(invalid("Need a complete RIFF/WAVE"));
    }
    let (mut format, mut data) = (None, None);
    let mut at = 12;
    while at < wav.len() {
        if wav.len() - at < 8 {
            return Err(invalid("Truncated WAV chunk"));
        }
        let size = u32le(&wav[at + 4..]) as usize;
        let end = at + 8 + size;
        if end > wav.len() {
            return Err(invalid("WAV chunk escapes input"));
        }
        match &wav[at..at + 4] {
            b"fmt " => {
                if format.is_some() || size < 16 {
                    return Err(invalid("Invalid/duplicate WAV format"));
                }
                format = Some(&wav[at + 8..end]);
            }
            b"data" => {
                if data.is_some() {
                    return Err(invalid("Multiple PCM data chunks"));
                }
                data = Some(&wav[at + 8..end]);
            }
            _ => {}
        }
        at = end + size % 2;
    }
    if at != wav.len() {
        return Err(invalid("Missing WAV chunk padding"));
    }
    let format = format.ok_or_else(|| invalid("Missing PCM format"))?;
    let data = data.ok_or_else(|| invalid("Missing PCM samples"))?;
    let channels = u16le(&format[2..]);
    let rate = u32le(&format[4..]);
    let align = u16le(&format[12..]);
    if u16le(format) != 1
        || ![1, 2].contains(&channels)
        || !(1..=192000).contains(&rate)
        || align != channels * 2
        || u16le(&format[14..]) != 16
        || u32le(&format[8..]) != rate * u32::from(align)
        || data.len() % usize::from(align) != 0
    {
        return Err(invalid("Need bounded mono/stereo PCM16 at a valid rate"));
    }
    let frames = data.len() / usize::from(align);
    if frames == 0 || frames as u64 > u64::from(rate) * 600 {
        return Err(invalid("Need nonempty music of at most ten minutes"));
    }
    let step = (f64::from(rate) * 0.1).round_ties_even().max(1.0) as usize;
    let mut energy = Vec::new();
    for (i, chunk) in data.chunks(step * usize::from(align)).enumerate() {
        if i % 100 == 0 {
            checkpoint()?;
        }
        let samples = chunk.as_chunks::<2>().0;
        let sum: u64 = samples
            .iter()
            .map(|v| {
                let v = i64::from(i16::from_le_bytes(*v));
                (v * v) as u64
            })
            .sum();
        energy.push((sum as f64 / samples.len() as f64).sqrt() / 32768.0);
    }
    Ok(PcmEnvelope {
        sample_rate: rate,
        channels,
        frames,
        energy,
        step_seconds: step as f64 / f64::from(rate),
        duration_seconds: frames as f64 / f64::from(rate),
    })
}
fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = (sorted.len() - 1) as f64 * fraction;
    let low = index.floor() as usize;
    sorted[low] + (sorted[(low + 1).min(sorted.len() - 1)] - sorted[low]) * (index - low as f64)
}
impl PcmEnvelope {
    fn rms(&self, start: f64, end: f64) -> f64 {
        let start = start.max(0.0);
        let end = end.min(self.duration_seconds);
        if end <= start {
            return 0.0;
        }
        let (mut weighted, mut weight) = (0.0, 0.0);
        let first = (start / self.step_seconds) as usize;
        let last = ((end / self.step_seconds).ceil() as usize).min(self.energy.len());
        for i in first..last {
            let length = (end.min((i + 1) as f64 * self.step_seconds)
                - start.max(i as f64 * self.step_seconds))
            .max(0.0);
            weighted += self.energy[i].powi(2) * length;
            weight += length;
        }
        if weight > 0.0 {
            (weighted / weight).sqrt()
        } else {
            0.0
        }
    }
    fn support(&self, t: f64, threshold: f64) -> Value {
        let start = self.rms(t, t + 0.15);
        let chunks = (0..3)
            .filter(|i| t + *i as f64 * 0.5 < self.duration_seconds)
            .map(|i| self.rms(t + i as f64 * 0.5, t + (i + 1) as f64 * 0.5))
            .collect::<Vec<_>>();
        let strong = chunks.iter().filter(|v| **v >= threshold).count();
        json!({"startRms":start,"sustainRms":self.rms(t,t+1.5),"strongHalfSecondWindows":strong,"availableHalfSecondWindows":chunks.len(),"supported":start>=threshold && strong>=2.min(chunks.len())})
    }
}
fn density(t: f64, times: &[f64], settings: &PreviewSettings) -> Value {
    let a = times.partition_point(|v| *v < t - 1e-9);
    let b = times.partition_point(|v| *v <= t + settings.note_window_seconds + 1e-9);
    let count = b - a;
    let first = (count > 0).then(|| times[a] - t);
    let next = (count >= 2).then(|| times[a + 1] - times[a]);
    json!({"notes":count,"firstNoteGapSeconds":first,"nextNoteGapSeconds":next,"supported":count>=settings.minimum_notes && first.is_some_and(|g| g<=settings.maximum_gap_seconds+1e-9) && next.is_some_and(|g| g<=settings.maximum_gap_seconds+1e-9)})
}
fn supported(v: &Value) -> bool {
    v["supported"].as_bool().unwrap_or(false)
}

pub fn choose_preview(
    audio: &PcmEnvelope,
    charts: &[&Chart],
    settings: &PreviewSettings,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    let s = settings;
    if [
        s.seconds,
        s.quiet_ratio,
        s.maximum_gap_seconds,
        s.note_window_seconds,
        s.lead_in_seconds,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || !(1.0..=30.0).contains(&s.seconds)
        || !(0.0 < s.quiet_ratio && s.quiet_ratio <= 1.0)
        || !(0.1..=10.0).contains(&s.maximum_gap_seconds)
        || !(0.5..=20.0).contains(&s.note_window_seconds)
        || !(0.0..=1.0).contains(&s.lead_in_seconds)
        || !(2..=64).contains(&s.minimum_notes)
    {
        return Err(Error::new(
            "invalid-preview-settings",
            "Unsupported preview settings",
        ));
    }
    // PcmEnvelope is public: validate caller-created envelopes as well as WAVs.
    let valid_rate = (1..=192000).contains(&audio.sample_rate);
    let step_frames = (f64::from(audio.sample_rate) * 0.1)
        .round_ties_even()
        .max(1.0) as usize;
    if !valid_rate
        || ![1, 2].contains(&audio.channels)
        || audio.frames == 0
        || audio.frames as u64 > u64::from(audio.sample_rate) * 600
        || audio.energy.len() != audio.frames.div_ceil(step_frames)
        || audio
            .energy
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        || !audio.step_seconds.is_finite()
        || (audio.step_seconds - step_frames as f64 / f64::from(audio.sample_rate)).abs() > 1e-12
        || !audio.duration_seconds.is_finite()
        || (audio.duration_seconds - audio.frames as f64 / f64::from(audio.sample_rate)).abs()
            > 1e-12
    {
        return Err(invalid("Invalid audio envelope"));
    }
    let audible = audio
        .energy
        .iter()
        .copied()
        .filter(|e| *e >= 1.0 / 32768.0)
        .collect::<Vec<_>>();
    let typical = if audible.is_empty() {
        0.0
    } else {
        percentile(&audible, 0.75)
    };
    let threshold = (typical * s.quiet_ratio).max(1.0 / 32768.0);
    let settings = json!({"policy":"pickup","audioBinSeconds":audio.step_seconds,"typicalRmsPercentile":0.75,"quietRatio":s.quiet_ratio,"minimumNotes":s.minimum_notes,"maximumGapSeconds":s.maximum_gap_seconds,"noteWindowSeconds":s.note_window_seconds,"leadInSeconds":s.lead_in_seconds,"startAudioWindowSeconds":0.15,"sustainAudioWindowSeconds":1.5,"requiredSustainHalfSeconds":2});
    if audible.is_empty() {
        return Ok(
            json!({"startSeconds":0,"lengthSeconds":s.seconds.min(audio.duration_seconds),"status":"no-activity","thresholdRms":threshold,"typicalRms":typical,"settings":settings,"method":"No audio above the PCM activity floor"}),
        );
    }
    let mut tracks = Vec::new();
    for chart in charts {
        let mut times = chart
            .lanes
            .iter()
            .flat_map(|l| {
                l.taps_seconds
                    .iter()
                    .copied()
                    .chain(l.holds_seconds.iter().map(|h| h[0]))
            })
            .collect::<Vec<_>>();
        if times.iter().any(|t| !t.is_finite()) {
            return Err(Error::new("invalid-time", "Invalid preview head"));
        }
        times.retain(|t| 0.0 <= *t && *t < audio.duration_seconds);
        times.sort_by(f64::total_cmp);
        times.dedup();
        tracks.push((chart.id.clone(), times));
    }
    let mut informative = Vec::new();
    for (i, (_, times)) in tracks.iter().enumerate() {
        checkpoint()?;
        if times.iter().any(|t| supported(&density(*t, times, s))) {
            informative.push(i);
        }
    }
    let required = if informative.is_empty() {
        0
    } else {
        informative.len() / 2 + 1
    };
    let mut candidates = tracks
        .iter()
        .flat_map(|(_, ts)| ts.iter().copied())
        .collect::<Vec<_>>();
    candidates.sort_by(f64::total_cmp);
    candidates.dedup();
    let (mut trace, mut prefix) = (Vec::new(), vec![(0, 0)]);
    let (mut quiet, mut sparse) = (0, 0);
    let (mut selected, mut fallback) = (None, None);
    for (i, t) in candidates.iter().copied().enumerate() {
        if i % 128 == 0 {
            checkpoint()?;
        }
        let sound = audio.support(t, threshold);
        let support = informative
            .iter()
            .map(|i| {
                let (id, ts) = &tracks[*i];
                (id.clone(), density(t, ts, s))
            })
            .collect::<serde_json::Map<_, _>>();
        if supported(&sound)
            && required > 0
            && support.values().filter(|v| supported(v)).count() >= required
        {
            selected = Some((t, sound, Value::Object(support), "selected"));
            break;
        }
        if supported(&sound) && fallback.is_none() {
            fallback = Some((
                t,
                sound.clone(),
                Value::Object(support.clone()),
                "audio-note-fallback",
            ));
        }
        let reason = if supported(&sound) {
            sparse += 1;
            "isolated-or-sparse-notes"
        } else {
            quiet += 1;
            "quiet-or-unsustained-audio"
        };
        prefix.push((quiet, sparse));
        if trace.len() < 16 {
            trace.push(
                json!({"noteSeconds":t,"reason":reason,"audio":sound,"chartSupport":support}),
            );
        }
    }
    let scanned = quiet + sparse;
    let (anchor, sound, chart_support, status) = if let Some(value) = selected.or(fallback) {
        value
    } else {
        let mut sustained = None;
        for (i, e) in audio.energy.iter().enumerate() {
            if i % 128 == 0 {
                checkpoint()?;
            }
            let t = i as f64 * audio.step_seconds;
            if *e >= threshold && supported(&audio.support(t, threshold)) {
                sustained = Some(t);
                break;
            }
        }
        let (t, status) = match sustained {
            Some(t) => (t, "audio-only-fallback"),
            None => (
                audio.energy.iter().position(|e| *e >= threshold).unwrap() as f64
                    * audio.step_seconds,
                "short-audio-fallback",
            ),
        };
        (t, audio.support(t, threshold), Value::Null, status)
    };
    if status != "selected" {
        (quiet, sparse) = prefix[candidates.partition_point(|t| *t < anchor)];
        trace.retain(|v| v["noteSeconds"].as_f64().unwrap() < anchor);
    }
    let start = (anchor - s.lead_in_seconds).max(0.0);
    let length = s.seconds.min(audio.duration_seconds - start);
    let bins = &audio.energy[(start / audio.step_seconds) as usize
        ..(((start + length) / audio.step_seconds).ceil() as usize).min(audio.energy.len())];
    let first =
        audio.energy.iter().position(|e| *e >= threshold).unwrap() as f64 * audio.step_seconds;
    Ok(
        json!({"startSeconds":start,"lengthSeconds":length,"status":status,"anchorSeconds":anchor,"anchorType":if ["selected","audio-note-fallback"].contains(&status){"note"}else{"audio"},"firstNoteSeconds":candidates.first(),"firstActiveSeconds":first,"thresholdRms":threshold,"typicalRms":typical,"activeFraction":if bins.is_empty(){0.0}else{bins.iter().filter(|e|**e>=threshold).count()as f64/bins.len()as f64},"settings":settings,"chartIds":tracks.iter().map(|(id,_)|id).collect::<Vec<_>>(),"informativeChartIds":informative.iter().map(|i|&tracks[*i].0).collect::<Vec<_>>(),"sparseChartIds":tracks.iter().enumerate().filter(|(i,_)|!informative.contains(i)).map(|(_,v)|&v.0).collect::<Vec<_>>(),"requiredChartSupport":required,"selectedAudioSupport":sound,"selectedChartSupport":chart_support,"skippedCandidates":quiet+sparse,"rejectedJointCandidates":scanned,"quietCandidates":quiet,"sparseCandidates":sparse,"firstSkippedCandidates":trace,"method":"Earliest sustained music and nearby distinct chart heads; strict majority of informative charts; bounded lead-in; no audio cropping"}),
    )
}
