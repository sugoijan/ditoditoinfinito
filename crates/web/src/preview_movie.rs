//! Song select's preview movie on the web (`.ssc` `#PREVIEWVID`;
//! `ddi_session::movies::preview`, `docs/plans/lane-covers.md`, "Preview
//! video"): the selected song's movie, muted and looping, in the banner's
//! place once the selection has settled. When it plays is shared with
//! desktop ([`PreviewMovie`]); this side reads the file (stored or linked
//! under `bg/<name>`, as play reads a background movie), decodes it as
//! play does (the browser's decoder, else the module's worker:
//! [`MovieDeck`]), draws its frames into the picture the 2D pass shows as
//! the image [`SOURCE`] (`ddi_render::movie_picture`), and tells song
//! select whose movie shows (its feed).
//!
//! Song select's page installs it as its host's frame hook
//! (`ui_dev::set_frame_hook`): it runs in each frame before the 2D pass
//! and keeps frames coming while a movie settles or plays. The page going
//! takes it away ([`PreviewMovieHost::release`]).

use std::cell::RefCell;
use std::rc::Rc;

use crate::web::movie_source::MovieSource;
use ddi_library::backgrounds::BgImage;
use ddi_platform::video::VideoFormat;
use ddi_render::movie_picture::{MoviePicture, SOURCE};
use ddi_session::movies::preview::{self, PreviewMovie, Step};

use crate::movies::MovieDeck;
use crate::songs::ManifestEntry;
use crate::web::gfx::Gfx;

/// A movie's file as it was read: for selection `generation`.
type Read = (u64, Result<(MovieSource, VideoFormat), String>);

/// The movie and its picture on one device.
#[derive(Default)]
pub(crate) struct PreviewMovieHost {
    state: PreviewMovie,
    /// The selection the deck and the read are for.
    generation: u64,
    deck: Option<MovieDeck>,
    /// The file being read, once read.
    read: Rc<RefCell<Option<Read>>>,
    /// The picture, and the device it was made on.
    picture: Option<(u64, MoviePicture)>,
    /// The picture's texture generation the 2D pass was given.
    registered: Option<u64>,
    /// The frames shown so far by the deck, as last counted.
    counted: u64,
}

impl PreviewMovieHost {
    /// A frame at `now_ms`: follows song select (`song`: the song it
    /// shows, `still`: reduced motion; `entry` looks a song up), decodes,
    /// draws the picture into `encoder`; whether frames are to go on, and
    /// what song select is to be told (`Feed::preview_movie`).
    pub(crate) fn frame(
        &mut self,
        gfx: &Gfx,
        encoder: &mut wgpu::CommandEncoder,
        (song, still): (Option<&str>, bool),
        now_ms: f64,
        entry: &dyn Fn(&str) -> Option<ManifestEntry>,
    ) -> (bool, Option<Option<String>>) {
        let now = now_ms / 1000.0;
        self.state.follow(song, still, now, |id| {
            let mode = crate::settings::Settings::load().video;
            preview::movie_of(&entry(id)?, mode)
        });
        if self
            .picture
            .as_ref()
            .is_some_and(|(g, _)| *g != gfx.generation)
        {
            // A new device: the movie opens again on it.
            self.picture = None;
            self.registered = None;
            self.generation = 0;
        }
        if self.state.generation() != self.generation {
            self.generation = self.state.generation();
            // The last movie's decoders close; its textures go.
            self.deck = None;
            self.counted = 0;
            if let Some((_, p)) = &mut self.picture {
                p.clear();
            }
            self.read.borrow_mut().take();
            if let Some((id, name)) = self.state.wanted()
                && let Some(entry) = entry(id)
            {
                self.start_read(entry, name.to_string());
            }
        }
        // Owned: the state is told what came of it below.
        let step = match self.state.step(now) {
            Step::Idle => None,
            Step::Play { name, layers, t } => Some((name.to_string(), layers.to_vec(), t)),
        };
        match step {
            None => {
                if let Some(d) = &mut self.deck {
                    d.stop();
                }
            }
            Some((name, layers, t)) => {
                if self.deck.is_none() {
                    let read = self
                        .read
                        .borrow_mut()
                        .take_if(|(g, _)| *g == self.generation);
                    match read {
                        // Read: the deck opens it.
                        Some((_, Ok((blob, format)))) => {
                            let mut deck = MovieDeck::default();
                            deck.add_source(name.clone(), blob, format, false);
                            self.deck = Some(deck);
                        }
                        Some((_, Err(e))) => {
                            web_sys::console::warn_1(&format!("preview video {name}: {e}").into());
                            self.state.fail(e);
                        }
                        None => {}
                    }
                }
                if let Some(deck) = &mut self.deck {
                    // Made for the first movie of the page.
                    let (_, picture) = self.picture.get_or_insert_with(|| {
                        (gfx.generation, MoviePicture::new(&gfx.device, &gfx.queue))
                    });
                    deck.update(&layers, t, picture.renderer_mut(), &gfx.device, &gfx.queue);
                    let shown = deck.stats().shown;
                    let id = deck
                        .shown_ids()
                        .find_map(|(image, id)| matches!(image, BgImage::Movie(_)).then_some(id));
                    if let Some((_, e)) = deck.failed().first() {
                        self.state.fail(e.clone());
                    } else if let Some(id) = id
                        && shown != self.counted
                    {
                        self.counted = shown;
                        picture.draw(&gfx.device, &gfx.queue, encoder, id);
                        self.state.shown(now);
                    }
                }
            }
        }
        if let Some((_, picture)) = &self.picture
            && self.registered != Some(picture.generation())
            && picture.texture().is_some()
        {
            crate::web::screen::with_painter(gfx, |p| {
                p.set_external_image(&gfx.device, SOURCE, picture.texture());
            });
            self.registered = Some(picture.generation());
        }
        (self.state.busy(), self.state.news())
    }

