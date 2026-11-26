//! Plugins that misbehave, for the host's tests: one program, its way of
//! misbehaving the name it is installed under (its executable's file
//! stem). Written on the protocol crate directly, not the SDK, which
//! would not let them.
//!
//! | name | what it does |
//! |---|---|
//! | `hang-hello` | never answers the Hello |
//! | `hang-search` | answers nothing it is asked |
//! | `ignore-cancel` | answers nothing, cancels included |
//! | `crash` | floods its diagnostics, then exits with 101 when asked anything |
//! | `crash-mid-fetch` | sends a fetch's first chunk, then exits |
//! | `flood-progress` | answers a search with progress, forever |
//! | `flood-results` | answers a search with page after page |
//! | `unknown-op` | answers for an operation never asked |
//! | `oversized` | announces a 64 MiB frame |
//! | `malformed` | sends a frame that is not JSON |
//! | `garbage` | writes a log line to its output |
//! | `truncated` | sends half a frame and exits |
//! | `newer` | speaks only protocol 99 |
//! | `wrong-id` | says it is another plugin |
//! | `lie-size` | fetches 4 bytes and says it sent 2 files |
//! | `hostile-path` | fetches a file named `../../escape.ssc` |
//! | `hostile-content` | fetches files that are not a song |
//! | `huge-fetch` | streams a 1 GiB file until cancelled |
//! | `slow-exit` | says Bye and does not exit |
//! | `orphan` | leaves a child holding its pipes, then exits (Unix) |
//! | `exit-quietly` | exits after its Hello with nothing running |

use std::io::{BufReader, Write};
use std::sync::mpsc;
use std::time::Duration;

use ddi_source_protocol::frame;
use ddi_source_protocol::*;

fn forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn write(out: &mut impl Write, m: &PluginMessage, attachment: &[u8]) {
    if frame::write(out, m, attachment).is_err() {
        std::process::exit(0);
    }
}

