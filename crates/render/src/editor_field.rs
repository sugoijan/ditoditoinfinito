//! The chart editor's field in the player's noteskin
//! (`docs/plans/editor.md`, step 3): a play's frame paused at the
//! editor's cursor (`ddi_ui::screens::editor_field`, which places every
//! note as play would) drawn as play draws its field, receptors, holds
//! and notes, with no HUD and no background, into a texture of its own
//! that the 2D pass shows under [`SOURCE`] and the editor draws its
//! grid over. As Options' field preview ([`crate::field_preview`]), on
//! every backend: offscreen in a float format, encoded at the end.
//!
//! The skin is tamed: while the music does not play, its clock is held
//! at rest ([`REST`] seconds in, beat 0: its Lua's and actors' own
//! animations still, the receptors' beat pulse among them, no
//! explosions); once the music stops the skin starts over and is played
//! to rest again. The notes are drawn as the play's frame at the cursor
//! draws them all the same: their animation frame and the colors it
//! picks (Vivid's cycle, a beat-based sprite's frame, Rainbow's flow,
//! the built-in arrows' schemes) at the cursor's beat, the beat a play
//! is at when that row reaches the receptors. StepMania's editor does
//! the same: it draws its notes with the song's beat set to its cursor's
//! (`ScreenEdit::DrawPrimitives`, `src/ScreenEdit.cpp:2020-2058` at
//! 5.1's `825467b`, which `NoteDisplay::SetActiveFrame` reads), so as
//! the cursor moves a vivid note's color moves as it would in play.
//! While the music plays, the frames' clock and events (the notes
//! passing, as an autoplay hits them) run the skin as in play. A frame
//! is drawn only when the shell asks: the editor's picture changed.

use ddi_chart::Layout;
use ddi_engine::frame::{Frame, SkinEvent};
use ddi_engine::rules::{JudgeNames, presets};

use crate::noteskin::{NoteLayer, NoteskinPlay};
use crate::renderer::{Renderer, SkinBlending};
use crate::scene::{ColorScheme, FieldGeometry, Preview, RenderOptions};

/// The texture source the 2D pass draws the field under (the editor
/// screen's `editor_field::SOURCE`).
pub const SOURCE: &str = "editor-field";

/// The format of the texture drawn into: encoded values, as the 2D
/// pass's images.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The skin's clock at rest: seconds after its start, long enough for
/// its first commands (a receptor's entrance) to end.
pub const REST: f64 = 2.0;

/// The steps the skin is played to rest in (seconds).
const SETTLE_STEP: f64 = 0.25;

/// One frame to draw.
pub struct FieldDraw<'a> {
    pub frame: &'a Frame,
    pub layout: &'a Layout,
    /// In the texture's pixels (its size the texture's).
    pub geometry: FieldGeometry,
    /// The music plays: the skin's clock runs with the frame's, and
    /// `events` play on it.
    pub running: bool,
    pub events: &'a [SkinEvent],
    pub note_colors: ColorScheme,
    /// Notes in the colors the chart gives them, as a play's
    /// [`RenderOptions::chart_colors`].
    pub chart_colors: bool,
    /// Under the field: encoded RGBA.
    pub background: [f32; 4],
}

/// The texture drawn into.
struct Target {
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The editor's field on one device.
pub struct EditorField {
    renderer: Renderer,
    names: JudgeNames,
    noteskin: Option<NoteskinPlay>,
    target: Option<Target>,
    /// The skin was played to rest since it last ran.
    settled: bool,
    frames: u64,
    generation: u64,
}

impl EditorField {
    /// A field on `device`, drawn through `float` (a float format the
    /// device renders and blends; `None`: straight into the plain
    /// texture).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        float: Option<wgpu::TextureFormat>,
        fonts: &[&'static [u8]],
    ) -> EditorField {
        let blending = match float {
            Some(f) => SkinBlending::Encoded(f),
            None => SkinBlending::Scene,
        };
        EditorField {
            renderer: Renderer::new(device, queue, FORMAT, blending, fonts),
            names: presets::itg().names,
            noteskin: None,
            target: None,
            settled: false,
            frames: 0,
            generation: 0,
        }
    }

    /// Draws the notes with `play` (loaded for the chart's layout and
    /// the direction drawn; `None`: the built-in arrows) from the next
    /// frame; `upload` puts its textures on the renderer and tells
    /// `play` their ids. The last skin's textures go.
    pub fn set_noteskin(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        play: Option<NoteskinPlay>,
        upload: impl FnOnce(&mut Renderer, &mut NoteskinPlay),
    ) {
        self.renderer.clear_noteskin_textures();
        self.renderer.set_skins_drawn(device, queue, play.is_some());
        self.noteskin = play.map(|mut p| {
            p.set_srgb(self.renderer.skin_colors_linear());
            upload(&mut self.renderer, &mut p);
            p
        });
        self.settled = false;
    }

