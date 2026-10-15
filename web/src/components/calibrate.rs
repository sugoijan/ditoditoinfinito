//! Calibration hub: explains the three tests and launches them on the real
//! gameplay path (`#/calibrate/run?mode=…`). See `crate::calibration` for
//! what each mode measures and how results are applied.

use yew::prelude::*;

use crate::calibration::CalMode;
use crate::router::Route;
use crate::settings::Settings;

pub(crate) enum Msg {
    ResetOffsets,
}

pub(crate) struct Calibrate {
    settings: Settings,
}

impl Component for Calibrate {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Calibrate {
            settings: Settings::load(),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ResetOffsets => {
                self.settings.audio_offset = 0.0;
                self.settings.visual_offset = 0.0;
                self.settings.save();
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let s = &self.settings;
        let run = |mode: CalMode| {
            Route::CalibrateRun {
                mode: mode.id().into(),
                auto: false,
                bias_ms: 0,
            }
            .to_hash()
        };
        html! {
            <main class="shell">
                <h1>{ "Calibration" }</h1>
                <p class="muted">{ format!("Current offsets: audio {:+.0} ms, visual {:+.0} ms.", s.audio_offset * 1000.0, s.visual_offset * 1000.0) }</p>
                <p>{ "Each test plays a generated chart (100 BPM, three clicks and a rest, a click on every note) exactly like a song. While you play, the test measures how early or late your hits are, applies the correction on the fly and keeps measuring until the remaining error is within noise (95% confidence), then stops. Saving adds the correction to the setting." }</p>
                <p>{ "The two offsets are independent, so the guided order is: arrows only (display), then sound only (audio path), then a check with both that should read close to zero." }</p>
                <p class="muted">{ format!("Offsets beyond ±{:.0} ms cannot be measured this way (a press that late cannot be told apart from one on the next note); set those by hand in the options, then calibrate from there.", crate::calibration::MAX_OFFSET * 1000.0) }</p>
                <section class="cal-modes">
                    <div class="cal-mode">
                        <h2>{ "1. Arrows only" }</h2>
                        <p class="muted">{ "Muted. Hit each arrow at the receptor. Measures display lag; saves to the visual offset (only moves what is drawn)." }</p>
                        <a class="button" href={run(CalMode::Visual)}>{ "start guided calibration" }</a>
                    </div>
                    <div class="cal-mode">
                        <h2>{ "2. Sound only" }</h2>
                        <p class="muted">{ "Arrows hidden; press any arrow key on each click. Measures the audio path; saves to the audio offset (the judged timeline)." }</p>
                        <a class="button" href={run(CalMode::Audio)}>{ "start" }</a>
                    </div>
                    <div class="cal-mode">
                        <h2>{ "3. Check: both" }</h2>
                        <p class="muted">{ "Play normally. Should read near zero after 1 and 2; a significant remainder can be saved as a fine-tune to the audio offset." }</p>
                        <a class="button" href={run(CalMode::Combined)}>{ "start" }</a>
                    </div>
                </section>
                <p class="muted">{ "Tip: turn on “show timing error” in the options to see each hit's offset during normal play; the debug overlay shows the running mean." }</p>
                <p>
                    <button onclick={ctx.link().callback(|_| Msg::ResetOffsets)}>{ "reset offsets to 0" }</button>
                    { " · " }<a href={Route::Options.to_hash()}>{ "options" }</a>
                    { " · " }<a href={Route::Home.to_hash()}>{ "← back" }</a>
                </p>
            </main>
        }
    }
}
