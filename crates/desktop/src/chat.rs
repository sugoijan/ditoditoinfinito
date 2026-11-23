//! A room host's chat on desktop (`docs/plans/multiplayer-twitch.md`,
//! part C): its emote images fetched and decoded off the frame, each on a
//! thread of its own (four at most at once, the chat's cache says), and,
//! TESTING ONLY, the scripted chat `--mp-dev-chat` plays
//! (`ddi_chat::script`).
//!
//! An image is read before it is decoded (`ddi_chat::anim::probe`,
//! round TW4): refused where it is larger than an emote or broken; an
//! animation within its limits decoded frame by frame with the `image`
//! crate (GIF, WebP, APNG), only the frames the atlas keeps scaled and
//! kept, under the crate's own limits and a deadline, else its first
//! frame, still.
//!
//! The scripted chat's images are its own shapes (drawn by
//! `ddi_shell::chat`) or files and addresses its script names (a local
//! test server's). One script reaches the providers: the global-emotes
//! demo (`--mp-dev-chat global`, round TW5, TESTING ONLY,
//! `ddi_chat::global`), which the maintainer runs: their global sets read
//! at the start (directly, then through our server's proxy with the
//! run's session, Twitch's through our server alone), their images as a
//! real chat's.
//!
//! The live chat (round TW6, [`DesktopLive`]): the play's
//! `ddi_session::chat::LiveIo` on desktop: each socket to Twitch's
//! EventSub on a thread of its own as a room's socket is
//! (`crate::online::PlainSocket`, tungstenite over the system's TLS), our
//! server's subscribe route and the providers' sets through ureq on a
//! thread a request, and the run's report on stderr, every line led by
//! `ddi_chat::live::LOG_PREFIX`. `--mp-dev-chat live` runs it in solo
//! plays and CPU practice's stages for the signed-in person's own channel
//! ([`own_channel`]; a player's are online rooms' stages alone);
//! `--mp-dev-chat-socket URL` points the socket at a local fake;
//! `--mp-dev-chat-record DIR` writes every raw socket frame to a file
//! there (the chat's messages in them: only where the maintainer asks).

use std::io::Cursor;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use std::collections::HashMap;
use std::sync::OnceLock;

use ddi_chat::anim::{self, Format, Kept, Verdict};
use ddi_chat::live::LOG_PREFIX;
use ddi_session::chat::{LiveInput, LiveIo, LiveSetup, LiveWant};
use ddi_chat::cache::Fetch;
use ddi_chat::script::Script;
use ddi_shell::chat::{Decoded, decode_size};

/// Images a fetch reads at most (an emote's still image is a few
/// kilobytes, an animated one rarely a megabyte).
const MOST_BYTES: u64 = anim::MOST_BYTES as u64;
/// How long an animation's frames may take to decode on their thread:
/// past it, its first frame, still.
const DEADLINE: Duration = Duration::from_millis(1500);
/// Bytes the decoder may allocate at once (its canvas and buffers).
const MOST_ALLOC: u64 = 64 << 20;

/// The fetches in flight and what came back.
pub struct Fetcher {
    tx: Sender<(Fetch, Option<Decoded>)>,
    rx: Receiver<(Fetch, Option<Decoded>)>,
}

impl Default for Fetcher {
    fn default() -> Fetcher {
        let (tx, rx) = channel();
        Fetcher { tx, rx }
    }
}

impl Fetcher {
    /// Fetches `f` on a thread: an `http(s)` address through the shared
    /// client, else a file's path.
    pub fn fetch(&mut self, f: &Fetch) -> bool {
        let tx = self.tx.clone();
        let f = f.clone();
        std::thread::Builder::new()
            .name("ddi-chat-emote".into())
            .spawn(move || {
                let decoded = read(&f.url, f.via == ddi_chat::cache::Via::Proxy)
                    .map_err(|e| eprintln!("chat emote {} ({:?}): {e}", f.url, f.via))
                    .ok()
                    .and_then(|bytes| decode(&bytes));
                let _ = tx.send((f, decoded));
            })
            .is_ok()
    }

    /// The fetches that ended since.
    pub fn take(&mut self) -> Vec<(Fetch, Option<Decoded>)> {
        self.rx.try_iter().collect()
    }
}

/// The bytes at `url` ([`MOST_BYTES`] at most): an `http(s)` address
/// through the shared client (our server's proxy, `proxy`, with the run's
/// session, which it asks for), else a file's path; or why not.
fn read(url: &str, proxy: bool) -> Result<Vec<u8>, String> {
    read_with(url, proxy.then(crate::online::session_token).flatten())
}

/// [`read`] with the session `token` it is asked with, if any.
fn read_with(url: &str, token: Option<String>) -> Result<Vec<u8>, String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut request = crate::share::agent().get(url);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let mut answer = request.call().map_err(|e| e.to_string())?;
        if !answer.status().is_success() {
            return Err(format!("HTTP {}", answer.status().as_u16()));
        }
        answer
            .body_mut()
            .with_config()
            .limit(MOST_BYTES)
            .read_to_vec()
            .map_err(|e| e.to_string())
    } else {
        let path = url.strip_prefix("file://").unwrap_or(url);
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if meta.len() > MOST_BYTES {
            return Err(anim::Refusal::TooLarge.to_string());
        }
        std::fs::read(path).map_err(|e| e.to_string())
    }
}

/// TESTING ONLY (round TW6): the folder `--mp-dev-chat-record` writes the
/// live chat's raw socket frames to.
static RECORD: OnceLock<std::path::PathBuf> = OnceLock::new();

