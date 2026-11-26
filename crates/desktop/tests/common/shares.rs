//! A fake of the server's shares for tests (TESTING ONLY): HTTP on a
//! loopback port of this process, as much of `docs/plans/shared-songs.md`'s
//! routes as an online room's song upload and a member's download use
//! (`POST <base>/s`, each file `PUT`, `POST <base>/s/<id>/finish`,
//! `GET <base>/s/<id>` and its files), every file's bytes kept as sent,
//! with the `Authorization` each share was made under. Nothing leaves the
//! machine.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// A share as made.
#[derive(Clone, Debug, Default)]
pub struct Made {
    /// The share's JSON as posted (`NewShare`).
    pub body: serde_json::Value,
    /// Its `Authorization`.
    pub auth: String,
    /// Each file's bytes, by the path the share keeps it at, song by song.
    pub files: Vec<BTreeMap<String, Vec<u8>>>,
    pub finished: bool,
}

#[derive(Default)]
struct State {
    shares: BTreeMap<String, Made>,
    next: u32,
}

/// The fake's handle: its base URL and what it was sent.
#[derive(Clone)]
pub struct FakeShares {
    pub base: String,
    state: Arc<Mutex<State>>,
}

impl FakeShares {
    /// Listening on a port of its own, served on a thread.
    pub fn start() -> FakeShares {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let base = format!("http://{}/api", listener.local_addr().unwrap());
        let state: Arc<Mutex<State>> = Arc::default();
        let s = state.clone();
        std::thread::Builder::new()
            .name("fake-shares".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let s = s.clone();
                    std::thread::spawn(move || {
                        let _ = serve(stream, &s);
                    });
                }
            })
            .unwrap();
        FakeShares { base, state }
    }

    /// The shares made, by id.
    pub fn made(&self) -> BTreeMap<String, Made> {
        self.state.lock().unwrap().shares.clone()
    }
}

fn serve(stream: TcpStream, state: &Mutex<State>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let (method, path) = (
            parts.next().unwrap_or_default().to_string(),
            parts.next().unwrap_or_default().to_string(),
        );
        let mut length = 0usize;
        let mut auth = String::new();
        loop {
            let mut h = String::new();
            reader.read_line(&mut h)?;
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((name, value)) = h.split_once(':') {
                match name.trim().to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse().unwrap_or(0),
                    "authorization" => auth = value.trim().to_string(),
                    _ => {}
                }
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        let (status, answer) = answer(state, &method, &path, &auth, body);
        write!(
            out,
            "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
            answer.len()
        )?;
        out.write_all(&answer)?;
        out.flush()?;
    }
}

fn answer(
    state: &Mutex<State>,
    method: &str,
    path: &str,
    auth: &str,
    body: Vec<u8>,
) -> (u16, Vec<u8>) {
    let Some(rest) = path.strip_prefix("/api/s") else {
        return (404, b"{}".to_vec());
    };
    let mut st = state.lock().unwrap();
    match (method, rest) {
        ("POST", "") => {
            st.next += 1;
            let id = format!("Fake{:04}", st.next);
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let songs = body["songs"].as_array().map_or(0, Vec::len);
            st.shares.insert(
                id.clone(),
                Made {
                    body,
                    auth: auth.to_string(),
                    files: vec![BTreeMap::new(); songs],
                    finished: false,
                },
            );
            (200, format!(r#"{{"id":"{id}"}}"#).into_bytes())
        }
        _ => {
            let rest = rest.trim_start_matches('/');
            let mut bits = rest.splitn(4, '/');
            let id = bits.next().unwrap_or_default().to_string();
            let Some(made) = st.shares.get_mut(&id) else {
                return (404, b"{}".to_vec());
            };
            match (method, bits.next(), bits.next(), bits.next()) {
                ("POST", Some("finish"), None, None) => {
                    made.finished = true;
                    (200, b"{}".to_vec())
                }
                ("PUT", Some("f"), Some(n), Some(file)) => {
                    let n: usize = n.parse().unwrap_or(usize::MAX);
                    let file = ddi_library::share::decode_path(file).unwrap_or_default();
                    match made.files.get_mut(n) {
                        Some(files) => {
                            files.insert(file, body);
                            (200, b"{}".to_vec())
                        }
                        None => (404, b"{}".to_vec()),
                    }
                }
                ("GET", Some("f"), Some(n), Some(file)) => {
                    let n: usize = n.parse().unwrap_or(usize::MAX);
                    let file = ddi_library::share::decode_path(file).unwrap_or_default();
                    match made.files.get(n).and_then(|f| f.get(&file)) {
                        Some(bytes) if made.finished => (200, bytes.clone()),
                        _ => (404, b"{}".to_vec()),
                    }
                }
                ("GET", None, None, None) if made.finished => {
                    let mut songs = made.body["songs"].clone();
                    if let Some(list) = songs.as_array_mut() {
                        for (n, song) in list.iter_mut().enumerate() {
                            if let Some(files) = song["files"].as_array_mut() {
                                for f in files {
                                    let p = f["path"].as_str().unwrap_or_default().to_string();
                                    f["url"] = ddi_library::share::file_path(&id, n, &p).into();
                                }
                            }
                        }
                    }
                    let doc = serde_json::json!({
                        "pack": made.body["pack"],
                        "expires_at": null,
                        "created_at": 0,
                        "songs": songs,
                        "pack_art": null,
                    });
                    (200, doc.to_string().into_bytes())
                }
                _ => (404, b"{}".to_vec()),
            }
        }
    }
}
