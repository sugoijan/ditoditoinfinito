//! A room host's Twitch chat behind the play, placed and drawn
//! (`docs/plans/multiplayer-twitch.md`, C6, C7, C9, C10): the session's
//! chat events (`ddi_session::chat`) taken into the layer
//! (`ddi_chat::overlay`) on the host's clock, its items placed in the
//! play's free space (the rows over the whole screen, Falling's columns
//! in the side space clear of the miniatures where there is room), and
//! drawn by the play's own 2D painter into a texture the backdrop shows
//! under the field (`ddi_render::Renderer::set_chat_layer`): the text
//! through the platform's runs as the lyrics' is, the emotes from one
//! atlas (one batch).
//!
//! It is the first thing shed when the play's frames run late: no new
//! items while the last measured second was late (the signal the dancing
//! hand's load rule reads, `ddi_session::hand_load`), the items already on
//! screen going on, every animated emote held on the frame it had.
//!
//! Animated emotes (round TW4, `ddi_chat::anim`): each frame a cell of the
//! atlas, the frame drawn chosen by the layer's own clock (the host's,
//! held while shedding, standing still under reduced motion), so every
//! copy of an emote shows the same frame.

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_chat::cache::{ATLAS, Answer, EmoteCache, Fetch, Got, Via, fit};
use ddi_chat::emotes::Emote;
use ddi_chat::overlay::{Area, CHAT_OVERLAY_V0, Glyphs, Options, Overlay};
use ddi_render::texture_cache::TextureKey;
use ddi_render::{DrawList, Painter, Quad, Renderer};
use ddi_session::chat::ChatLayer;

/// The faces chat text is set in: the system's, so any script a chat
/// writes draws.
pub const FONT: &str = "system-ui, \"Noto Sans\", sans-serif";

/// The text's size over an item's height.
const TEXT_SIZE: f32 = 0.62;

/// The chat's one quiet color, its names' and the counts', and the soft
/// shadow under the text.
const TEXT: [f32; 3] = [0.93, 0.92, 0.98];
const NAME: [f32; 3] = [0.72, 0.78, 0.98];
const COUNT: [f32; 3] = [0.82, 0.82, 0.88];
const SHADOW: [f32; 3] = [0.02, 0.0, 0.05];

/// The atlas's source for the painter.
const ATLAS_SOURCE: &str = "chat:emotes";

/// A decoded emote image: its frames' size (at most [`fit`]'s), their
/// RGBA rows one frame after another, straight alpha, encoded, and each
/// frame's duration, milliseconds (one frame: a still image).
#[derive(Clone, Debug)]
pub struct Decoded {
    pub size: [u32; 2],
    pub rgba: Vec<u8>,
    pub durations: Vec<u32>,
}

impl Decoded {
    /// A still image.
    pub fn still(size: [u32; 2], rgba: Vec<u8>) -> Decoded {
        Decoded {
            size,
            rgba,
            durations: vec![0],
        }
    }

    /// Frame `k`'s pixels.
    pub fn frame(&self, k: usize) -> Option<&[u8]> {
        let n = (self.size[0] as usize) * (self.size[1] as usize) * 4;
        self.rgba.get(k * n..(k + 1) * n)
    }
}

/// An item's height for a target `size` pixels, `ratio` of them a
/// logical pixel: a 22nd of the height (a 14th of the width on a screen
/// held upright), between 18 and 40 logical pixels.
pub fn line_for([width, height]: [f32; 2], ratio: f32) -> f32 {
    (height / 22.0).min(width / 14.0).clamp(18.0 * ratio, 40.0 * ratio)
}

