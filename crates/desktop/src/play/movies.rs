//! A play's background movies on desktop (feature `video`): the FFmpeg
//! helper in its sandbox or the checked OS path ([`crate::video`]) under
//! the movies both shells share ([`ddi_session::movies`]: which segments
//! have a decoder, their tracks, the frames due, what shows), frames shown
//! through `Renderer::add_video`. The same schedule rules
//! (`ddi_library::backgrounds`, both layers on the one clock a movie
//! has), lead time and queue depth as the web's, so a movie changes,
//! loops and stops where the web's does.

use ddi_library::backgrounds::BgSegment;
use ddi_platform::video::{VideoBackend, VideoCodec, VideoContainer, VideoFormat, VideoFrame};
use ddi_render::Renderer;
use ddi_session::movies::{MovieShell, MovieSlots};

use crate::video::{DesktopVideo, HelperStatus, MovieFile, VideoHost};

/// The first movie's picture probe: its name, its id, its answer (the
/// picture's shape, or none).
type Probe = (String, Option<u32>, Option<Result<f32, ()>>);

/// What desktop brings to the shared movies: the song's files and the
/// decoder helper (or the OS's decoders) that opens them.
#[derive(Default)]
pub struct Decoders {
    host: Option<VideoHost>,
    /// `(name, file)`, or why it cannot be had.
    sources: Vec<(String, Result<MovieFile, String>)>,
}

impl Decoders {
    fn host(&mut self) -> &mut VideoHost {
        self.host.get_or_insert_with(|| {
            let h = VideoHost::shared();
            h.preload();
            h
        })
    }

    /// A frame's planes back to the decoder thread.
    fn recycle(&self, frame: VideoFrame) {
        if let Some(h) = &self.host {
            h.recycle(frame.planes);
        }
    }
}

impl MovieShell for Decoders {
    type Decoder = DesktopVideo;

    fn open(&mut self, name: &str) -> Option<Result<DesktopVideo, String>> {
        let (_, path) = self.sources.iter().find(|(n, _)| n == name)?;
        let path = match path.clone() {
            Ok(p) => p,
            Err(e) => return Some(Err(e)),
        };
        let format = VideoFormat {
            container: VideoContainer::Unknown,
            codec: VideoCodec::Unknown,
        };
        Some(Ok(self.host().open(path, name, &format)))
    }

    fn discard(&mut self, frame: VideoFrame) {
        self.recycle(frame);
    }

    /// A helper that stopped is started again for a retry.
    fn stopped(&mut self) {
        if self
            .host
            .as_ref()
            .is_some_and(|h| matches!(h.status(), HelperStatus::Failed(_)))
        {
            self.host = None;
        }
    }

    fn warn(&self, line: &str) {
        eprintln!("{line}");
    }
}

/// The song's movies: [`MovieSlots`]' state and rules (through `Deref`),
/// with the probe and the textures.
#[derive(Default)]
pub struct Movies {
    slots: MovieSlots<Decoders>,
    /// The first movie's picture probe, and its answer.
    probe: Option<Probe>,
}

impl std::ops::Deref for Movies {
    type Target = MovieSlots<Decoders>;

    fn deref(&self) -> &Self::Target {
        &self.slots
    }
}

impl std::ops::DerefMut for Movies {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.slots
    }
}

impl Movies {
    /// A movie of the song, at `at` (a file, or a range of one: a zip's
    /// stored entry), or why it cannot be had. The first one added is
    /// probed for the background frame.
    pub fn add(&mut self, name: &str, at: Result<crate::play::load::MovieAt, String>) {
        let decoders = self.slots.shell_mut();
        if decoders.sources.iter().any(|(n, _)| n == name) {
            return;
        }
        let path = at.map(|(path, range)| MovieFile { path, range });
        if let Ok(p) = &path
            && self.probe.is_none()
        {
            let id = decoders.host().probe(p, name);
            self.probe = Some((name.to_string(), Some(id), None));
        }
        decoders.sources.push((name.to_string(), path));
    }

    /// The probed movie's picture shape: `Err(true)` while the probe runs.
    pub fn picture_aspect(&mut self) -> Result<f32, bool> {
        let host = self.slots.shell().host.clone();
        let Some((_, id, answer)) = self.probe.as_mut() else {
            return Err(false);
        };
        if answer.is_none()
            && let (Some(i), Some(h)) = (*id, host)
        {
            *answer = h
                .probe_result(i)
                .map(|r| r.ok().and_then(|p| p.aspect()).ok_or(()));
        }
        match answer {
            None => Err(true),
            Some(Ok(a)) => Ok(*a),
            Some(Err(())) => Err(false),
        }
    }

    /// Where the decoder helper stands, once a movie asked for it.
    pub fn helper_status(&self) -> Option<HelperStatus> {
        self.slots.shell().host.as_ref().map(VideoHost::status)
    }

    /// Opens, feeds and closes decoders for song second `t`, and shows
    /// the frames due; `layers` the background layers' schedules.
    pub fn update(
        &mut self,
        layers: &[Vec<BgSegment>],
        t: f64,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if self.slots.shell().sources.is_empty() {
            return;
        }
        let due = self.slots.update(layers, t);
        for (name, w, h, _) in due.early {
            self.texture(&name, w, h, renderer, device);
        }
        for (name, frame) in due.show {
            self.upload(&name, &frame, renderer, device, queue);
            self.slots.shell().recycle(frame);
        }
    }

    fn texture(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        renderer: &mut Renderer,
        device: &wgpu::Device,
    ) -> Option<usize> {
        let limit = device.limits().max_texture_dimension_2d;
        if !self.slots.fits(name, width, height, limit) {
            return None;
        }
        if let Some(id) = self.slots.texture(name) {
            return Some(id);
        }
        let id = renderer.add_video(device, width, height);
        self.slots.set_texture(name, id);
        Some(id)
    }

    fn upload(
        &mut self,
        name: &str,
        frame: &VideoFrame,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if frame.external.is_none() && frame.split().is_none() {
            self.slots.fail(name, "a malformed frame".into());
            return;
        }
        let Some(id) = self.texture(name, frame.width, frame.height, renderer, device) else {
            return;
        };
        if crate::video::show(renderer, device, queue, id, frame) {
            self.slots.shown(name);
        }
    }
}