    pub fn noteskin(&self) -> Option<&NoteskinPlay> {
        self.noteskin.as_ref()
    }

    /// The texture drawn into, once a frame was.
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.texture)
    }

    /// Changes each time the texture is made again (a new size).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Lets the textures kept at the field's size go while nothing shows
    /// it (the 2D pass's image taken away first); the next frame makes
    /// them again under a new [`EditorField::generation`].
    pub fn release_targets(&mut self) {
        self.target = None;
        self.renderer.release_targets();
    }

    pub fn has_targets(&self) -> bool {
        self.target.is_some()
    }

    /// The skin from its start, its clock stepped to [`REST`] with no
    /// notes and no events.
    fn settle(play: &mut NoteskinPlay, frame: &Frame, reverse: bool) {
        play.restart();
        let mut f = Frame {
            notes: Vec::new(),
            ..rest(frame)
        };
        let steps = (REST / SETTLE_STEP).round() as u32;
        for i in 0..=steps {
            f.song_time = f64::from(i) * SETTLE_STEP;
            play.update(&f, std::iter::empty(), reverse);
        }
    }

    /// Records `d`'s frame into `encoder`, `size` pixels, into the
    /// field's texture (made again for a new size).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        d: &FieldDraw<'_>,
    ) {
        let size = [size[0].max(1), size[1].max(1)];
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = crate::gpu::texture_2d(
                device,
                "ddi-editor-field",
                size,
                FORMAT,
                1,
                // Read back by tests.
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                &[],
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some(Target {
                size,
                texture,
                view,
            });
            self.generation += 1;
        }
        let reverse = d.geometry.direction < 0.0;
        let mut still;
        let frame = if d.running {
            self.settled = false;
            if let Some(p) = &mut self.noteskin {
                p.update(d.frame, d.events.iter(), reverse);
            }
            d.frame
        } else {
            still = rest(d.frame);
            if let Some(p) = &mut self.noteskin {
                if !self.settled {
                    Self::settle(p, d.frame, reverse);
                }
                p.update(&still, std::iter::empty(), reverse);
            }
            self.settled = true;
            // The skin's clock at rest, its notes at the cursor's beat.
            still.beat = d.frame.beat;
            &still
        };
        let [r, g, b, a] = d.background;
        self.renderer.skin.background = wgpu::Color {
            r: f64::from(r),
            g: f64::from(g),
            b: f64::from(b),
            a: f64::from(a),
        };
        let opts = RenderOptions {
            reverse,
            note_colors: d.note_colors,
            chart_colors: d.chart_colors,
            background: 0.0,
            preview: Some(Preview {
                geometry: d.geometry,
                receptors: true,
            }),
            ..RenderOptions::default()
        };
        let Some(target) = &self.target else {
            return;
        };
        self.renderer.render(
            device,
            queue,
            encoder,
            &target.view,
            None,
            size[0],
            size[1],
            frame,
            d.layout,
            &self.names,
            opts,
            self.noteskin.as_mut().map(|p| p as &mut dyn NoteLayer),
        );
        self.frames += 1;
    }
}

/// `frame` with the skin's clock at rest: [`REST`] seconds, beat 0,
/// nothing held.
fn rest(frame: &Frame) -> Frame {
    let mut f = frame.clone();
    f.song_time = REST;
    f.beat = 0.0;
    for r in &mut f.receptors {
        r.holding = None;
        r.pressed = false;
        r.flash = None;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_rest_the_clock_is_still() {
        let frame = Frame {
            song_time: 31.5,
            beat: 63.0,
            roll_window_seconds: 0.35,
            lanes: 1,
            receptors: vec![ddi_engine::frame::ReceptorState {
                pressed: true,
                flash: None,
                holding: Some(ddi_engine::frame::HoldKind::Hold),
            }],
            notes: Vec::new(),
            judgement: None,
            holds: Default::default(),
            combo: 0,
            score: Default::default(),
            life: 1.0,
            danger: false,
            failed: false,
            finished: false,
            visible_range: 10.0,
            appearance: Default::default(),
            cover: Default::default(),
        };
        let r = rest(&frame);
        assert_eq!((r.song_time, r.beat), (REST, 0.0));
        assert!(!r.receptors[0].pressed && r.receptors[0].holding.is_none());
    }
}