/// What the last frame did, for the debug state and the tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatDrawn {
    /// Items drawn, their text runs and emotes.
    pub items: usize,
    pub runs: usize,
    pub emotes: usize,
    /// New items are held back (the frames run late).
    pub shed: bool,
    /// The layer's statistics since the play began.
    pub stats: ddi_chat::overlay::Stats,
    /// Images ready, in flight, failed; hosts through the proxy.
    pub images: (usize, usize, usize, usize),
    /// Animated emotes ready and the atlas units their frames take.
    pub animated: (usize, usize),
    /// The animation clock, milliseconds (held while shedding, 0 under
    /// reduced motion).
    pub anim_ms: u64,
    /// Each emote drawn this frame (which frame of it showed).
    pub emotes_drawn: Vec<EmoteDrawn>,
    /// The level and the style this frame's layout let it show
    /// ([`shown`]).
    pub shown: Option<(ddi_chat::Level, ddi_chat::Style)>,
    /// The texts drawn, for tests (each item's text pieces joined).
    pub texts: Vec<String>,
    /// Where each item was, `[x, y, width, height]` target pixels.
    pub boxes: Vec<[f32; 4]>,
    /// The layer's opacity.
    pub opacity: f32,
    /// Milliseconds this frame's chat took on the main thread.
    pub ms: f64,
}

/// An emote as a frame drew it, for the debug state and the tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmoteDrawn {
    /// Its item's id (the same while the item is on screen).
    pub item: u64,
    /// Where it was drawn, `[left, top, right, bottom]` target pixels.
    pub rect: [f32; 4],
    /// Its atlas cell's left and top: which frame showed.
    pub cell: [u32; 2],
    /// Its provider animates it.
    pub animated: bool,
}

/// The chat's view over a play.
pub struct ChatView {
    overlay: Overlay,
    cache: EmoteCache,
    seen: u64,
    atlas: Option<wgpu::Texture>,
    layer: Option<wgpu::Texture>,
    list: DrawList,
    /// New items are held back since a late second, until a good one.
    shed: bool,
    /// The host second the first measured second ended at.
    since: Option<f64>,
    /// Animated emotes' clock.
    clock: ddi_chat::anim::Clock,
    drawn: ChatDrawn,
}

impl ChatView {
    /// The view of `chat`, reduced motion as `reduced`.
    pub fn new(chat: &ChatLayer, reduced: bool) -> ChatView {
        let c = chat.config();
        ChatView {
            overlay: Overlay::new(
                CHAT_OVERLAY_V0,
                Options {
                    level: c.level,
                    style: c.style,
                    names: c.names,
                    reduced,
                },
                chat.sets().clone(),
            ),
            // The scripted chat's images come from no provider: no proxy.
            cache: EmoteCache::new(None),
            seen: 0,
            atlas: None,
            layer: None,
            list: DrawList::new(),
            shed: false,
            since: None,
            clock: ddi_chat::anim::Clock::default(),
            drawn: ChatDrawn::default(),
        }
    }

    /// The animation clock at host second `now`: running unless the
    /// frames run late (held on the frame it had) or motion is reduced
    /// (every emote's first frame, still); its milliseconds.
    pub fn tick_clock(&mut self, now: f64) -> u64 {
        let running = !self.shed && !self.overlay.options().reduced;
        self.clock.advance(now, running);
        self.clock.ms()
    }

    pub fn drawn(&self) -> &ChatDrawn {
        &self.drawn
    }

    /// Whether new items are held back now (the frames run late).
    pub fn shedding(&self) -> bool {
        self.shed
    }

    /// A measured second (`second`) ended at host second `now`: whether
    /// it was late as the dancing hand's rule has it (fewer frames than
    /// [`ddi_session::hand_load::LATE_SHARE`] of the refresh; a one-off
    /// stall, [`ddi_session::hand_load::HITCH_MS`], and the play's first
    /// [`ddi_session::hand_load::WARMUP_MS`] count for nothing). New items
    /// are held back from a late second to the next good one.
    pub fn second(&mut self, second: ddi_platform::video::Second, refresh: f64, now: f64) {
        use ddi_session::hand_load::{HITCH_MS, LATE_SHARE, WARMUP_MS};
        let since = *self.since.get_or_insert(now);
        if second.worst_ms > HITCH_MS || (now - since) * 1000.0 < WARMUP_MS {
            self.shed = false;
            return;
        }
        self.shed = second.fps < LATE_SHARE * refresh;
    }