/// TESTING ONLY: record every raw frame of the live chat's sockets under
/// `dir` from now on (`--mp-dev-chat-record DIR`).
pub fn record_to(dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    eprintln!(
        "{LOG_PREFIX} recording every raw socket frame (people's chat messages in them) to {}",
        dir.display()
    );
    let _ = RECORD.set(dir);
}

/// Round TW7: the live chat's report kept beside stderr, so a run's can be
/// sent later: `chat-report.txt` in the data folder, replaced by the first
/// live chat of each run of the app, every play's lines after it (the
/// same redacted lines stderr has, never a token, a session or a person's
/// chat), [`REPORT_MOST`] bytes at most.
pub const REPORT_FILE: &str = "chat-report.txt";

/// The report file's size at most; past it, one line saying so.
pub const REPORT_MOST: u64 = 1 << 20;

/// This run's report file, once opened (`None` inside: none could be).
static REPORT: std::sync::Mutex<Option<ReportFile>> = std::sync::Mutex::new(None);

/// The folder this run's report file goes in, once set.
static REPORT_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// This run's report file goes in `dir`: the app's run sets the data
/// folder (`crate::run`), a test a scratch folder of its own; unset, no
/// file is written (so a test never writes into a player's folders).
pub fn report_into(dir: std::path::PathBuf) {
    let _ = REPORT_DIR.set(dir);
}

/// A run's report file.
pub struct ReportFile {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
    written: u64,
}