fn main() {
    let mode = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let mode = mode.as_str();
    let mut input = BufReader::new(std::io::stdin());
    let mut out = std::io::stdout().lock();
    if mode == "hang-hello" {
        forever();
    }
    let Ok(Some(AppMessage::Hello { .. })) = frame::read_app(&mut input) else {
        std::process::exit(2);
    };
    match mode {
        "garbage" => {
            let _ = out.write_all(b"starting up, version 1.0\n");
            let _ = out.flush();
            forever();
        }
        "oversized" => {
            let _ = out.write_all(&(64u32 << 20).to_be_bytes());
            let _ = out.write_all(&[0; 16]);
            let _ = out.flush();
            forever();
        }
        "malformed" => {
            let bytes = frame::encode_frame(b"{this is not json", &[]).unwrap();
            let _ = out.write_all(&bytes);
            let _ = out.flush();
            forever();
        }
        "truncated" => {
            let bytes = frame::encode(&PluginMessage::Bye {}, &[]).unwrap();
            let _ = out.write_all(&bytes[..bytes.len() / 2]);
            let _ = out.flush();
            std::process::exit(0);
        }
        _ => {}
    }
    let hello = PluginMessage::Hello {
        protocol: if mode == "newer" { 99 } else { 1 },
        features: vec![],
        plugin: PluginInfo {
            id: if mode == "wrong-id" {
                "someone-else".into()
            } else {
                mode.into()
            },
            version: "1.0.0".into(),
        },
        capabilities: vec![Capability::Search, Capability::Metadata, Capability::Fetch],
        inputs: vec![],
        filters: vec![],
    };
    write(&mut out, &hello, &[]);
    if mode == "newer" {
        std::process::exit(0);
    }
    if mode == "exit-quietly" {
        std::process::exit(0);
    }
    // What the game sends, read on a thread so a stream can look for a
    // cancel between its chunks.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(Some(m)) = frame::read_app(&mut input) {
            if tx.send(m).is_err() {
                break;
            }
        }
        std::process::exit(0);
    });
    let item = |n: u32| ItemMeta {
        id: format!("song-{n}"),
        title: format!("Song {n}"),
        ..ItemMeta::default()
    };
    while let Ok(message) = rx.recv() {
        match message {
            AppMessage::Shutdown {} => {
                write(&mut out, &PluginMessage::Bye {}, &[]);
                if mode == "slow-exit" {
                    forever();
                }
                std::process::exit(0);
            }
            AppMessage::Cancel { op } => {
                if mode != "ignore-cancel" {
                    let failed = PluginMessage::Failed {
                        op,
                        code: FailCode::Cancelled,
                        detail: None,
                    };
                    write(&mut out, &failed, &[]);
                }
            }
            AppMessage::Search { op, .. } | AppMessage::Describe { op, .. } => match mode {
                "hang-search" | "ignore-cancel" => {}
                "crash" => crash(),
                "orphan" => orphan(),
                "flood-progress" => loop {
                    let p = PluginMessage::Progress {
                        op,
                        step: Step::Searching,
                        done: None,
                        total: None,
                        bytes: None,
                    };
                    write(&mut out, &p, &[]);
                },
                "flood-results" => loop {
                    let page = PluginMessage::Results {
                        op,
                        items: (1..=50).map(item).collect(),
                    };
                    write(&mut out, &page, &[]);
                },
                "unknown-op" => {
                    let r = PluginMessage::Results {
                        op: op + 1000,
                        items: vec![item(1)],
                    };
                    write(&mut out, &r, &[]);
                }
                _ => {
                    write(
                        &mut out,
                        &PluginMessage::Results {
                            op,
                            items: vec![item(1)],
                        },
                        &[],
                    );
                    write(
                        &mut out,
                        &PluginMessage::Done {
                            op,
                            cursor: None,
                            total: Some(1),
                        },
                        &[],
                    );
                }
            },
            AppMessage::Fetch { op, .. } => {
                let file = |path: &str, size: u64, offset: u64, last: bool| PluginMessage::File {
                    op,
                    path: path.into(),
                    size,
                    offset,
                    last,
                };
                match mode {
                    "crash" => crash(),
                    "crash-mid-fetch" => {
                        write(&mut out, &file("a/song.ssc", 100, 0, false), &[b'#'; 10]);
                        std::process::exit(3);
                    }
                    "lie-size" => {
                        write(&mut out, &file("a/song.ssc", 4, 0, true), b"abcd");
                        let fetched = PluginMessage::Fetched {
                            op,
                            delivery: Delivery::Files { files: 2, bytes: 4 },
                        };
                        write(&mut out, &fetched, &[]);
                    }
                    "hostile-path" => {
                        write(&mut out, &file("../../escape.ssc", 1, 0, true), b"x");
                    }
                    "hostile-content" => {
                        let junk: Vec<u8> = (0..4096u32).map(|i| (i * 7919 % 251) as u8).collect();
                        let notes = "#TITLE:\u{202E}x;#NOTES:\n".to_string() + &"9".repeat(4000);
                        write(&mut out, &file("Bad/song.ssc", notes.len() as u64, 0, true), notes.as_bytes());
                        write(&mut out, &file("Bad/song.ogg", junk.len() as u64, 0, true), &junk);
                        write(&mut out, &file("Worse/x.sm", junk.len() as u64, 0, true), &junk);
                        let fetched = PluginMessage::Fetched {
                            op,
                            delivery: Delivery::Files {
                                files: 3,
                                bytes: notes.len() as u64 + 2 * junk.len() as u64,
                            },
                        };
                        write(&mut out, &fetched, &[]);
                    }
                    "huge-fetch" => {
                        let chunk = vec![0u8; 1 << 20];
                        let size = 1u64 << 30;
                        let mut at = 0;
                        loop {
                            if let Ok(AppMessage::Cancel { op: c }) = rx.try_recv()
                                && c == op
                            {
                                let failed = PluginMessage::Failed {
                                    op,
                                    code: FailCode::Cancelled,
                                    detail: None,
                                };
                                write(&mut out, &failed, &[]);
                                break;
                            }
                            write(&mut out, &file("Huge/song.ogg", size, at, false), &chunk);
                            at += chunk.len() as u64;
                        }
                    }
                    _ => {}
                }
            }
            AppMessage::Open { .. } | AppMessage::Browse { .. } | AppMessage::Hello { .. } => {}
        }
    }
}

fn crash() -> ! {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(&vec![b'X'; 100 << 10]);
    let _ = err.write_all(b"\nboom: crashed on purpose\n");
    let _ = err.flush();
    std::process::exit(101);
}

/// Leaves a child holding the pipes (Unix's `sleep`, which inherits
/// them), then exits.
fn orphan() -> ! {
    #[cfg(unix)]
    let _ = std::process::Command::new("sleep").arg("6").spawn();
    std::process::exit(4);
}