    /// One frame at host second `now` into `target` (`size` pixels, the
    /// painter's format): the events since the last frame taken in, the
    /// images fetched (`fetch` hands a shell's fetch on, false where it
    /// cannot; `fetched` are those back since), the items placed in `area`
    /// and drawn into the layer, which `renderer` shows under the field.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        (device, queue, encoder): (&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder),
        painter: &mut Painter,
        renderer: &mut Renderer,
        chat: &ChatLayer,
        (size, format): ([u32; 2], wgpu::TextureFormat),
        (area, (level, style)): (&Area, (ddi_chat::Level, ddi_chat::Style)),
        now: f64,
        fetch: &mut dyn FnMut(&Fetch) -> bool,
        fetched: Vec<(Fetch, Option<Decoded>)>,
    ) {
        // What this frame's layout lets it show ([`shown`]).
        self.overlay.set_shown(level, style);
        let accepting = !self.shed;
        for (n, at, event) in chat.since(self.seen) {
            self.overlay.receive(event, at, accepting);
            self.seen = n;
        }
        // The images: kept recent while wanted or drawn, new ones asked
        // for, those back placed in the atlas.
        self.cache.next_frame();
        let wanted: Vec<Emote> = self
            .overlay
            .wanted()
            .into_iter()
            .chain(self.overlay.drawn_emotes())
            .cloned()
            .collect();
        for e in &wanted {
            self.cache.want(e, self.overlay.sets());
        }
        let mut back = fetched;
        for f in self.cache.start() {
            if let Some((size, rgba)) = ddi_chat::script::shape_pixels(&f.url) {
                // The scripted chat's own plain shapes, drawn here.
                back.push((f, Some(Decoded::still(size, rgba))));
            } else if let Some((size, rgba, durations)) = ddi_chat::script::anim_pixels(&f.url) {
                back.push((
                    f,
                    Some(Decoded {
                        size,
                        rgba,
                        durations,
                    }),
                ));
            } else if !fetch(&f) {
                back.push((f, None));
            }
        }
        for (f, decoded) in back {
            let got = decoded.as_ref().map(|d| Got {
                size: d.size,
                durations: d.durations.clone(),
            });
            let answer = self.cache.fetched(&f.emote, f.via, got);
            if let (Answer::Place { cells }, Some(d)) = (answer, decoded) {
                let atlas = self.atlas.get_or_insert_with(|| {
                    let t = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("ddi-chat-emotes"),
                        size: wgpu::Extent3d {
                            width: ATLAS[0],
                            height: ATLAS[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    painter.set_external_image(device, ATLAS_SOURCE, Some(&t));
                    t
                });
                for (k, cell) in cells.into_iter().enumerate() {
                    if let Some(frame) = d.frame(k) {
                        write_cell(queue, atlas, cell, d.size, frame);
                    }
                }
            }
        }
        let anim_ms = self.tick_clock(now);
        let line = area.line;
        let style = TextStyle::new(FONT, line * TEXT_SIZE);
        {
            let mut measure = painter.measure(1.0);
            let cache = &self.cache;
            self.overlay.advance(
                now,
                area,
                &|e| cache.readiness(e),
                &mut |t| measure.measure(t, &style).advance,
            );
        }
        self.list.clear();
        let atlas_id = painter.texture(TextureKey {
            source: ATLAS_SOURCE.into(),
            max_side: 0,
            blurred: false,
        });
        let mut drawn = ChatDrawn {
            shed: self.shed,
            stats: self.overlay.stats(),
            images: self.cache.counts(),
            animated: self.cache.animated(),
            anim_ms,
            shown: Some((self.overlay.options().level, self.overlay.options().style)),
            opacity: 0.0,
            ..ChatDrawn::default()
        };
        // The text first (its shadow, then it), the emotes after, all from
        // the atlas: one batch.
        let mut emotes: Vec<Quad> = Vec::new();
        let shadow = (line / 24.0).max(1.0);
        for d in self.overlay.drawn(now) {
            drawn.items += 1;
            let s = d.scale;
            let h = line * s;
            let text_style = TextStyle::new(FONT, line * TEXT_SIZE * s);
            let ty = d.y + (h - text_style.line_height()) / 2.0;
            let mut words = String::new();
            for l in d.laid {
                let x = d.x + l.x * s;
                match &l.what {
                    Glyphs::Name(t) | Glyphs::Text(t) | Glyphs::Count(t) => {
                        let color = match &l.what {
                            Glyphs::Name(_) => NAME,
                            Glyphs::Count(_) => COUNT,
                            _ => TEXT,
                        };
                        let [r, g, b] = SHADOW;
                        self.list
                            .text(t.as_str(), &text_style, x + shadow, ty + shadow, [r, g, b, 0.7 * d.alpha]);
                        let [r, g, b] = color;
                        self.list.text(t.as_str(), &text_style, x, ty, [r, g, b, d.alpha]);
                        drawn.runs += 1;
                        if !matches!(l.what, Glyphs::Name(_)) {
                            words.push_str(t);
                        }
                    }
                    Glyphs::Emote(e) => {
                        let Some(cell) = self.cache.cell_at(e, anim_ms) else {
                            continue;
                        };
                        let pad = h * 0.04;
                        drawn.emotes_drawn.push(EmoteDrawn {
                            item: d.id,
                            rect: [x, d.y + pad, x + l.width * s, d.y + h - pad],
                            cell: [cell[0], cell[1]],
                            animated: e.animated,
                        });
                        let [cx, cy, cw, ch] = cell.map(|v| v as f32);
                        let [aw, ah] = ATLAS.map(|v| v as f32);
                        let pad = h * 0.04;
                        emotes.push(Quad {
                            texture: Some(atlas_id),
                            uv: [cx / aw, cy / ah, (cx + cw) / aw, (cy + ch) / ah],
                            ..Quad::plain(
                                [x, d.y + pad, x + l.width * s, d.y + h - pad],
                                [1.0, 1.0, 1.0, d.alpha],
                            )
                        });
                        drawn.emotes += 1;
                        words.push('◆');
                    }
                }
            }
            drawn.texts.push(words);
            drawn.boxes.push([d.x, d.y, d.width * s, h]);
        }
        for q in emotes {
            self.list.quad(q);
        }
        let opacity = chat.config().opacity;
        if drawn.items == 0 || opacity <= 0.0 {
            // Nothing to show: no pass, nothing composited.
            renderer.set_chat_layer(device, None);
        } else {
            let layer = match &self.layer {
                Some(t) if t.width() == size[0] && t.height() == size[1] && t.format() == format => {
                    t.clone()
                }
                _ => {
                    let t = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("ddi-chat-layer"),
                        size: wgpu::Extent3d {
                            width: size[0].max(1),
                            height: size[1].max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    });
                    self.layer = Some(t.clone());
                    t
                }
            };
            let view = layer.create_view(&wgpu::TextureViewDescriptor::default());
            painter.draw(
                device,
                queue,
                encoder,
                &view,
                size,
                1.0,
                Some(wgpu::Color::TRANSPARENT),
                &self.list,
                &mut |_, _| {},
            );
            renderer.set_chat_layer(device, Some((&layer, opacity)));
            drawn.opacity = opacity;
        }
        self.drawn = drawn;
    }

    /// The milliseconds the last frame's chat took (the shell's clock).
    pub fn set_ms(&mut self, ms: f64) {
        self.drawn.ms = ms;
    }
}

