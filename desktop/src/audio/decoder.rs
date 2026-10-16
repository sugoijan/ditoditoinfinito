//! The decoder thread: music read from disk, decoded
//! (`ddi_platform::decode_all`), cut to the part asked for and resampled
//! to the output's rate, once, off the main and audio threads. A newer
//! job replaces any still waiting (a preview superseded by the next
//! selection); a decoder that panics on a broken file fails its job only.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use ddi_platform::decode_all::{self, Range};

use super::mixer::{Pcm, Sound};

/// What to decode.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: u64,
    pub path: PathBuf,
    /// From this second (earlier audio decoded and dropped: exact, and
    /// quick at a few hundred times real time).
    pub from: f64,
    /// To this second; `None`: the end.
    pub until: Option<f64>,
    /// The output's rate.
    pub rate: u32,
    /// Whose decode an MP3 lines up with (a work's: Chrome's).
    pub target: ddi_platform::mp3::Target,
}

/// A job's answer.
#[derive(Debug)]
pub struct Done {
    pub id: u64,
    pub result: Result<Sound, String>,
    pub took: Duration,
}

pub struct Decoder {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    next: u64,
    waiting: usize,
}

impl Decoder {
    pub fn new() -> Decoder {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("ddi-decoder".into())
            .spawn(move || {
                while let Ok(mut job) = rx.recv() {
                    // Only the newest waiting job is worth doing.
                    loop {
                        match rx.try_recv() {
                            Ok(newer) => {
                                let _ = tx.send(Done {
                                    id: job.id,
                                    result: Err("replaced".into()),
                                    took: Duration::ZERO,
                                });
                                job = newer;
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }
                    let start = Instant::now();
                    let result = std::panic::catch_unwind(|| run(&job))
                        .unwrap_or_else(|_| Err("the decoder failed on this file".into()));
                    let done = Done {
                        id: job.id,
                        result,
                        took: start.elapsed(),
                    };
                    if tx.send(done).is_err() {
                        return;
                    }
                }
            })
            .expect("decoder thread");
        Decoder {
            jobs,
            done,
            next: 1,
            waiting: 0,
        }
    }

    /// Queues `path` from `from` to `until` seconds at `rate`; its id.
    pub fn decode(&mut self, path: &Path, from: f64, until: Option<f64>, rate: u32) -> u64 {
        self.decode_for(path, from, until, rate, Default::default())
    }

    /// [`Self::decode`], an MP3 lined up with `target`'s decode.
    pub fn decode_for(
        &mut self,
        path: &Path,
        from: f64,
        until: Option<f64>,
        rate: u32,
        target: ddi_platform::mp3::Target,
    ) -> u64 {
        let id = self.next;
        self.next += 1;
        if self
            .jobs
            .send(Job {
                id,
                path: path.to_path_buf(),
                from,
                until,
                rate,
                target,
            })
            .is_ok()
        {
            self.waiting += 1;
        }
        id
    }

    /// A job finished, if one has.
    pub fn poll(&mut self) -> Option<Done> {
        let d = self.done.try_recv().ok()?;
        self.waiting = self.waiting.saturating_sub(1);
        Some(d)
    }

    /// Jobs not answered yet.
    pub fn busy(&self) -> bool {
        self.waiting > 0
    }
}

impl Default for Decoder {
    fn default() -> Decoder {
        Decoder::new()
    }
}

fn run(job: &Job) -> Result<Sound, String> {
    let bytes = std::fs::read(&job.path).map_err(|e| format!("{}: {e}", job.path.display()))?;
    let ext = job
        .path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let range = Range {
        from: job.from,
        until: job.until,
        target: job.target,
    };
    sound(bytes, ext.as_deref(), range, job.rate)
}

/// The part `range` of a file's audio (`ext` helps the probe) as a sound
/// at `rate`. Only that part is kept as it decodes; folded to stereo,
/// then resampled straight into the interleaved buffer the mixer plays:
/// at most the decoded part and the result at once.
pub fn sound(bytes: Vec<u8>, ext: Option<&str>, range: Range, rate: u32) -> Result<Sound, String> {
    let audio = decode_all::decode_range(&bytes, ext, range).map_err(|e| e.to_string())?;
    drop(bytes);
    let audio = Pcm::fold(audio);
    let channels = audio.channels.len() as u16;
    let data = decode_all::resample_interleaved(audio, rate).map_err(|e| e.to_string())?;
    Ok(std::sync::Arc::new(Pcm {
        rate,
        channels,
        data,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../platform/tests/fixtures")
            .join(name)
    }

    fn wait(d: &mut Decoder) -> Done {
        let start = Instant::now();
        loop {
            if let Some(done) = d.poll() {
                return done;
            }
            assert!(start.elapsed() < Duration::from_secs(20), "no answer");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn decodes_a_slice_at_the_output_rate() {
        let mut d = Decoder::new();
        // 0.1 s of 48 kHz Opus, 0.02–0.07 s, at 44.1 kHz.
        let id = d.decode(&fixture("sine-440.opus"), 0.02, Some(0.07), 44_100);
        let done = wait(&mut d);
        assert_eq!(done.id, id);
        let pcm = done.result.unwrap();
        assert_eq!(pcm.rate, 44_100);
        assert_eq!(pcm.channels, 1);
        assert!((pcm.frames() as i64 - 2_205).abs() <= 1, "{}", pcm.frames());
        assert!(!d.busy());
    }

    #[test]
    fn missing_and_broken_files_fail_their_job_only() {
        let mut d = Decoder::new();
        d.decode(Path::new("/nonexistent/song.ogg"), 0.0, None, 48_000);
        assert!(wait(&mut d).result.is_err());
        let id = d.decode(&fixture("sine-441.flac"), 0.0, None, 44_100);
        let done = wait(&mut d);
        assert_eq!(done.id, id);
        assert_eq!(done.result.unwrap().frames(), 11_025);
    }
}
