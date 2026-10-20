//! A deadline around each movie's OS decoder, plus module fallback at the
//! same position. No OS call (including destruction) runs on the game or
//! module thread. One timeout disables new native sessions for this process;
//! at most four workers can exist, including any abandoned worker.

use super::{
    Backend, DecoderStatus, Event, Frame, Machine, MovieFile, Msg, codec::Order, media::Media,
};
use ddi_library::h264::{ExactSeek, Offer};
use ddi_platform::video::VideoInfo;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use super::mf::Session;
#[cfg(target_os = "macos")]
use super::vt::Session;
#[cfg(target_os = "macos")]
const NAME: &str = "VideoToolbox";
#[cfg(target_os = "windows")]
const NAME: &str = "Media Foundation";
#[cfg(target_os = "macos")]
const SELECTED: &str = "VideoToolbox (hardware)";
#[cfg(target_os = "windows")]
const SELECTED: &str = "Media Foundation (D3D11; experimental)";

pub(super) struct Output {
    pub id: usize,
    pub generation: u32,
    pub pixels: Pixels,
}

pub(super) enum Pixels {
    Planes(Vec<u8>),
    #[cfg(all(target_os = "macos", feature = "video-metal"))]
    Surface(super::metal::Surface),
}
impl Pixels {
    fn len(&self) -> usize {
        match self {
            Self::Planes(p) => p.len(),
            #[cfg(all(target_os = "macos", feature = "video-metal"))]
            Self::Surface(p) => p.bytes(),
        }
    }
}

const DEADLINE: Duration = Duration::from_secs(4);
const MAX_WORKERS: usize = 4;
const REORDER_FRAMES: usize = 32;
const REORDER_BYTES: usize = 128 * 1024 * 1024;
static WORKERS: AtomicUsize = AtomicUsize::new(0);
static STALLED: AtomicBool = AtomicBool::new(false);
struct Permit;
impl Permit {
    fn take() -> Result<Self, String> {
        if STALLED.load(Ordering::Relaxed) {
            return Err(format!("{NAME} disabled after a stalled session"));
        }
        WORKERS
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n < MAX_WORKERS).then_some(n + 1)
            })
            .map_err(|_| "native video worker limit")?;
        Ok(Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::Relaxed);
    }
}

enum Command {
    Next,
    Seek(f64, u32),
}
enum Reply {
    Open(VideoInfo),
    Next(Option<Frame>, Option<f64>),
    Sought,
}
struct Worker {
    tx: SyncSender<Command>,
    rx: Receiver<Result<Reply, String>>,
    deadline: Duration,
}
impl Worker {
    fn open(
        path: MovieFile,
        name: String,
        helper: Option<PathBuf>,
        deadline: Duration,
    ) -> Result<(Self, VideoInfo), String> {
        let permit = Permit::take()?;
        let (tx, input) = mpsc::sync_channel(1);
        let (output, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ddi-native-video".into())
            .spawn(move || {
                let _permit = permit;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut decoder = Decoder::open(&path, &name, helper)?;
                    output
                        .send(Ok(Reply::Open(decoder.info())))
                        .map_err(|_| "movie closed")?;
                    while let Ok(cmd) = input.recv() {
                        let reply = match cmd {
                            Command::Next => decoder.next().map(|f| Reply::Next(f, decoder.last)),
                            Command::Seek(t, g) => decoder.seek(t, g).map(|()| Reply::Sought),
                        };
                        let failed = reply.is_err();
                        if output.send(reply).is_err() || failed {
                            break;
                        }
                    }
                    Ok::<_, String>(())
                }))
                .unwrap_or_else(|_| Err("native video worker panicked".into()));
                if let Err(e) = result {
                    let _ = output.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        let worker = Self { tx, rx, deadline };
        match worker.receive()? {
            Reply::Open(info) => Ok((worker, info)),
            _ => Err("invalid decoder open reply".into()),
        }
    }
    fn receive(&self) -> Result<Reply, String> {
        match self.rx.recv_timeout(self.deadline) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                STALLED.store(true, Ordering::Relaxed);
                Err(format!("{NAME} session stalled; disabled for this process"))
            }
            Err(_) => Err("native video worker stopped".into()),
        }
    }
    fn request(&self, command: Command) -> Result<Reply, String> {
        self.tx
            .try_send(command)
            .map_err(|_| "native video worker unavailable")?;
        self.receive()
    }
}