/// Writes a frame of `size` pixels (`rgba`) into `cell` of the atlas
/// (cut to the cell where it is larger).
fn write_cell(queue: &wgpu::Queue, atlas: &wgpu::Texture, cell: [u32; 4], size: [u32; 2], rgba: &[u8]) {
    let [x, y, cw, ch] = cell;
    let [w, h] = size;
    let (cw, ch) = (cw.min(w), ch.min(h));
    if cw == 0 || ch == 0 || rgba.len() < (w * h * 4) as usize {
        return;
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: atlas,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: cw,
            height: ch,
            depth_or_array_layers: 1,
        },
    );
}

/// The size a shell decodes an emote image of `size` pixels to: the
/// atlas's row height, its width in proportion ([`fit`]).
pub fn decode_size(size: [u32; 2]) -> [u32; 2] {
    fit(size)
}

/// The proxy's way of asking: whether a fetch went through it (for the
/// shells' logs).
pub fn proxied(f: &Fetch) -> bool {
    f.via == Via::Proxy
}

/// What a play's frame shows of `config`'s chat (round TW4,
/// [`ddi_chat::overlay::shown`]): Falling where the play shows
/// opponents' small fields beside its own field (`miniatures`), emotes
/// alone where a `size` target (`ratio` pixels a logical one) leaves no
/// side space beside the field's `band` (a phone held upright).
pub fn shown(
    config: &ddi_session::chat::ChatConfig,
    size: [u32; 2],
    band: [f32; 2],
    ratio: f32,
    miniatures: bool,
) -> (ddi_chat::Level, ddi_chat::Style) {
    let [w, h] = size.map(|v| v as f32);
    let side = Area::side_space(w, line_for([w, h], ratio), band);
    ddi_chat::overlay::shown(config.level, config.style, miniatures, side)
}