impl ReportFile {
    /// `REPORT_FILE` in `dir`, emptied (created where missing).
    pub fn open(dir: &std::path::Path) -> ReportFile {
        let path = dir.join(REPORT_FILE);
        let _ = std::fs::create_dir_all(dir);
        let file = std::fs::File::create(&path).ok();
        ReportFile {
            path,
            file,
            written: 0,
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// One line of the report (led by `ddi_chat::live::LOG_PREFIX`, as on
    /// stderr), within [`REPORT_MOST`].
    pub fn line(&mut self, line: &str) {
        use std::io::Write;
        let Some(f) = self.file.as_mut() else { return };
        let text = format!("{LOG_PREFIX} {line}\n");
        if self.written + text.len() as u64 > REPORT_MOST {
            let _ = writeln!(f, "{LOG_PREFIX} (the report stops here: {REPORT_MOST} bytes)");
            self.file = None;
            return;
        }
        if f.write_all(text.as_bytes()).is_ok() {
            self.written += text.len() as u64;
        }
    }
}

/// A line of the live chat's report into this run's file, opened (and
/// said where on stderr) with the run's first.
fn report_line(line: &str) {
    let Ok(mut r) = REPORT.lock() else { return };
    if r.is_none() {
        let Some(dir) = REPORT_DIR.get().cloned() else {
            return;
        };
        let file = ReportFile::open(&dir);
        eprintln!(
            "{LOG_PREFIX} this run's chat report is also written to {}",
            file.path().display()
        );
        *r = Some(file);
    }
    if let Some(f) = r.as_mut() {
        f.line(line);
    }
}

/// The live chat's setup on desktop: Twitch's EventSub socket (or `url`,
/// a test's), our server at `api`, the providers' addresses from `at`
/// where a test says.
pub fn live_setup(url: String, api: String, at: Option<String>) -> LiveSetup {
    live_setup_with(url, api, at, crate::online::session_token)
}

/// [`live_setup`] signed in with what `token` gives (tests: none).
pub fn live_setup_with(
    url: String,
    api: String,
    at: Option<String>,
    token: fn() -> Option<String>,
) -> LiveSetup {
    let base = api.clone();
    LiveSetup {
        url,
        api,
        at,
        make: Box::new(move || Box::new(DesktopLive::new(base.clone(), token))),
    }
}

/// The live chat's side on desktop (see the module's notes).
pub struct DesktopLive {
    api: String,
    sockets: HashMap<u32, crate::online::PlainSocket>,
    tx: Sender<LiveInput>,
    rx: Receiver<LiveInput>,
    /// The raw frames' file, where recorded.
    record: Option<std::fs::File>,
    began: Instant,
    /// The run's session (our server's subscribe route and proxy).
    token: fn() -> Option<String>,
}

impl DesktopLive {
    pub fn new(api: String, token: fn() -> Option<String>) -> DesktopLive {
        let (tx, rx) = channel();
        let record = RECORD.get().and_then(|dir| {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let path = dir.join(format!("eventsub-{stamp}.jsonl"));
            eprintln!("{LOG_PREFIX} recording this play's socket frames to {}", path.display());
            std::fs::File::create(path).ok()
        });
        DesktopLive {
            api: api.trim_end_matches('/').to_string(),
            sockets: HashMap::new(),
            tx,
            rx,
            record,
            began: Instant::now(),
            token,
        }
    }
}

impl LiveIo for DesktopLive {
    fn open(&mut self, socket: u32, url: &str) {
        self.sockets
            .insert(socket, crate::online::PlainSocket::open(url, "[ddi-chat] socket"));
    }

    fn close(&mut self, socket: u32) {
        self.sockets.remove(&socket);
    }

    fn subscribe(&mut self, socket: u32, session_id: &str, want: &LiveWant, messages: bool) {
        let path = want.route();
        let url = format!("{}{}", self.api, path);
        let body = want.body(session_id, messages);
        let tx = self.tx.clone();
        let token = self.token;
        std::thread::Builder::new()
            .name("ddi-chat-subscribe".into())
            .spawn(move || {
                // The room's other requests' way (`crate::online`).
                let request = ddi_ui::state::online::api::ApiRequest {
                    method: ddi_ui::state::online::api::Method::Post,
                    path,
                    body: Some(body),
                    bearer: token(),
                    poll: None,
                };
                let answer = match crate::online::try_send(&url, &request, Duration::from_secs(8)) {
                    Ok(a) => ddi_chat::live::read_subscribe_answer(a.status, &a.body),
                    Err(e) => ddi_chat::live::SubscribeAnswer::Failed(format!("no answer: {e}")),
                };
                let _ = tx.send(LiveInput::Subscribed(socket, answer));
            })
            .ok();
    }

    fn read_set(&mut self, key: usize, url: &str, via: ddi_chat::cache::Via) {
        let tx = self.tx.clone();
        let url = url.to_string();
        let token = self.token;
        std::thread::Builder::new()
            .name("ddi-chat-set".into())
            .spawn(move || {
                let proxy = via == ddi_chat::cache::Via::Proxy;
                let answer = read_with(&url, proxy.then(token).flatten())
                    .map(|b| String::from_utf8_lossy(&b).into_owned());
                let _ = tx.send(LiveInput::SetRead(key, via, answer));
            })
            .ok();
    }

    fn poll(&mut self) -> Vec<LiveInput> {
        use ddi_room::TransportEvent;
        let mut out: Vec<LiveInput> = self.rx.try_iter().collect();
        let mut gone = Vec::new();
        for (n, s) in &self.sockets {
            for e in s.poll() {
                match e {
                    TransportEvent::Opened => out.push(LiveInput::Opened(*n)),
                    TransportEvent::Text(t) => {
                        if let Some(f) = self.record.as_mut() {
                            use std::io::Write;
                            let line = serde_json::json!({
                                "ms": self.began.elapsed().as_millis() as u64,
                                "socket": n,
                                "frame": t,
                            });
                            let _ = writeln!(f, "{line}");
                        }
                        out.push(LiveInput::Text(*n, t));
                    }
                    TransportEvent::Binary(_) => {}
                    TransportEvent::Closed { code } => {
                        gone.push(*n);
                        out.push(LiveInput::Closed(*n, code));
                    }
                }
            }
        }
        for n in gone {
            self.sockets.remove(&n);
        }
        out
    }

    fn report(&mut self, line: &str) {
        eprintln!("{LOG_PREFIX} {line}");
        report_line(line);
    }
}

/// TESTING ONLY (round TW6), `--mp-dev-chat live`: the signed-in person's
/// own channel (their Twitch user id and login) as our server at `api`
/// says (`GET /me`), and whether their access has the chat's scope; or
/// why not.
pub fn own_channel(api: &str) -> Result<(String, String, bool), String> {
    let token = crate::online::session_token().ok_or("not signed in: sign in first (Multiplayer)")?;
    let mut answer = crate::share::agent()
        .get(format!("{}/me", api.trim_end_matches('/')))
        .header("authorization", format!("Bearer {token}"))
        .call()
        .map_err(|e| format!("GET /me: {e}"))?;
    if !answer.status().is_success() {
        return Err(format!("GET /me: HTTP {}", answer.status().as_u16()));
    }
    let me: ddi_ui::state::online::api::Me =
        serde_json::from_str(&answer.body_mut().read_to_string().unwrap_or_default())
            .map_err(|e| format!("GET /me: {e}"))?;
    let (id, login) = me.own_channel().ok_or("signed in without a Twitch account (a bot?)")?;
    Ok((id, login, me.twitch.has("chat")))
}

/// TESTING ONLY (round TW5): the global-emotes demo's chat
/// (`--mp-dev-chat global`): the providers' global sets read now, under
/// the device's "Emotes from" (`from`), each read on a thread of its own,
/// directly first and through our server's proxy at `api` after a
/// failure; every provider's address served from `at` instead where
/// given (`--mp-dev-chat-at`: a local folder or server of fixtures, never
/// a provider). What each provider answered is printed.
pub fn global_script(from: ddi_chat::EmotesFrom, api: String, at: Option<String>) -> Script {
    let mut demo = ddi_chat::global::Demo::new(from, Some(api), at);
    loop {
        let reads = demo.start();
        if reads.is_empty() {
            break;
        }
        let threads: Vec<_> = reads
            .into_iter()
            .map(|r| {
                std::thread::spawn(move || {
                    let answer = read(&r.url, r.via == ddi_chat::cache::Via::Proxy)
                        .map(|b| String::from_utf8_lossy(&b).into_owned());
                    (r, answer)
                })
            })
            .collect();
        for t in threads {
            let Ok((r, answer)) = t.join() else { continue };
            eprintln!("{}", ddi_chat::global::answer_line(&r, &answer));
            demo.answered(r.provider, r.via, answer);
        }
    }
    eprintln!("chat global emotes: {}", demo.report());
    demo.script()
}

/// An image read ([`anim::probe`]) and decoded to the atlas's row
/// ([`ddi_shell::chat::decode_size`]): every kept frame of an animation
/// within the limits, else its first frame; `None` where it is refused.
fn decode(bytes: &[u8]) -> Option<Decoded> {
    decode_by(bytes, Instant::now() + DEADLINE)
}

fn decode_by(bytes: &[u8], deadline: Instant) -> Option<Decoded> {
    let probe = anim::probe(bytes).ok()?;
    let size = decode_size(probe.size);
    if let Verdict::Animated(kept) = probe.verdict()
        && let Some(d) = animation(bytes, probe.format, &kept, size, deadline)
    {
        return Some(d);
    }
    still(bytes, probe.format, size)
}

fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(anim::MOST_SIDE);
    l.max_image_height = Some(anim::MOST_SIDE);
    l.max_alloc = Some(MOST_ALLOC);
    l
}

fn image_format(f: Format) -> image::ImageFormat {
    match f {
        Format::Png => image::ImageFormat::Png,
        Format::Gif => image::ImageFormat::Gif,
        Format::WebP => image::ImageFormat::WebP,
        Format::Jpeg => image::ImageFormat::Jpeg,
    }
}

/// The first frame (a still image's only one), scaled to `size`.
fn still(bytes: &[u8], format: Format, size: [u32; 2]) -> Option<Decoded> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image_format(format));
    reader.limits(limits());
    let image = reader.decode().ok()?.to_rgba8();
    let scaled = image::imageops::resize(&image, size[0], size[1], image::imageops::FilterType::Triangle);
    Some(Decoded::still(size, scaled.into_raw()))
}