    /// Reads movie `name` of `entry` for this selection.
    fn start_read(&self, entry: ManifestEntry, name: String) {
        let cell = self.read.clone();
        let generation = self.generation;
        let format = entry
            .preview_video
            .as_ref()
            .map(|v| v.format.clone())
            .unwrap_or(VideoFormat {
                container: ddi_platform::video::VideoContainer::Unknown,
                codec: ddi_platform::video::VideoCodec::Unknown,
            });
        wasm_bindgen_futures::spawn_local(async move {
            let got = match crate::songs::load_movie(&entry, &name, &[], None).await {
                Ok(Some(blob)) => Ok((blob, format)),
                Ok(None) => Err("no file".to_string()),
                Err(e) => Err(e),
            };
            *cell.borrow_mut() = Some((generation, got));
        });
    }

    /// The page goes: the 2D pass forgets the picture, the decoders close.
    pub(crate) fn release(&mut self, gfx: Option<&Gfx>) {
        self.deck = None;
        if let (Some(gfx), Some(_)) = (gfx, self.registered.take()) {
            crate::web::screen::with_painter(gfx, |p| {
                p.set_external_image(&gfx.device, SOURCE, None);
            });
        }
        self.picture = None;
    }

    /// For `__DDI_DEBUG.songs.preview.movie`: the song and movie wanted,
    /// whose movie shows, frames drawn, why it failed, and the decoder's
    /// state (as `__DDI_DEBUG.video`'s).
    pub(crate) fn debug(&self) -> js_sys::Object {
        let obj = js_sys::Object::new();
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(&obj, &k.into(), &v);
        };
        let null = wasm_bindgen::JsValue::NULL;
        let (song, name) = self.state.wanted().unzip();
        set("song", song.map_or(null.clone(), |s| s.into()));
        set("name", name.map_or(null.clone(), |s| s.into()));
        set(
            "showing",
            self.state.showing().map_or(null.clone(), |s| s.into()),
        );
        set("frames", (self.state.frames() as f64).into());
        set("busy", self.state.busy().into());
        set(
            "failed",
            self.state.failed().map_or(null.clone(), |s| s.into()),
        );
        set(
            "decoder",
            self.deck.as_ref().map_or(null, |d| d.debug().into()),
        );
        obj
    }
}