/// Where the chat goes in a play: a `size` target, the field's `band`
/// (left and right edges), the miniatures' slots (`[x, y, width,
/// height]`), what it keeps clear of (`keep_out`, `[left, top, right,
/// bottom]`: the receptors' rows and the HUD), `ratio` target pixels a
/// logical one, at `level`; rows taken from the bottom first where the
/// receptors are at the top (`from_bottom`).
pub fn area(
    size: [u32; 2],
    band: [f32; 2],
    slots: &[[f32; 4]],
    keep_out: &[[f32; 4]],
    from_bottom: bool,
    ratio: f32,
    level: ddi_chat::Level,
) -> Area {
    let [w, h] = size.map(|v| v as f32);
    let obstacles: Vec<[f32; 4]> = slots
        .iter()
        .map(|[x, y, sw, sh]| [*x, *y, x + sw, y + sh])
        .collect();
    Area::new([w, h], line_for([w, h], ratio), band, &obstacles, level)
        .keeping_clear(keep_out, from_bottom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_platform::video::Second;
    use ddi_session::chat::{ChatConfig, ChatSource};

    fn view() -> ChatView {
        let chat = ChatLayer::new(ChatConfig {
            level: ddi_chat::Level::Messages,
            style: ddi_chat::Style::Across,
            names: false,
            emotes_from: ddi_chat::EmotesFrom::TwitchAndOthers,
            opacity: 0.6,
            source: ChatSource::Script(ddi_chat::script::Script::builtin("demo").unwrap()),
        });
        ChatView::new(&chat, false)
    }

    /// The chat is shed as the dancing hand's rule reads a late second:
    /// a late second holds new items back until a good one; the play's
    /// first seconds and a one-off stall count for nothing.
    #[test]
    fn new_items_are_held_back_from_a_late_second_to_a_good_one() {
        let mut v = view();
        let late = Second {
            fps: 40.0,
            worst_ms: 40.0,
        };
        let good = Second {
            fps: 60.0,
            worst_ms: 18.0,
        };
        v.second(late, 60.0, 10.0);
        assert!(!v.shedding(), "warming up");
        v.second(late, 60.0, 13.0);
        assert!(v.shedding());
        v.second(good, 60.0, 14.0);
        assert!(!v.shedding());
        let stall = Second {
            fps: 30.0,
            worst_ms: 400.0,
        };
        v.second(stall, 60.0, 15.0);
        assert!(!v.shedding(), "a stall is not load");
    }

    /// Round TW4: animated emotes' clock runs on the host's seconds, is
    /// held on the frame it had while the frames run late (the same
    /// reading that holds new items back), and stands still under reduced
    /// motion.
    #[test]
    fn animation_is_held_from_a_late_second_and_still_when_reduced() {
        let mut v = view();
        assert_eq!(v.tick_clock(10.0), 0);
        assert_eq!(v.tick_clock(10.5), 500);
        let late = Second {
            fps: 40.0,
            worst_ms: 40.0,
        };
        v.second(late, 60.0, 9.0);
        v.second(late, 60.0, 12.0);
        assert!(v.shedding());
        assert_eq!(v.tick_clock(11.0), 500, "held");
        assert_eq!(v.tick_clock(12.0), 500, "held");
        v.second(
            Second {
                fps: 60.0,
                worst_ms: 18.0,
            },
            60.0,
            13.0,
        );
        assert_eq!(v.tick_clock(12.25), 750, "on from where it held");
        let chat = ChatLayer::new(ChatConfig {
            level: ddi_chat::Level::Emotes,
            style: ddi_chat::Style::Falling,
            names: false,
            emotes_from: ddi_chat::EmotesFrom::TwitchAndOthers,
            opacity: 0.6,
            source: ChatSource::Script(ddi_chat::script::Script::builtin("emotes").unwrap()),
        });
        let mut reduced = ChatView::new(&chat, true);
        for t in 0..50 {
            assert_eq!(reduced.tick_clock(f64::from(t) * 0.1), 0);
        }
    }

    /// Round TW4: Falling beside opponents' fields, the choice kept in a
    /// solo play; emotes alone on a phone held upright, where the field
    /// leaves no side space.
    #[test]
    fn opponents_fields_fall_and_an_upright_phone_shows_emotes_alone() {
        use ddi_chat::{Level, Style};
        let chat = ChatLayer::new(ChatConfig {
            level: Level::Messages,
            style: Style::Across,
            names: false,
            emotes_from: ddi_chat::EmotesFrom::TwitchAndOthers,
            opacity: 0.6,
            source: ChatSource::Script(ddi_chat::script::Script::builtin("demo").unwrap()),
        });
        let c = chat.config();
        // 1280×720, a stage's field in the middle.
        let band = [485.0, 795.0];
        assert_eq!(shown(c, [1280, 720], band, 1.0, false), (Level::Messages, Style::Across));
        assert_eq!(shown(c, [1280, 720], band, 1.0, true), (Level::Messages, Style::Falling));
        // A phone held upright (390×844 logical at 3×): the field fills
        // the width.
        let phone = [1170, 2532];
        assert_eq!(
            shown(c, phone, [30.0, 1140.0], 3.0, false),
            (Level::Emotes, Style::Across)
        );
        // The same phone on its side has room beside the field.
        assert_eq!(
            shown(c, [2532, 1170], [900.0, 1630.0], 3.0, false),
            (Level::Messages, Style::Across)
        );
    }

    #[test]
    fn a_line_suits_the_screen() {
        assert_eq!(line_for([1280.0, 720.0], 1.0), 720.0 / 22.0);
        // A phone held upright: a 14th of its width.
        assert_eq!(line_for([1170.0, 2532.0], 3.0), 1170.0 / 14.0);
        assert_eq!(line_for([320.0, 200.0], 1.0), 18.0);
    }
}
