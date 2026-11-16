//! `announcer tidy`: sounds tidied by the game's own code
//! (`ddi_library::announcer::tidy`, the announcer studio's: the ends cut
//! and faded, the level evened by its compressor and limiter), a whole
//! folder in one run. `tools/announcer-design/kokoro.py` makes our packs' sounds
//! through it, so that the studio and our packs have one leveler
//! (`docs/plans/announcers.md`, "Louder packs"). WAVs in (16-bit PCM or
//! 32-bit float), 32-bit float WAVs out under the same names, and a line
//! of JSON for each on standard output: what was cut, the gain, what the
//! compressor and the limiter took and how the sound measures before and
//! after.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use ddi_library::announcer::tidy;
use serde::Serialize;

#[derive(Subcommand)]
pub enum Action {
    /// Apply the studio's stadium effect and final leveling to prepared speech.
    Stadium {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Preserve the speech duration for ready/restart/here-we-go.
        #[arg(long)]
        opening: bool,
    },
    /// Tidy every .wav of a folder into another, as the announcer studio
    /// tidies a take.
    Tidy {
        /// The folder of WAVs to tidy (16-bit PCM or 32-bit float).
        #[arg(long)]
        dir: PathBuf,
        /// Where the tidied WAVs go (32-bit float), under the same names.
        #[arg(long)]
        out: PathBuf,
        /// The voiced RMS to level to, in dBFS, in place of the game's
        /// (an audition's).
        #[arg(long, allow_hyphen_values = true)]
        level: Option<f64>,
    },
    /// Restore the level after creative effects without compressing or trimming again.
    Finish {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// JSON object mapping each WAV filename to its original speech duration in seconds.
        #[arg(long)]
        bodies: PathBuf,
    },
}

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Stadium {
            input,
            out,
            opening,
        } => {
            let (audio, rate) = read_wav(&fs::read(input)?)?;
            let body = audio.first().map_or(0, Vec::len) as f64 / f64::from(rate);
            let line = opening.then_some(ddi_library::announcer::Line::Ready);
            let colored = ddi_library::announcer::stadium::apply(&audio, rate, line);
            let (finished, did) =
                tidy::finished(&colored, ddi_library::announcer::stadium::RATE, body);
            fs::write(
                out,
                write_wav(&finished, ddi_library::announcer::stadium::RATE),
            )?;
            println!(
                "{}",
                serde_json::json!({
                    "version": ddi_library::announcer::stadium::VERSION,
                    "finishing": tidy::FINISHING, "leveling": tidy::LEVELING,
                    "body_seconds": body, "seconds": ddi_library::announcer::stadium::seconds(body, line),
                    "voiced_rms_db": did.voiced_rms_db, "gain_db": did.gain_db,
                    "limited_db": did.limited_db, "limited_peaks_db": did.peaks_db,
                })
            );
            Ok(())
        }
        Action::Tidy { dir, out, level } => tidy_folder(&dir, &out, level, None),
        Action::Finish { dir, out, bodies } => {
            let bodies = serde_json::from_slice(&fs::read(bodies)?)?;
            tidy_folder(&dir, &out, None, Some(&bodies))
        }
    }
}

/// What tidying did to a sound.
#[derive(Debug, Serialize)]
pub struct Report {
    pub file: String,
    /// The leveling's name and its numbers.
    pub leveling: &'static str,
    pub voiced_rms_target_db: f64,
    pub limit_db: f64,
    pub rate: u32,
    pub channels: usize,
    /// Its length as it came, and the part kept, seconds.
    pub seconds_in: f64,
    pub start: f64,
    pub end: f64,
    pub seconds: f64,
    /// The gain after the compressor (which keeps the voiced level),
    /// before the limiter, dB.
    pub gain_db: f64,
    /// The compressor's largest reduction and its median over the voiced
    /// frames, dB.
    pub compressed_db: f64,
    pub compressed_median_db: f64,
    /// The limiter's largest reduction, how much of it was taken of a
    /// peak alone (the rest by riding the level), and how much of the
    /// voiced level it took, dB.
    pub limited_db: f64,
    pub limited_peaks_db: f64,
    pub limited_level_db: f64,
    /// The loudest sample over the voiced RMS, before (of the part kept)
    /// and after, dB.
    pub crest_before_db: f64,
    pub crest_after_db: f64,
    /// As it comes out: the voiced RMS and the loudest sample, dBFS.
    pub voiced_rms_db: f64,
    pub peak_db: f64,
    /// What of the result is not the part kept (rotated, which changes
    /// no level) times one gain, over the result, dB: the compressor's
    /// and the limiter's work, a gain that moves slowly included.
    pub shaped_db: f64,
}