struct NativeMovie {
    worker: Worker,
    path: MovieFile,
    name: String,
    epoch: u32,
    seek: Option<f64>,
    last: Option<f64>,
    same_time: usize,
    ended: bool,
}

pub(super) struct Hub {
    backend: Backend,
    movies: HashMap<u32, NativeMovie>,
    deadline: Duration,
}
impl Hub {
    pub fn new(backend: Backend, deadline: Duration) -> Self {
        Self {
            backend,
            movies: HashMap::new(),
            deadline: deadline.min(DEADLINE),
        }
    }
    /// None: handled here; Some: the module handles the original message.
    pub fn handle(&mut self, machine: &mut Machine, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::Open { id, path, name } if self.backend.native_on(std::env::consts::OS) => {
                match Worker::open(
                    path.clone(),
                    name.clone(),
                    machine.helper_path(),
                    self.deadline,
                ) {
                    Ok((worker, info)) => {
                        machine.decoder_status(
                            id,
                            DecoderStatus {
                                backend: SELECTED.into(),
                                fallback: None,
                            },
                        );
                        self.movies.insert(
                            id,
                            NativeMovie {
                                worker,
                                path,
                                name,
                                epoch: 0,
                                seek: None,
                                last: None,
                                same_time: 0,
                                ended: false,
                            },
                        );
                        machine.post(id, None, Event::Opened(info));
                        None
                    }
                    Err(why) => {
                        machine.decoder_status(
                            id,
                            DecoderStatus {
                                backend: if self.backend.strict() {
                                    "unavailable"
                                } else {
                                    "software"
                                }
                                .into(),
                                fallback: Some(why.clone()),
                            },
                        );
                        if self.backend.strict() {
                            machine.post(id, None, Event::Error(why));
                            None
                        } else {
                            Some(Msg::Open { id, path, name })
                        }
                    }
                }
            }
            Msg::Close { id } if self.movies.remove(&id).is_some() => None,
            Msg::Seek { id, t, epoch } if self.movies.contains_key(&id) => {
                let v = self.movies.get_mut(&id).unwrap();
                v.epoch = epoch;
                v.seek = Some(t);
                v.last = None;
                v.same_time = 0;
                v.ended = false;
                match v.worker.request(Command::Seek(t, epoch)) {
                    Ok(Reply::Sought) => {}
                    Err(e) => self.fallback(machine, id, e),
                    _ => self.fallback(machine, id, "invalid decoder seek reply".into()),
                }
                None
            }
            Msg::Want { id, n, epoch } if self.movies.contains_key(&id) => {
                for i in 0..n {
                    // A queued seek/close invalidates old work before its message
                    // reaches this thread; never refill a superseded generation.
                    if !machine
                        .shared()
                        .movies
                        .get(&id)
                        .is_some_and(|m| m.generation == epoch)
                    {
                        break;
                    }
                    let v = self.movies.get_mut(&id).unwrap();
                    if v.epoch != epoch || v.ended {
                        break;
                    }
                    match v.worker.request(Command::Next) {
                        Ok(Reply::Next(Some(frame), _)) => {
                            v.same_time = if v.last == Some(frame.pts) {
                                v.same_time + 1
                            } else {
                                1
                            };
                            v.last = Some(frame.pts);
                            machine.post(id, Some(epoch), Event::Frame(frame));
                        }
                        Ok(Reply::Next(None, last_pts)) => {
                            v.ended = true;
                            machine.post(id, Some(epoch), Event::End { last_pts });
                            break;
                        }
                        reply => {
                            let why = match reply {
                                Err(e) => e,
                                _ => "invalid decoder frame reply".into(),
                            };
                            self.fallback(machine, id, why);
                            machine.handle(Msg::Want {
                                id,
                                n: n - i,
                                epoch,
                            });
                            break;
                        }
                    }
                }
                None
            }
            msg => Some(msg),
        }
    }

    pub fn crashed(&mut self, m: &mut Machine, why: &str) {
        for (id, _) in self.movies.drain() {
            m.post(
                id,
                None,
                Event::Error(format!("the video host failed: {why}")),
            );
        }
    }

    fn fallback(&mut self, m: &mut Machine, id: u32, why: String) {
        let v = self.movies.remove(&id).unwrap();
        if self.backend.strict() {
            m.post(id, Some(v.epoch), Event::Error(why));
            return;
        }
        m.decoder_status(
            id,
            DecoderStatus {
                backend: "software".into(),
                fallback: Some(why.clone()),
            },
        );
        eprintln!("[video] {}: {NAME} → module: {why}", v.name);
        let result = m.open_announced(id, v.path, &v.name, false).and_then(|()| {
            if let Some(last) = v.last {
                let coalesced = v.seek.is_some_and(|t| last <= t + 1e-6);
                if coalesced {
                    m.seek(id, v.seek.expect("coalesced seek"), v.epoch)?;
                } else if last > 1e-5 {
                    // Seek just before the group, so ExactSeek cannot discard
                    // its still-undelivered pictures. At zero reopen directly.
                    m.seek(id, last - 1e-5, v.epoch)?;
                }
                if let Some(movie) = m.videos.get_mut(&id) {
                    movie.epoch = v.epoch;
                    movie.resume = Some(super::Resume {
                        time: last,
                        remaining: v.same_time,
                    });
                }
            } else if let Some(target) = v.seek {
                m.seek(id, target, v.epoch)?;
            } else if let Some(movie) = m.videos.get_mut(&id) {
                movie.epoch = v.epoch;
            }
            Ok(())
        });
        if let Err(super::Crash(e)) = result {
            m.crashed(&e, Some(id));
        }
        // Dropping the controller disconnects the worker, never joins it.
    }
}