/// The `kept` frames of an animation, each scaled to `size`, by
/// `deadline`; `None` where it cannot be (a decoder's refusal, fewer than
/// two frames, the deadline passed).
fn animation(
    bytes: &[u8],
    format: Format,
    kept: &[Kept],
    size: [u32; 2],
    deadline: Instant,
) -> Option<Decoded> {
    use image::AnimationDecoder;
    use image::ImageDecoder;
    let frames: image::Frames<'_> = match format {
        Format::Gif => {
            let mut d = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
            d.set_limits(limits()).ok()?;
            d.into_frames()
        }
        Format::WebP => {
            let mut d = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).ok()?;
            d.set_limits(limits()).ok()?;
            d.into_frames()
        }
        Format::Png => {
            let mut d = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).ok()?;
            d.set_limits(limits()).ok()?;
            d.apng().ok()?.into_frames()
        }
        Format::Jpeg => return None,
    };
    let last = kept.last()?.index;
    let mut want = kept.iter().peekable();
    let mut rgba = Vec::new();
    let mut durations = Vec::new();
    for (i, frame) in frames.enumerate() {
        if i > last || Instant::now() > deadline {
            break;
        }
        let Ok(frame) = frame else { break };
        if want.peek().is_some_and(|k| k.index == i) {
            let k = want.next()?;
            let scaled = image::imageops::resize(
                frame.buffer(),
                size[0],
                size[1],
                image::imageops::FilterType::Triangle,
            );
            rgba.extend_from_slice(scaled.as_raw());
            durations.push(k.ms);
        }
    }
    if Instant::now() > deadline || durations.len() < 2 {
        return None;
    }
    Some(Decoded {
        size,
        rgba,
        durations,
    })
}

/// TESTING ONLY: the script `--mp-dev-chat` names: a file of one, else a
/// built-in one by name (`demo`, `emotes`, `flood`) or a rate (messages
/// a second).
pub fn load_script(what: &str) -> Result<Script, String> {
    match std::fs::read_to_string(what) {
        Ok(text) => Script::parse(&text).map_err(|e| format!("{what}: {e}")),
        Err(_) => Script::builtin(what)
            .ok_or_else(|| format!("{what}: no such file, built-in script or rate")),
    }
}

/// TESTING ONLY: emote pictures made by code for the scripted chat's walks
/// and the decoders' tests (round TW4), never a provider's: a GIF and an
/// animated WebP of given frames (`image`'s encoders; the WebP's frames
/// its lossless ones put in an animation's chunks), and every picture a
/// script names written to a folder (`examples/chat_images.rs`).
pub mod made {
    use std::io::Cursor;
    use std::path::Path;

