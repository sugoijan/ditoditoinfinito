//! A deadline around each movie's OS decoder, plus module fallback at the
//! same position. No OS call (including destruction) runs on the game or
//! module thread. One timeout disables new native sessions for this process;
//! at most four workers can exist, including any abandoned worker.

use super::{
    Backend, DecoderStatus, Event, Frame, Machine, MovieFile, Msg, avi::Media, vt::Session,
};
use ddi_library::h264::{self, ExactSeek, Offer, OutputOrder, PictureOrder};
use ddi_platform::video::{VideoInfo, YuvMatrix};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

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
            return Err("VideoToolbox disabled after a stalled session".into());
        }
        WORKERS
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n < MAX_WORKERS).then_some(n + 1)
            })
            .map_err(|_| "VideoToolbox worker limit")?;
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
    fn open(path: MovieFile, deadline: Duration) -> Result<(Self, VideoInfo), String> {
        let permit = Permit::take()?;
        let (tx, input) = mpsc::sync_channel(1);
        let (output, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ddi-videotoolbox".into())
            .spawn(move || {
                let _permit = permit;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut decoder = Decoder::open(&path)?;
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
                .unwrap_or_else(|_| Err("VideoToolbox worker panicked".into()));
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
                Err("VideoToolbox session stalled; disabled for this process".into())
            }
            Err(_) => Err("VideoToolbox worker stopped".into()),
        }
    }
    fn request(&self, command: Command) -> Result<Reply, String> {
        self.tx
            .try_send(command)
            .map_err(|_| "VideoToolbox worker unavailable")?;
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
            Msg::Open { id, path, name } if self.backend != Backend::Module => {
                match Worker::open(path.clone(), self.deadline) {
                    Ok((worker, info)) => {
                        machine.decoder_status(
                            id,
                            DecoderStatus {
                                backend: "VideoToolbox (hardware)".into(),
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
                                backend: if self.backend == Backend::VideoToolbox {
                                    "unavailable"
                                } else {
                                    "module"
                                }
                                .into(),
                                fallback: Some(why.clone()),
                            },
                        );
                        if self.backend == Backend::VideoToolbox {
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
        if self.backend == Backend::VideoToolbox {
            m.post(id, Some(v.epoch), Event::Error(why));
            return;
        }
        m.decoder_status(
            id,
            DecoderStatus {
                backend: "module".into(),
                fallback: Some(why.clone()),
            },
        );
        eprintln!("[video] {}: VideoToolbox → module: {why}", v.name);
        let result = m.open_announced(id, v.path, &v.name, false).and_then(|()| {
            if let Some(target) = v.last.or(v.seek) {
                m.seek(id, target, v.epoch)?;
                if let Some(movie) = m.videos.get_mut(&id) {
                    movie.skip_through = v.last;
                }
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
    pictures: PictureOrder,
    order: OutputOrder,
    sequence: VecDeque<(usize, f64)>,
    decoded: HashMap<usize, Vec<u8>>,
    received: Vec<bool>,
    ready: VecDeque<(Vec<u8>, f64)>,
    exact: ExactSeek<Vec<u8>>,
    index: usize,
    start: usize,
    target: Option<f64>,
    retries: u32,
    generation: u32,
    flushed: bool,
    last: Option<f64>,
}
impl Decoder {
    fn open(path: &MovieFile) -> Result<Self, String> {
        let media = Media::open(path)?;
        let session = Self::session(&media, 0)?;
        let pictures = PictureOrder::new(&media.stream);
        let order = OutputOrder::new(media.delay);
        let received = vec![false; media.chunks.len()];
        Ok(Self {
            media,
            session,
            pictures,
            order,
            sequence: VecDeque::new(),
            decoded: HashMap::new(),
            received,
            ready: VecDeque::new(),
            exact: ExactSeek::new(None, false),
            index: 0,
            start: 0,
            target: None,
            retries: 0,
            generation: 0,
            flushed: false,
            last: None,
        })
    }
    fn session(m: &Media, generation: u32) -> Result<Session, String> {
        Session::new(
            &m.sets,
            m.stream.sps.width,
            m.stream.sps.height,
            generation,
            m.stream.sps.full_range,
        )
    }
    fn info(&self) -> VideoInfo {
        VideoInfo {
            width: self.media.stream.sps.width,
            height: self.media.stream.sps.height,
            duration: self.media.chunks.last().map(|c| {
                (u64::from(c.frame) + 1 - u64::from(self.media.avi.start)) as f64 * self.period()
            }),
            codec: "h264".into(),
        }
    }
    fn period(&self) -> f64 {
        f64::from(self.media.avi.scale) / f64::from(self.media.avi.rate)
    }
    fn reset(&mut self, start: usize, target: Option<f64>, may_retry: bool) -> Result<(), String> {
        self.session = Self::session(&self.media, self.generation)?;
        self.pictures = PictureOrder::new(&self.media.stream);
        self.order = OutputOrder::new(self.media.delay);
        self.sequence.clear();
        self.decoded.clear();
        self.received.fill(false);
        self.ready.clear();
        self.exact = ExactSeek::new(target, may_retry);
        self.index = start;
        self.start = start;
        self.flushed = false;
        self.last = None;
        Ok(())
    }
    fn seek(&mut self, t: f64, generation: u32) -> Result<(), String> {
        self.generation = generation;
        self.target = Some(if t.is_finite() { t.max(0.0) } else { 0.0 });
        self.retries = 0;
        let start = h264::seek_start(
            &self.media.chunks,
            self.target.unwrap() / self.period(),
            self.media.delay,
        );
        self.reset(start.unwrap_or(0), self.target, start.is_some())
    }
    fn next(&mut self) -> Result<Option<Frame>, String> {
        let began = Instant::now();
        loop {
            if let Some((planes, pts)) = self.ready.pop_front() {
                let s = &self.media.stream.sps;
                return Ok(Some(Frame {
                    pts,
                    width: s.width,
                    height: s.height,
                    matrix: if s.bt709() {
                        YuvMatrix::Bt709
                    } else {
                        YuvMatrix::Bt601
                    },
                    full_range: s.full_range,
                    planes,
                    decode_ms: began.elapsed().as_secs_f64() * 1000.0,
                }));
            }
            if let Some(&(id, stamp)) = self.sequence.front()
                && let Some(planes) = self.decoded.remove(&id)
            {
                self.sequence.pop_front();
                let t = stamp;
                self.last = Some(t);
                match self.exact.offer(planes, t) {
                    Offer::Show(frames) => self.ready.extend(frames),
                    Offer::Retry => {
                        let start = h264::seek_back(
                            &self.media.chunks,
                            self.media.chunks[self.start].frame,
                            self.retries,
                            1.0 / self.period(),
                        );
                        self.retries += 1;
                        self.reset(start.unwrap_or(0), self.target, start.is_some())?;
                    }
                }
                continue;
            }
            if self.flushed {
                if !self.sequence.is_empty() {
                    return Err("VideoToolbox omitted an expected frame".into());
                }
                if let Some(frame) = self.exact.end(self.period()) {
                    self.ready.push_back(frame);
                    continue;
                }
                return Ok(None);
            }
            if self.decoded.len() >= REORDER_FRAMES
                || self.sequence.len() >= REORDER_FRAMES
                || self.decoded.values().map(Vec::len).sum::<usize>() > REORDER_BYTES
            {
                return Err("VideoToolbox reorder queue exceeded its bound".into());
            }
            if self.index < self.media.chunks.len() {
                let i = self.index;
                let bytes = self.media.packet(i)?;
                let picture = self
                    .pictures
                    .picture(&bytes, self.media.stream.framing, &self.media.stream.sps)
                    .ok_or("H.264 packet has no supported picture")?;
                if i == self.start && !picture.idr {
                    return Err("H.264 seek did not land on an IDR".into());
                }
                let released = self.order.push(i, picture, self.media.chunks[i].frame);
                if released.dropped.is_some() {
                    return Err("H.264 output order dropped a picture".into());
                }
                if let Some(frame) = released.out {
                    self.sequence
                        .push_back((frame.0, frame.1 as f64 * self.period()));
                }
                self.session.decode(&self.media.sample(&bytes)?, i)?;
                self.index += 1;
            } else {
                let mut last = self
                    .sequence
                    .back()
                    .map(|f| f.1)
                    .or(self.last)
                    .unwrap_or(-1.0)
                    .max(
                        self.media
                            .chunks
                            .last()
                            .map_or(-1.0, |c| f64::from(c.frame) * self.period()),
                    );
                for (id, _) in self.order.drain() {
                    last += self.period();
                    self.sequence.push_back((id, last));
                }
                self.session.finish()?;
                self.flushed = true;
            }
            for output in self.session.take()? {
                if output.generation != self.generation {
                    continue;
                }
                if output.id >= self.index || std::mem::replace(&mut self.received[output.id], true)
                {
                    return Err("invalid or duplicate VideoToolbox frame identity".into());
                }
                if self.decoded.len() >= REORDER_FRAMES
                    || self.decoded.values().map(Vec::len).sum::<usize>() + output.planes.len()
                        > REORDER_BYTES
                {
                    return Err("VideoToolbox reorder queue exceeded its bound".into());
                }
                self.decoded.insert(output.id, output.planes);
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
