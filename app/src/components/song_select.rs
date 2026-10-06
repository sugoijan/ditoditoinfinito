//! Song list with difficulty picker and audio preview.

use web_sys::HtmlAudioElement;
use yew::prelude::*;

use crate::router::Route;
use crate::songs::{Manifest, ManifestEntry, asset_url, load_manifest};

pub(crate) enum Msg {
    Loaded(Result<Manifest, String>),
    /// Toggle the preview of a song (by manifest index).
    Preview(usize),
    PreviewEnded,
}

pub(crate) struct SongSelect {
    state: State,
    preview: Option<(usize, HtmlAudioElement)>,
    _ended: Option<gloo::events::EventListener>,
}

enum State {
    Loading,
    Ready(Manifest),
    Failed(String),
}

impl Component for SongSelect {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link()
            .send_future(async { Msg::Loaded(load_manifest().await) });
        SongSelect {
            state: State::Loading,
            preview: None,
            _ended: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(Ok(m)) => self.state = State::Ready(m),
            Msg::Loaded(Err(e)) => self.state = State::Failed(e),
            Msg::Preview(i) => {
                let was_playing = self.stop_preview() == Some(i);
                if was_playing {
                    return true;
                }
                let State::Ready(m) = &self.state else {
                    return false;
                };
                let Some(entry) = m.songs.get(i) else {
                    return false;
                };
                // The <audio> element is fine for previews: ±50 ms is harmless
                // here, and it streams instead of decoding the whole file.
                let Ok(audio) = HtmlAudioElement::new_with_src(&asset_url(&format!(
                    "songs/{}/{}",
                    entry.id, entry.music
                ))) else {
                    return false;
                };
                audio.set_current_time(entry.preview_start);
                audio.set_volume(0.6);
                let _ = audio.play();
                let link = ctx.link().clone();
                self._ended = Some(gloo::events::EventListener::new(
                    &audio,
                    "ended",
                    move |_| link.send_message(Msg::PreviewEnded),
                ));
                self.preview = Some((i, audio));
            }
            Msg::PreviewEnded => {
                self.stop_preview();
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        match &self.state {
            State::Loading => html! { <p class="muted">{ "loading songs…" }</p> },
            State::Failed(e) => {
                html! { <p class="error">{ format!("could not load the song list: {e}") }</p> }
            }
            State::Ready(m) if m.songs.is_empty() => {
                html! { <p class="muted">{ "no songs bundled" }</p> }
            }
            State::Ready(m) => html! {
                <ul class="song-list">
                    { for m.songs.iter().enumerate().map(|(i, e)| {
                        let playing = self.preview.as_ref().is_some_and(|(p, _)| *p == i);
                        song_card(e, playing, ctx.link().callback(move |_| Msg::Preview(i)))
                    }) }
                </ul>
            },
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.stop_preview();
    }
}

impl SongSelect {
    /// Stops any preview; returns the index that was playing.
    fn stop_preview(&mut self) -> Option<usize> {
        self._ended = None;
        let (i, audio) = self.preview.take()?;
        let _ = audio.pause();
        audio.set_src("");
        Some(i)
    }
}

fn song_card(entry: &ManifestEntry, playing: bool, on_preview: Callback<MouseEvent>) -> Html {
    let banner = entry.banner.as_ref().map(|b| {
        let src = asset_url(&format!("songs/{}/{}", entry.id, b));
        html! { <img class="song-banner" src={src} alt="" loading="lazy" /> }
    });
    let charts: Vec<_> = entry
        .charts
        .iter()
        .filter(|c| c.layout == "dance-single")
        .collect();
    html! {
        <li class="song-card">
            { for banner }
            <div class="song-meta">
                <div class="song-title">
                    { &entry.title }
                    { " " }
                    <button class="preview-button" onclick={on_preview} title="preview">{ if playing { "■" } else { "▶" } }</button>
                </div>
                <div class="muted song-sub">{ format!("{} · {} BPM · {}", entry.artist, entry.bpm, entry.credit) }</div>
                <div class="chart-buttons">
                    { for charts.iter().map(|c| {
                        let href = Route::Play { song: entry.id.clone(), chart: c.index, force_gl: false, auto: false, bias_ms: 0 }.to_hash();
                        let class = format!("chart-button diff-{}", c.difficulty.to_lowercase());
                        html! {
                            <a class={class} href={href} title={format!("{} · {} notes · steps by {}", c.name, c.notes, c.credit)}>
                                <span class="chart-diff">{ &c.difficulty }</span>
                                <span class="chart-meter">{ c.meter }</span>
                            </a>
                        }
                    }) }
                </div>
            </div>
        </li>
    }
}