    /// An animated GIF of `frames` (RGBA rows of `size`), each `ms` long,
    /// looping.
    pub fn gif(size: [u32; 2], frames: &[Vec<u8>], ms: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = image::codecs::gif::GifEncoder::new(&mut out);
            e.set_repeat(image::codecs::gif::Repeat::Infinite).expect("a GIF");
            for f in frames {
                let buffer = image::RgbaImage::from_raw(size[0], size[1], f.clone()).expect("a frame");
                let delay = image::Delay::from_numer_denom_ms(ms, 1);
                e.encode_frame(image::Frame::from_parts(buffer, 0, 0, delay))
                    .expect("a GIF frame");
            }
        }
        out
    }

    fn u24(v: u32) -> [u8; 3] {
        let b = v.to_le_bytes();
        [b[0], b[1], b[2]]
    }

    /// An animated WebP of `frames` (RGBA rows of `size`), each `ms` long.
    pub fn webp(size: [u32; 2], frames: &[Vec<u8>], ms: u32) -> Vec<u8> {
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = kind.to_vec();
            c.extend((data.len() as u32).to_le_bytes());
            c.extend(data);
            if data.len() % 2 == 1 {
                c.push(0);
            }
            c
        };
        let mut body = b"WEBP".to_vec();
        let mut vp8x = vec![0x02 | 0x10, 0, 0, 0];
        vp8x.extend(u24(size[0] - 1));
        vp8x.extend(u24(size[1] - 1));
        body.extend(chunk(b"VP8X", &vp8x));
        body.extend(chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
        for f in frames {
            let mut one = Vec::new();
            image::codecs::webp::WebPEncoder::new_lossless(Cursor::new(&mut one))
                .encode(f, size[0], size[1], image::ExtendedColorType::Rgba8)
                .expect("a WebP frame");
            // Its VP8L chunk, whole.
            let mut at = 12;
            let mut vp8l = None;
            while at + 8 <= one.len() {
                let len = u32::from_le_bytes(one[at + 4..at + 8].try_into().unwrap()) as usize;
                if &one[at..at + 4] == b"VP8L" {
                    vp8l = Some(chunk(b"VP8L", &one[at + 8..at + 8 + len]));
                }
                at += 8 + len + (len & 1);
            }
            let mut anmf = Vec::new();
            anmf.extend(u24(0));
            anmf.extend(u24(0));
            anmf.extend(u24(size[0] - 1));
            anmf.extend(u24(size[1] - 1));
            anmf.extend(u24(ms));
            // Not blended over the frame before.
            anmf.push(0x02);
            anmf.extend(vp8l.expect("a lossless frame"));
            body.extend(chunk(b"ANMF", &anmf));
        }
        let mut out = b"RIFF".to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    /// A PNG of `rgba` rows of `size`.
    pub fn png(size: [u32; 2], rgba: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_raw(size[0], size[1], rgba)
            .expect("an image")
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("a PNG");
        out
    }

    /// The global-emotes demo's fixtures (round TW5), as the providers'
    /// hosts would serve them, under `dir` (`<dir><host>/<path>`, the
    /// proxy's under `<dir>api/`): the four global sets written from the
    /// providers' documentation (`crates/chat/tests/fixtures/`, never
    /// recorded) and a picture made by code for each of their emotes (an
    /// animated one a GIF or an animated WebP as its provider serves it).
    /// BTTV's set and images are only behind the proxy here, so that a
    /// walk sees a host need it; Twitch's set is only ever there. The
    /// files written.
    pub fn global(dir: &Path) -> std::io::Result<Vec<String>> {
        use ddi_chat::emotes::Provider;
        const SETS: [(Provider, &str); 4] = [
            (Provider::Twitch, include_str!("../../chat/tests/fixtures/twitch-global.json")),
            (Provider::SevenTv, include_str!("../../chat/tests/fixtures/demo-7tv-global.json")),
            (Provider::Bttv, include_str!("../../chat/tests/fixtures/demo-bttv-global.json")),
            (Provider::Ffz, include_str!("../../chat/tests/fixtures/demo-ffz-global.json")),
        ];
        let mut out = Vec::new();
        let mut put = |rel: String, bytes: &[u8]| -> std::io::Result<()> {
            let path = dir.join(&rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
            out.push(rel);
            Ok(())
        };
        let host_path = |url: &str| url.split_once("://").map_or(url, |(_, r)| r).to_string();
        for (k, (provider, text)) in SETS.into_iter().enumerate() {
            let at = ddi_chat::emotes::global_address(provider).expect("a set");
            let behind_proxy = matches!(provider, Provider::Twitch | Provider::Bttv);
            if behind_proxy {
                put(format!("api/{}", at.proxy.clone().unwrap()), text.as_bytes())?;
            } else {
                put(host_path(&at.url), text.as_bytes())?;
            }
            let list = match provider {
                Provider::Twitch => ddi_chat::emotes::twitch_global(text),
                Provider::SevenTv => ddi_chat::emotes::seven_tv_global(text),
                Provider::Bttv => ddi_chat::emotes::bttv_global(text),
                _ => ddi_chat::emotes::ffz_global(text),
            };
            let kind = ["circle", "triangle", "square", "diamond"][k];
            for (n, (_, emote, image)) in list.iter().enumerate() {
                // A color an emote, and now and then a wide one.
                let h = n as u32 * 47 + k as u32 * 101;
                let rgb = format!("{:02x}{:02x}{:02x}", 80 + (h * 37) % 170, 80 + (h * 91) % 170, 80 + (h * 53) % 170);
                let (shape, aspect) = if n % 5 == 4 { ("bar", ":2") } else { (kind, "") };
                let bytes = if emote.animated {
                    let spec = format!("anim:{shape}:{rgb}:{}:{}{aspect}", 8 + n % 9, 60 + 10 * (n % 5));
                    let (size, rgba, durations) = ddi_chat::script::anim_pixels(&spec).expect("frames");
                    let len = (size[0] * size[1] * 4) as usize;
                    let frames: Vec<Vec<u8>> = rgba.chunks(len).map(<[u8]>::to_vec).collect();
                    let ms = durations[0];
                    match provider {
                        Provider::Twitch | Provider::Bttv => gif(size, &frames, ms),
                        _ => webp(size, &frames, ms),
                    }
                } else {
                    let (size, rgba) =
                        ddi_chat::script::shape_pixels(&format!("shape:{shape}:{rgb}{aspect}")).expect("a shape");
                    png(size, rgba)
                };
                if behind_proxy {
                    put(format!("api/{}", image.proxy.clone().unwrap()), &bytes)?;
                } else {
                    put(host_path(&image.url), &bytes)?;
                }
            }
        }
        Ok(out)
    }

    /// Writes every picture `script` names into `dir`: a still one's
    /// `<id>.png`, an animated one's `<id>.gif` and `<id>.webp` (the
    /// scripted chat's `images_at`, the web's `mpfakechatimg`). The files
    /// written.
    pub fn write(script: &ddi_chat::script::Script, dir: &Path) -> std::io::Result<Vec<String>> {
        std::fs::create_dir_all(dir)?;
        let mut out = Vec::new();
        for (id, animated, spec) in script.emote_specs() {
            if animated {
                let Some((size, rgba, durations)) = ddi_chat::script::anim_pixels(&spec) else {
                    continue;
                };
                let n = (size[0] * size[1] * 4) as usize;
                let frames: Vec<Vec<u8>> = rgba.chunks(n).map(<[u8]>::to_vec).collect();
                let ms = durations.first().copied().unwrap_or(100);
                for (kind, bytes) in [("gif", gif(size, &frames, ms)), ("webp", webp(size, &frames, ms))] {
                    let name = format!("{id}.{kind}");
                    std::fs::write(dir.join(&name), bytes)?;
                    out.push(name);
                }
            } else if let Some((size, rgba)) = ddi_chat::script::shape_pixels(&spec) {
                let name = format!("{id}.png");
                std::fs::write(dir.join(&name), png(size, rgba))?;
                out.push(name);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_is_decoded_to_the_atlas_row() {
        let mut png = Vec::new();
        let image = image::RgbaImage::from_pixel(28, 14, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let d = decode(&png).unwrap();
        assert_eq!(d.size, [128, 64]);
        assert_eq!(d.rgba.len(), 128 * 64 * 4);
        assert_eq!(&d.rgba[..4], [255, 0, 0, 255]);
    }

    #[test]
    fn scripts_load_from_files_names_and_rates() {
        assert!(load_script("demo").is_ok());
        assert!(load_script("30").is_ok());
        assert!(load_script("/no/such/script").is_err());
    }

    /// Frames that differ: a color a frame.
    fn frames(size: [u32; 2], n: usize) -> Vec<Vec<u8>> {
        (0..n)
            .map(|k| {
                let c = [(40 * k % 256) as u8, (200 - 20 * (k % 10)) as u8, 90, 255];
                c.iter().copied().cycle().take((size[0] * size[1] * 4) as usize).collect()
            })
            .collect()
    }

    /// Round TW4: a GIF's and an animated WebP's frames decoded to the
    /// atlas's row, their durations the files' own.
    #[test]
    fn animated_gifs_and_webps_are_decoded_frame_by_frame() {
        for (name, bytes) in [
            ("gif", made::gif([28, 28], &frames([28, 28], 6), 80)),
            ("webp", made::webp([28, 28], &frames([28, 28], 6), 80)),
        ] {
            let d = decode(&bytes).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(d.size, [64, 64], "{name}");
            assert_eq!(d.durations, vec![80; 6], "{name}");
            assert_eq!(d.rgba.len(), 6 * 64 * 64 * 4, "{name}");
            assert_ne!(d.frame(0), d.frame(1), "{name}: frames differ");
            let px = d.frame(2).unwrap();
            assert!(px[0].abs_diff(80) < 8 && px[1].abs_diff(160) < 8, "{name}: {:?}", &px[..4]);
        }
        // A long one thinned to 64 frames, its loop as long.
        let long = made::gif([16, 16], &frames([16, 16], 130), 20);
        let d = decode(&long).unwrap();
        assert!(d.durations.len() <= 64, "{}", d.durations.len());
        assert_eq!(d.durations.iter().sum::<u32>(), 130 * 20);
    }

    /// The hostile and the huge: refused, or their first frame still,
    /// before their frames are decoded.
    #[test]
    fn hostile_images_are_refused_or_still() {
        use ddi_chat::anim::test_gif;
        // Enormous sides (a GIF's 65535 × 65535 in a few bytes, a PNG
        // whose header claims 30000 × 30000): never decoded.
        let t = Instant::now();
        assert!(decode(&test_gif([65535, 65535], 1, 0)).is_none());
        let mut png = made::png([4, 4], vec![255; 64]);
        png[16..20].copy_from_slice(&30000u32.to_be_bytes());
        png[20..24].copy_from_slice(&30000u32.to_be_bytes());
        assert!(decode(&png).is_none());
        // Thousands of frames: their first frame alone, still.
        let many = test_gif([16, 16], 5000, 4);
        if let Some(d) = decode(&many) {
            assert_eq!(d.durations.len(), 1);
        }
        // More bytes than an emote's, and not an image at all.
        assert!(decode(&vec![0x47; (2 << 20) + 1]).is_none());
        assert!(decode(b"<html>not an image</html>").is_none());
        // A tiny canvas with huge frames (100 of 1024 × 1024 on 1 × 1):
        // the frames' own areas count, their first frame alone.
        let mut tiny = test_gif([1024, 1024], 100, 4);
        tiny[6..10].copy_from_slice(&[1, 0, 1, 0]);
        if let Some(d) = decode(&tiny) {
            assert_eq!(d.durations.len(), 1);
        }
        // An animated WebP whose frame's own bitstream claims 16383 ×
        // 16383: refused before a decoder sizes a buffer by it.
        let mut liar = made::webp([28, 28], &frames([28, 28], 3), 80);
        let at = liar.windows(4).rposition(|w| w == b"VP8L").unwrap();
        liar[at + 9..at + 13].copy_from_slice(&(16382u32 | (16382 << 14)).to_le_bytes());
        assert!(decode(&liar).is_none());
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
        // An animation past its deadline: its first frame, still.
        let gif = made::gif([28, 28], &frames([28, 28], 6), 80);
        let d = decode_by(&gif, Instant::now()).unwrap();
        assert_eq!(d.durations.len(), 1);
    }

    /// Round TW5: the global-emotes demo read from its fixtures as files
    /// (never a provider): every set read, BTTV's and Twitch's through
    /// the proxy's paths, and every chosen emote's picture where the
    /// cache will ask for it.
    #[test]
    fn the_global_demo_reads_its_fixtures() {
        let dir = std::env::temp_dir().join(format!("ddi-chat-global-{}", std::process::id()));
        made::global(&dir).unwrap();
        let at = format!("{}/", dir.display());
        let script = global_script(ddi_chat::EmotesFrom::TwitchAndOthers, "https://unused.test".into(), Some(at.clone()));
        let emotes = script.emotes();
        assert_eq!(emotes.len(), 5 + 9 + 9 + 9);
        assert_eq!(script.api(), Some(format!("{at}api")));
        for (word, emote, image) in &emotes {
            let direct = read(&image.url, false);
            let proxied = read(&image.proxied(&format!("{at}api")).unwrap(), true);
            let bytes = direct.or(proxied).unwrap_or_else(|e| panic!("{word}: {e}"));
            let d = decode(&bytes).unwrap_or_else(|| panic!("{word}"));
            assert_eq!(d.durations.len() > 1, emote.animated, "{word}");
        }
        // "Emotes from: Twitch": its emotes alone.
        let script = global_script(ddi_chat::EmotesFrom::TwitchOnly, "https://unused.test".into(), Some(at));
        assert!(script.emotes().iter().all(|(_, e, _)| e.provider == ddi_chat::emotes::Provider::Twitch));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The scripted chat's pictures written as files (the walks' images).
    #[test]
    fn the_scripted_chats_pictures_are_written_as_files() {
        let dir = std::env::temp_dir().join(format!("ddi-chat-images-{}", std::process::id()));
        let names = made::write(&Script::builtin("demo").unwrap(), &dir).unwrap();
        assert!(names.iter().any(|n| n == "25.png"));
        assert!(names.iter().any(|n| n == "c2.gif") && names.iter().any(|n| n == "c2.webp"));
        let gif = std::fs::read(dir.join("c2.gif")).unwrap();
        assert_eq!(decode(&gif).unwrap().durations, vec![70; 12]);
        let webp = std::fs::read(dir.join("c2.webp")).unwrap();
        assert_eq!(decode(&webp).unwrap().durations, vec![70; 12]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A local fake of Twitch's EventSub socket (round TW6, never
    /// Twitch's): the first connection welcomes as `s1`, chats, then asks
    /// for a reconnect to the second, which welcomes as `s2` and chats;
    /// each waits for the client to close it. Its address, and the
    /// closes it saw.
    fn fake_eventsub() -> (String, std::sync::Arc<std::sync::Mutex<Vec<usize>>>) {
        use tungstenite::Message;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let closed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = closed.clone();
        std::thread::spawn(move || {
            for n in 0..2usize {
                let Ok((stream, _)) = listener.accept() else { return };
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let mut ws = tungstenite::accept(stream).unwrap();
                    let session = format!("s{}", n + 1);
                    let welcome = format!(
                        r#"{{"metadata":{{"message_id":"w{n}","message_type":"session_welcome"}},"payload":{{"session":{{"id":"{session}","status":"connected","keepalive_timeout_seconds":10,"reconnect_url":null}}}}}}"#
                    );
                    ws.send(Message::text(welcome)).unwrap();
                    for k in 0..10 {
                        std::thread::sleep(Duration::from_millis(60));
                        let chat = format!(
                            r#"{{"metadata":{{"message_id":"n{n}-{k}","message_type":"notification","subscription_type":"channel.chat.message"}},"payload":{{"event":{{"broadcaster_user_id":"1001","chatter_user_id":"{k}","chatter_user_login":"v","chatter_user_name":"V","message_id":"m{n}-{k}","message_type":"text","message":{{"text":"hi Kappa","fragments":[{{"type":"text","text":"hi "}},{{"type":"emote","text":"Kappa","emote":{{"id":"25","format":["static"]}}}}]}}}}}}}}"#
                        );
                        if ws.send(Message::text(chat)).is_err() {
                            return;
                        }
                    }
                    if n == 0 {
                        let reconnect = format!(
                            r#"{{"metadata":{{"message_id":"r","message_type":"session_reconnect"}},"payload":{{"session":{{"id":"s1","status":"reconnecting","reconnect_url":"ws://127.0.0.1:{port}/second"}}}}}}"#
                        );
                        ws.send(Message::text(reconnect)).unwrap();
                    }
                    loop {
                        match ws.read() {
                            Ok(Message::Close(_)) | Err(_) => {
                                seen.lock().unwrap().push(n);
                                return;
                            }
                            _ => {}
                        }
                    }
                });
            }
        });
        (format!("ws://127.0.0.1:{port}/ws"), closed)
    }

    /// A local fake of our server's subscribe route and the providers'
    /// sets under `/g/` (the global ones from the fixtures, a channel's
    /// 404): what it was asked.
    fn fake_http() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let path = first.split(' ').nth(1).unwrap_or("").to_string();
                log.lock().unwrap().push(format!("{path} {}", String::from_utf8_lossy(&body)));
                let fixture = |name: &str| {
                    std::fs::read_to_string(format!("{}/../chat/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
                };
                let (status, answer) = if path == "/me/chat/subscribe" {
                    (200, r#"{"channel":"me","subscriptions":[{"type":"channel.chat.message","outcome":"enabled","http":202,"status":"enabled","cost":0,"total":1,"total_cost":0,"max_total_cost":10,"ms":3}]}"#.to_string())
                } else if path.ends_with("/v3/emote-sets/global") {
                    (200, fixture("7tv-global.json"))
                } else if path.ends_with("/3/cached/emotes/global") {
                    (200, fixture("bttv-global.json"))
                } else if path.ends_with("/v1/set/global") {
                    (200, fixture("ffz-global.json"))
                } else {
                    (404, "{}".to_string())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
            }
        });
        (base, asked)
    }

    /// Round TW6: desktop's side of the live chat against a local fake
    /// socket and server (never Twitch): the socket opened, the welcome's
    /// session subscribed through the route (no session sent: the test
    /// has none), the chat taken, Twitch's reconnect followed and the old
    /// socket closed after the new one's welcome, the sets read, and the
    /// play's end closing the rest.
    #[test]
    fn the_live_chat_runs_over_desktops_sockets() {
        let (socket, closed) = fake_eventsub();
        let (http, asked) = fake_http();
        ddi_session::chat::set_scripted(None);
        ddi_session::chat::set_live(Some(live_setup_with(
            socket,
            http.clone(),
            Some(format!("{http}/g/")),
            || None,
        )));
        ddi_ui::state::twitch::set_dev_live(Some(ddi_ui::state::twitch::DevLive::Own {
            channel_id: "1001".into(),
            login: "me".into(),
        }));
        // Round TW7: the run's report also written to a file.
        let reports = std::env::temp_dir().join(format!("ddi-chat-report-{}", std::process::id()));
        report_into(reports.clone());
        let config = ddi_session::chat::ChatConfig::for_play(&ddi_ui::state::settings::Settings {
            twitch_chat: ddi_chat::Level::Messages,
            ..Default::default()
        })
        .unwrap()
        .under_room(None)
        .unwrap();
        let mut chat = ddi_session::chat::ChatLayer::new(config);
        let began = Instant::now();
        let now = || began.elapsed().as_secs_f64() + 100.0;
        let done = |c: &ddi_session::chat::ChatLayer| {
            let r = c.live_report().unwrap();
            r["connection"]["reconnects"]["done"] == 1 && c.counts().0 >= 20
        };
        while began.elapsed() < Duration::from_secs(10) && !done(&chat) {
            chat.advance(now());
            std::thread::sleep(Duration::from_millis(15));
        }
        let r = chat.live_report().unwrap();
        assert!(done(&chat), "{r:#}");
        assert_eq!(r["connection"]["state"], "live");
        assert_eq!(r["connection"]["subscriptions"][0]["max_total_cost"], 10);
        assert_eq!(r["connection"]["received"]["duplicate"], 0);
        // Subscribed once (the reconnect carried it over), messages too.
        let asked_now = asked.lock().unwrap().clone();
        let subscribes: Vec<&String> = asked_now.iter().filter(|a| a.starts_with("/me/chat/subscribe")).collect();
        assert_eq!(subscribes.len(), 1, "{asked_now:?}");
        assert!(subscribes[0].contains(r#""session_id":"s1""#) && subscribes[0].contains(r#""messages":true"#));
        assert!(asked_now.iter().any(|a| a.starts_with("/g/7tv.io/v3/users/twitch/1001")));
        // The first socket closed once the second welcomed.
        let t = Instant::now();
        while closed.lock().unwrap().is_empty() && t.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(*closed.lock().unwrap(), [0]);
        // The words of the global sets are matched.
        let asking = |c: &ddi_session::chat::ChatLayer| {
            c.live_report().unwrap()["emote_sets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["state"] == "asked" || s["state"] == "waiting")
        };
        while asking(&chat) && began.elapsed() < Duration::from_secs(10) {
            chat.advance(now());
            std::thread::sleep(Duration::from_millis(15));
        }
        assert!(!chat.sets().is_empty());
        let sets = chat.live_report().unwrap()["emote_sets"].clone();
        assert_eq!(sets[0]["state"], "no set", "{sets:#}");
        assert_eq!(sets[1]["state"], "read", "{sets:#}");
        drop(chat);
        let t = Instant::now();
        while closed.lock().unwrap().len() < 2 && t.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(*closed.lock().unwrap(), [0, 1], "the play's end closed the second");
        // The report's lines in the run's file, as stderr has them: the
        // channel first, the connection, the end's counts; never the
        // socket's session or a chatter's words.
        let file = reports.join(REPORT_FILE);
        if REPORT_DIR.get() == Some(&reports) {
            let text = std::fs::read_to_string(&file).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            assert!(lines.iter().all(|l| l.starts_with("[ddi-chat] ")), "{text}");
            assert!(lines[0].ends_with("channel me (your own), route /me/chat/subscribe"), "{text}");
            assert!(text.contains("live, subscribed") && text.contains("received "), "{text}");
            assert!(!text.contains("hi Kappa") && !text.contains("s1") && !text.contains("s2"), "{text}");
        }
        let _ = std::fs::remove_dir_all(&reports);
        ddi_ui::state::twitch::set_dev_live(None);
        ddi_session::chat::set_live(None);
    }

    /// Round TW7: a report file is emptied when opened, and stops with a
    /// line saying so at its size.
    #[test]
    fn the_report_file_is_replaced_and_bounded() {
        let dir = std::env::temp_dir().join(format!("ddi-chat-report-bound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(REPORT_FILE), "an earlier run's").unwrap();
        let mut f = ReportFile::open(&dir);
        f.line("first");
        assert_eq!(std::fs::read_to_string(f.path()).unwrap(), "[ddi-chat] first\n");
        let long = "x".repeat(4096);
        for _ in 0..400 {
            f.line(&long);
        }
        let text = std::fs::read_to_string(f.path()).unwrap();
        assert!(text.len() as u64 <= REPORT_MOST + 200);
        assert!(text.ends_with("(the report stops here: 1048576 bytes)\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