/// A WAV's samples, one `Vec` a channel, and its rate
/// ([`ddi_platform::wav::read`]).
pub fn read_wav(bytes: &[u8]) -> Result<(Vec<Vec<f32>>, u32)> {
    ddi_platform::wav::read(bytes).map_err(anyhow::Error::msg)
}

/// `channels` at `rate` as a 32-bit float WAV.
pub fn write_wav(channels: &[Vec<f32>], rate: u32) -> Vec<u8> {
    ddi_platform::wav::write_float(channels, rate)
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

/// `channels` at `rate` tidied as the studio tidies a take (leveled to
/// `level` dBFS in place of the game's target), and what that did.
pub fn tidy_sound(
    file: &str,
    channels: &[Vec<f32>],
    rate: u32,
    level: Option<f64>,
) -> (Vec<Vec<f32>>, Report) {
    let m = match level {
        Some(level) => tidy::measure_to(channels, rate, level),
        None => tidy::measure(channels, rate),
    };
    let (out, did) = tidy::tidied(channels, rate, &m);
    let at = |s: f64| (s * f64::from(rate)).round() as usize;
    let part: Vec<&[f32]> = channels
        .iter()
        .map(|c| &c[at(m.start).min(c.len())..at(m.end).min(c.len())])
        .collect();
    let peak = out
        .iter()
        .flat_map(|c| c.iter())
        .fold(0.0f32, |a, s| a.max(s.abs()));
    let report = Report {
        file: file.to_string(),
        leveling: tidy::LEVELING,
        voiced_rms_target_db: level.unwrap_or(tidy::VOICED_RMS_DB),
        limit_db: tidy::LIMIT_DB,
        rate,
        channels: channels.len(),
        seconds_in: m.seconds,
        start: m.start,
        end: m.end,
        seconds: m.tidied_seconds(),
        gain_db: m.gain_db(),
        compressed_db: did.compressed_db,
        compressed_median_db: did.compressed_median_db,
        limited_db: did.limited_db,
        limited_peaks_db: did.peaks_db,
        limited_level_db: did.limited_level_db,
        crest_before_db: tidy::crest_db(&part, rate),
        crest_after_db: tidy::crest_db(&out, rate),
        voiced_rms_db: tidy::voiced_rms_db(&out, rate),
        peak_db: db(f64::from(peak)),
        shaped_db: did.changed_db,
    };
    (out, report)
}

fn tidy_folder(
    dir: &Path,
    out: &Path,
    level: Option<f64>,
    bodies: Option<&BTreeMap<String, f64>>,
) -> Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")))
        .collect();
    files.sort();
    fs::create_dir_all(out).with_context(|| format!("making {}", out.display()))?;
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| format!("{}: its name", path.display()))?
            .to_string();
        let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let (channels, rate) = read_wav(&bytes).with_context(|| path.display().to_string())?;
        let (tidied, report) = if let Some(bodies) = bodies {
            let body = *bodies
                .get(&name)
                .with_context(|| format!("{name}: no speech duration"))?;
            let seconds = channels.first().map_or(0, Vec::len) as f64 / f64::from(rate);
            ensure!(
                body.is_finite() && body > 0.0 && body <= seconds,
                "{name}: invalid speech duration {body}"
            );
            let (audio, did) = tidy::finished(&channels, rate, body);
            let report = serde_json::json!({
                "file": name, "finishing": tidy::FINISHING, "leveling": tidy::LEVELING,
                "body_seconds": body, "seconds": seconds,
                "voiced_rms_db": did.voiced_rms_db, "gain_db": did.gain_db,
                "limited_db": did.limited_db, "limited_peaks_db": did.peaks_db,
            })
            .to_string();
            (audio, report)
        } else {
            let (audio, report) = tidy_sound(&name, &channels, rate, level);
            (audio, serde_json::to_string(&report)?)
        };
        fs::write(out.join(&name), write_wav(&tidied, rate))
            .with_context(|| format!("writing {}", out.join(&name).display()))?;
        println!("{report}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A voice of pulses between silences, at 24 kHz.
    fn voice() -> Vec<f32> {
        (0..24_000 * 2)
            .map(|i| {
                if !(6_000..36_000).contains(&i) {
                    return 0.0;
                }
                let k = i % 240;
                if k < 12 {
                    0.2 * (1.0 - (k as f32 / 6.0 - 1.0).powi(2))
                } else {
                    0.004 * ((i as f32) * 0.05).sin()
                }
            })
            .collect()
    }

    #[test]
    fn a_wav_is_read_back_as_written_float_or_16_bit() {
        let left = voice();
        let right: Vec<f32> = left.iter().map(|s| s * -0.5).collect();
        let planes = vec![left, right];
        let (back, rate) = read_wav(&write_wav(&planes, 24_000)).unwrap();
        assert_eq!((back, rate), (planes.clone(), 24_000));
        let pcm = ddi_library::wav::write(&planes, 48_000);
        let (back, rate) = read_wav(&pcm).unwrap();
        assert_eq!(rate, 48_000);
        assert_eq!(back.len(), 2);
        for (a, b) in back[0].iter().zip(&planes[0]) {
            assert!((a - b).abs() < 1.0 / 32_000.0);
        }
        assert!(read_wav(b"RIFFxxxxWAVE").is_err());
        assert!(read_wav(b"nothing").is_err());
    }

    /// The command tidies as the studio does: the same measure, the same
    /// samples, and its report says what the leveling aims at and did.
    #[test]
    fn a_sound_is_tidied_as_the_studio_tidies_a_take() {
        let planes = vec![voice()];
        let (out, report) = tidy_sound("a.wav", &planes, 24_000, None);
        let m = tidy::measure(&planes, 24_000);
        assert_eq!(out, tidy::apply(&planes, 24_000, &m));
        assert_eq!(report.leveling, tidy::LEVELING);
        assert!(
            (report.voiced_rms_db - tidy::VOICED_RMS_DB).abs() < 0.1,
            "{report:?}"
        );
        assert!(report.peak_db <= tidy::LIMIT_DB + 1e-6, "{report:?}");
        assert!(report.limited_db > 0.5 && report.crest_after_db < report.crest_before_db);
        assert!(
            report.compressed_db >= report.compressed_median_db
                && report.compressed_median_db > 0.0
                && report.limited_peaks_db <= report.limited_db
                && report.limited_level_db > 0.0,
            "{report:?}"
        );
        assert!(
            (report.start - 0.22).abs() < 0.011 && (report.end - 1.6).abs() < 0.02,
            "{report:?}"
        );
        assert!(report.shaped_db < 0.0);
        // Another level, an audition's.
        let (quiet, report) = tidy_sound("a.wav", &planes, 24_000, Some(-24.0));
        assert!((report.voiced_rms_db + 24.0).abs() < 0.1 && report.limited_db == 0.0);
        assert!(quiet[0].len() == out[0].len());
        // The compressor's work alone, the same at any level.
        assert!(report.compressed_db > 0.0 && report.limited_level_db == 0.0);
        assert!(report.shaped_db < 0.0, "{report:?}");
        // A sound at the level already: the gain alone.
        let (again, report) = tidy_sound("a.wav", &out, 24_000, None);
        assert_eq!(again[0].len(), out[0].len());
        assert!(
            report.gain_db.abs() < 0.3 && report.compressed_db == 0.0 && report.limited_db == 0.0,
            "{report:?}"
        );
        assert!(report.shaped_db < -100.0, "{report:?}");
    }

    #[test]
    fn a_folder_is_tidied_in_one_run() {
        let root = std::env::temp_dir().join(format!("ddi-xtask-tidy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (dir, out) = (root.join("in"), root.join("out"));
        fs::create_dir_all(&dir).unwrap();
        let planes = vec![voice()];
        fs::write(dir.join("b.wav"), write_wav(&planes, 24_000)).unwrap();
        fs::write(dir.join("a.wav"), ddi_library::wav::write(&planes, 24_000)).unwrap();
        fs::write(dir.join("notes.txt"), "not a sound").unwrap();
        tidy_folder(&dir, &out, None, None).unwrap();
        let names: Vec<String> = fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let (back, rate) = read_wav(&fs::read(out.join("b.wav")).unwrap()).unwrap();
        assert_eq!(rate, 24_000);
        assert_eq!(back, tidy_sound("b.wav", &planes, 24_000, None).0);
        let _ = fs::remove_dir_all(&root);
    }
}
