//! A room host's chat on desktop (`docs/plans/multiplayer-twitch.md`,
//! part C): its emote images fetched and decoded off the frame, each on a
//! thread of its own (four at most at once, the chat's cache says), and,
//! TESTING ONLY, the scripted chat `--mp-dev-chat` plays
//! (`ddi_chat::script`).
//!
//! No request reaches Twitch or an emote provider in this round: the
//! scripted chat's images are its own shapes (drawn by `ddi_shell::chat`)
//! or files and addresses its script names (a local test server's).

use std::sync::mpsc::{Receiver, Sender, channel};

use ddi_chat::cache::Fetch;
use ddi_chat::script::Script;
use ddi_shell::chat::{Decoded, decode_size};

/// Images a fetch reads at most (an emote's still image is a few
/// kilobytes).
const MOST_BYTES: u64 = 2 << 20;

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
                let decoded = read(&f.url).and_then(|bytes| decode(&bytes));
                let _ = tx.send((f, decoded));
            })
            .is_ok()
    }

    /// The fetches that ended since.
    pub fn take(&mut self) -> Vec<(Fetch, Option<Decoded>)> {
        self.rx.try_iter().collect()
    }
}

fn read(url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut answer = crate::share::agent().get(url).call().ok()?;
        if !answer.status().is_success() {
            return None;
        }
        answer
            .body_mut()
            .with_config()
            .limit(MOST_BYTES)
            .read_to_vec()
            .ok()
    } else {
        let path = url.strip_prefix("file://").unwrap_or(url);
        let meta = std::fs::metadata(path).ok()?;
        (meta.len() <= MOST_BYTES).then(|| std::fs::read(path).ok())?
    }
}

/// An image decoded and scaled to the atlas's row
/// ([`ddi_shell::chat::decode_size`]), its first frame where it moves.
fn decode(bytes: &[u8]) -> Option<Decoded> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let [w, h] = decode_size([image.width(), image.height()]);
    let scaled = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
    Some(Decoded {
        size: [w, h],
        rgba: scaled.into_raw(),
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
}