struct Decoder {
    media: Media,
    session: Session,
    order: Order,
    sequence: VecDeque<(usize, f64)>,
    decoded: HashMap<usize, Pixels>,
    submitted: HashSet<usize>,
    durations: HashMap<usize, Option<f64>>,
    timeline: Option<f64>,
    last_dts: Option<f64>,
    last_length: f64,
    ready: VecDeque<(Pixels, f64)>,
    exact: ExactSeek<Pixels>,
    index: usize,
    target: Option<f64>,
    generation: u32,
    flushed: bool,
    last: Option<f64>,
}
impl Decoder {
    fn open(path: &MovieFile, name: &str, helper: Option<PathBuf>) -> Result<Self, String> {
        let media = Media::open(path, name, helper)?;
        #[cfg(target_os = "windows")]
        if !media.packet_path() {
            return Err(
                "experimental Media Foundation requires MP4/Matroska/WebM packet PTS".into(),
            );
        }
        let session = Self::session(&media, 0)?;
        let order = media.format.order();
        let last_length = media.period;
        Ok(Self {
            media,
            session,
            order,
            sequence: VecDeque::new(),
            decoded: HashMap::new(),
            submitted: HashSet::new(),
            durations: HashMap::new(),
            timeline: None,
            last_dts: None,
            last_length,
            ready: VecDeque::new(),
            exact: ExactSeek::new(None, false),
            index: 0,
            target: None,
            generation: 0,
            flushed: false,
            last: None,
        })
    }
    fn session(m: &Media, generation: u32) -> Result<Session, String> {
        Session::new(&m.format, generation, m.period)
    }
    fn info(&self) -> VideoInfo {
        self.media.info()
    }
    fn period(&self) -> f64 {
        self.media.period
    }
    fn reset(&mut self) -> Result<(), String> {
        self.session = Self::session(&self.media, self.generation)?;
        self.order = self.media.format.order();
        self.sequence.clear();
        self.decoded.clear();
        self.submitted.clear();
        self.durations.clear();
        self.ready.clear();
        self.exact = ExactSeek::new(self.target, self.media.may_retry);
        self.index = 0;
        self.timeline = None;
        self.last_dts = None;
        self.last_length = self.period();
        self.flushed = false;
        self.last = None;
        Ok(())
    }
    fn seek(&mut self, t: f64, generation: u32) -> Result<(), String> {
        self.generation = generation;
        let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
        self.target = Some(t);
        self.media.seek(t)?;
        self.reset()
    }
    fn missing_time(&self) -> f64 {
        let before = self
            .timeline
            .unwrap_or(-1.0)
            .max(self.last_dts.unwrap_or(-1.0));
        if before < 0.0 {
            0.0
        } else {
            before + self.period()
        }
    }
    fn next(&mut self) -> Result<Option<Frame>, String> {
        let began = Instant::now();
        loop {
            if let Some((planes, pts)) = self.ready.pop_front() {
                let mut frame = Frame {
                    pts,
                    width: self.media.format.width(),
                    height: self.media.format.height(),
                    matrix: self.media.format.matrix(),
                    full_range: self.media.format.full_range(),
                    planes: Vec::new(),
                    #[cfg(all(target_os = "macos", feature = "video-metal"))]
                    surface: None,
                    decode_ms: began.elapsed().as_secs_f64() * 1000.0,
                };
                match planes {
                    Pixels::Planes(p) => frame.planes = p,
                    #[cfg(all(target_os = "macos", feature = "video-metal"))]
                    Pixels::Surface(p) => frame.surface = Some(p),
                }
                return Ok(Some(frame));
            }
            if let Some(&(id, stamp)) = self.sequence.front()
                && let Some(planes) = self.decoded.remove(&id)
            {
                self.sequence.pop_front();
                let t = stamp;
                self.last = Some(t);
                self.last_length = self
                    .durations
                    .remove(&id)
                    .flatten()
                    .unwrap_or(self.period());
                match self.exact.offer(planes, t) {
                    Offer::Show(frames) => self.ready.extend(frames),
                    Offer::Retry => {
                        self.media.retry()?;
                        self.reset()?;
                    }
                }
                continue;
            }
            if self.flushed {
                if !self.sequence.is_empty() {
                    return Err("native decoder omitted an expected frame".into());
                }
                if self.last.is_none() && self.media.may_retry {
                    self.media.reopen()?;
                    self.reset()?;
                    continue;
                }
                if let Some(frame) = self.exact.end(self.last_length) {
                    self.ready.push_back(frame);
                    continue;
                }
                return Ok(None);
            }
            if self.submitted.len() >= REORDER_FRAMES
                || self.durations.len() >= REORDER_FRAMES
                || self.decoded.len() >= REORDER_FRAMES
                || self.sequence.len() >= REORDER_FRAMES
                || self.decoded.values().map(Pixels::len).sum::<usize>() > REORDER_BYTES
            {
                return Err("native decoder reorder queue exceeded its bound".into());
            }
            if let Some(packet) = self.media.next()? {
                let i = self.index;
                if i >= 1_000_000 {
                    return Err("native video packet count exceeds its bound".into());
                }
                let (sample, released) = self.order.submit(
                    &self.media.format,
                    &packet.bytes,
                    i,
                    self.media.packet_path(),
                )?;
                if let Some(stamp) = packet.stamp {
                    self.last_dts = Some(stamp);
                }
                for id in released {
                    let stamp = packet
                        .stamp
                        .map(|t| t.max(0.0))
                        .unwrap_or_else(|| self.missing_time());
                    self.timeline = Some(stamp);
                    self.sequence.push_back((id, stamp));
                }
                self.submitted.insert(i);
                self.durations.insert(i, packet.duration);
                self.session.decode(&sample, i, packet.presentation)?;
                self.index += 1;
            } else {
                for id in self.order.drain() {
                    let stamp = self.missing_time();
                    self.timeline = Some(stamp);
                    self.sequence.push_back((id, stamp));
                }
                self.session.finish()?;
                self.flushed = true;
            }
            for output in self.session.take()? {
                if output.generation != self.generation {
                    continue;
                }
                if !self.submitted.remove(&output.id) {
                    return Err("invalid or duplicate native frame identity".into());
                }
                if self.decoded.len() >= REORDER_FRAMES
                    || self.decoded.values().map(Pixels::len).sum::<usize>() + output.pixels.len()
                        > REORDER_BYTES
                {
                    return Err("native decoder reorder queue exceeded its bound".into());
                }
                self.decoded.insert(output.id, output.pixels);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_stalled_worker_is_abandoned_and_disables_new_sessions() {
        let (tx, _commands) = mpsc::sync_channel(1);
        let (_answers, rx) = mpsc::sync_channel(1);
        let worker = Worker {
            tx,
            rx,
            deadline: Duration::from_millis(10),
        };
        let start = Instant::now();
        assert!(matches!(worker.request(Command::Next), Err(e) if e.contains("stalled")));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(matches!(Permit::take(), Err(e) if e.contains("disabled")));
        drop(worker); // No join or native teardown on this controller.
        STALLED.store(false, Ordering::Relaxed);
    }
}
