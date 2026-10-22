//! Song select's preview movie on desktop (`.ssc` `#PREVIEWVID`;
//! `ddi_session::movies::preview`, `docs/plans/lane-covers.md`, "Preview
//! video"), as the web's (`crates/web/src/preview_movie.rs`): the
//! selected song's movie, muted and looping, in the banner's place once
//! the selection has settled. When it plays is shared ([`PreviewMovie`]);
//! this side finds the file (a folder's, or a zip's read in place or
//! extracted once, on a thread: `crate::play::load::Files::movie`),
//! decodes it as a play does (feature `video`: the OS's decoders or the
//! helper, [`crate::play::movies::Movies`]), draws its frames into the
//! picture the painter shows as the image [`SOURCE`]
//! (`ddi_render::movie_picture`), and tells song select whose movie shows
//! (its feed).
//!
//! The shell follows song select before each frame
//! ([`PreviewMovieHost::follow`]) and keeps frames coming while
//! [`PreviewMovieHost::busy`]; the frame's drawing records the picture
//! before the painter ([`PreviewMovieHost::draw`]).

use std::time::Instant;

use ddi_render::Painter;
#[cfg(feature = "video")]
use ddi_render::movie_picture::{MoviePicture, SOURCE};
use ddi_session::movies::preview::{self, PreviewMovie};
use ddi_ui::Ui;
use ddi_ui::message::Message;
use ddi_ui::screens::Env;
use ddi_ui::screens::select::{FEED, SharedFeed};

#[cfg(feature = "video")]
type Found = Result<crate::play::load::MovieAt, String>;

/// The movie and its picture on one device.
pub struct PreviewMovieHost {
    state: PreviewMovie,
    start: Instant,
    /// Song select's feed, to tell whose movie shows.
    feed: Option<SharedFeed>,
    /// The selection what follows is for.
    #[cfg(feature = "video")]
    generation: u64,
    #[cfg(feature = "video")]
    deck: Option<crate::play::movies::Movies>,
    /// The file being found on a thread.
    #[cfg(feature = "video")]
    finding: Option<std::sync::mpsc::Receiver<Found>>,
    #[cfg(feature = "video")]
    picture: Option<MoviePicture>,
    /// The picture's texture generation the painter was given.
    #[cfg(feature = "video")]
    registered: Option<u64>,
    /// The frames shown so far by the deck, as last counted.
    #[cfg(feature = "video")]
    counted: u64,
}

impl Default for PreviewMovieHost {
    fn default() -> Self {
        PreviewMovieHost {
            state: PreviewMovie::default(),
            start: Instant::now(),
            feed: None,
            #[cfg(feature = "video")]
            generation: 0,
            #[cfg(feature = "video")]
            deck: None,
            #[cfg(feature = "video")]
            finding: None,
            #[cfg(feature = "video")]
            picture: None,
            #[cfg(feature = "video")]
            registered: None,
            #[cfg(feature = "video")]
            counted: 0,
        }
    }
}

impl PreviewMovieHost {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Follows song select before a frame: the song `env`'s state says it
    /// shows while it is what `ui` shows (`feed`: its feed, `None` without
    /// a library); the song's movie looked up when it changes.
    pub fn follow(&mut self, ui: &Ui, env: &Env, feed: Option<&SharedFeed>) {
        self.feed = feed.cloned();
        let song = env.state.borrow().song_id.clone();
        let song = (feed.is_some() && preview::select_shows(&ui.layers())).then_some(song.as_str());
        let now = self.now();
        self.state.follow(song, env.reduced(), now, |id| {
            if !cfg!(feature = "video") {
                return None;
            }
            let entry = crate::library::song(id)
                .map(|s| s.entry)
                .or_else(|| crate::library::bundled().into_iter().find(|e| e.id == id))?;
            let mode = ddi_ui::state::settings::Settings::load().video;
            preview::movie_of(&entry, mode)
        });
    }

    /// Whether frames are to go on (a movie settles, opens or plays).
    pub fn busy(&self) -> bool {
        self.state.busy()
    }

    /// For tests: the song and movie wanted, whose movie shows, frames
    /// drawn, why it failed.
    pub fn state(&self) -> &PreviewMovie {
        &self.state
    }

    /// The frame's part, before the painter draws: decodes, records the
    /// picture into `encoder` and gives it to `painter`; song select told
    /// whose movie shows (through `ui`).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        painter: &mut Painter,
        ui: &mut Ui,
    ) {
        #[cfg(feature = "video")]
        self.decode(device, queue, encoder, painter);
        #[cfg(not(feature = "video"))]
        let _ = (device, queue, encoder, painter);
        if let Some(news) = self.state.news()
            && let Some(feed) = &self.feed
        {
            feed.borrow_mut().preview_movie = Some(news);
            ui.broadcast(Message::new(FEED));
        }
    }

    #[cfg(feature = "video")]
    fn decode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        painter: &mut Painter,
    ) {
        use ddi_library::backgrounds::BgImage;
        use ddi_session::movies::preview::Step;
        let now = self.now();
        if self.state.generation() != self.generation {
            self.generation = self.state.generation();
            // The last movie's decoders close; its textures go.
            self.deck = None;
            self.finding = None;
            self.counted = 0;
            if let Some(p) = &mut self.picture {
                p.clear();
            }
            if let Some((id, name)) = self.state.wanted() {
                let (tx, rx) = std::sync::mpsc::channel();
                let (id, name) = (id.to_string(), name.to_string());
                let spawned = std::thread::Builder::new()
                    .name("preview-movie".into())
                    .spawn(move || {
                        let found = crate::play::load::Files::of(&id)
                            .ok_or_else(|| format!("no song {id}"))
                            .and_then(|(files, _)| files.movie(&name));
                        let _ = tx.send(found);
                    });
                match spawned {
                    Ok(_) => self.finding = Some(rx),
                    Err(e) => self.state.fail(e.to_string()),
                }
            }
        }
        // Owned: the state is told what came of it below.
        let step = match self.state.step(now) {
            Step::Idle => None,
            Step::Play { name, layers, t } => Some((name.to_string(), layers.to_vec(), t)),
        };
        let Some((name, layers, t)) = step else {
            // Closed (the still frame stays in the picture).
            self.deck = None;
            return;
        };
        if self.deck.is_none()
            && let Some(found) = self.finding.as_ref().and_then(|rx| rx.try_recv().ok())
        {
            self.finding = None;
            let mut deck = crate::play::movies::Movies::default();
            deck.add(&name, found);
            self.deck = Some(deck);
        }
        let Some(deck) = &mut self.deck else {
            return;
        };
        let picture = self
            .picture
            .get_or_insert_with(|| MoviePicture::new(device, queue));
        crate::video::protect_external_frames(device, encoder);
        deck.update(&layers, t, picture.renderer_mut(), device, queue);
        if let Some((_, e)) = deck.failed().first() {
            eprintln!("preview video {name}: {e}");
            self.state.fail(e.clone());
            return;
        }
        let shown = deck.stats().shown;
        let id = deck
            .shown_ids()
            .find_map(|(image, id)| matches!(image, BgImage::Movie(_)).then_some(id));
        if let Some(id) = id
            && shown != self.counted
        {
            self.counted = shown;
            picture.draw(device, queue, encoder, id);
            self.state.shown(now);
        }
        if self.registered != Some(picture.generation()) && picture.texture().is_some() {
            painter.set_external_image(device, SOURCE, picture.texture());
            self.registered = Some(picture.generation());
        }
    }
}
