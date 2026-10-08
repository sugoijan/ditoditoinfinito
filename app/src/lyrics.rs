//! Lyrics over the play field (Dancing☆Onigiri works): an HTML overlay,
//! since the HUD font has Latin glyphs only and lyrics are often Japanese.
//! The lines come from [`LyricTrack`]; the DOM is touched only when they
//! change.

use ddi_engine::lyrics::{FIELD_HEIGHT, LyricAlign, LyricLine, LyricTrack};
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

pub(crate) struct LyricsView {
    track: LyricTrack,
    root: HtmlElement,
    /// One element per depth, created on first use.
    lines: Vec<HtmlElement>,
    /// What is shown, on a field of which size (font sizes and the line
    /// width follow it).
    shown: (Vec<LyricLine>, (f64, f64)),
}

impl LyricsView {
    pub(crate) fn new(track: LyricTrack, root: HtmlElement) -> LyricsView {
        root.set_inner_html("");
        LyricsView {
            track,
            root,
            lines: Vec::new(),
            shown: (Vec::new(), (0.0, 0.0)),
        }
    }

    /// Shows the lines at song second `t` over a note field `width` CSS px
    /// wide on a screen `height` px tall. Lines run a little wider than
    /// the field, as danoniplus's run over most of its own.
    pub(crate) fn update(&mut self, t: f64, width: f64, height: f64) {
        let mut lines = self.track.at(t);
        // Fades move in steps of 1%, enough for the eye.
        for l in &mut lines {
            l.opacity = (l.opacity * 100.0).round() / 100.0;
        }
        if self.shown.0 == lines && self.shown.1 == (width, height) {
            return;
        }
        if self.shown.1.0 != width {
            let _ = self
                .root
                .style()
                .set_property("--lyric-width", &format!("{:.0}px", width * 1.4));
        }
        let depths = lines
            .iter()
            .map(|l| usize::from(l.depth) + 1)
            .max()
            .unwrap_or(0);
        while self.lines.len() < depths {
            let Some(el) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.create_element("div").ok())
                .and_then(|e| e.dyn_into::<HtmlElement>().ok())
            else {
                return;
            };
            let _ = self.root.append_child(&el);
            self.lines.push(el);
        }
        for (depth, el) in self.lines.iter().enumerate() {
            let Some(line) = lines.iter().find(|l| usize::from(l.depth) == depth) else {
                el.set_class_name("lyric");
                el.set_text_content(None);
                continue;
            };
            el.set_class_name(if line.top {
                "lyric top"
            } else {
                "lyric bottom"
            });
            el.set_text_content(Some(&line.text));
            let style = el.style();
            let align = match line.align {
                LyricAlign::Left => "left",
                LyricAlign::Center => "center",
                LyricAlign::Right => "right",
            };
            let _ = style.set_property("text-align", align);
            let px = f64::from(line.size / FIELD_HEIGHT) * height;
            let _ = style.set_property("font-size", &format!("{px:.1}px"));
            let _ = style.set_property("opacity", &format!("{}", line.opacity));
        }
        self.shown = (lines, (width, height));
    }

    /// Hides every line (the play ended or has not started).
    pub(crate) fn hide(&mut self) {
        if self.shown.0.is_empty() {
            return;
        }
        for el in &self.lines {
            el.set_text_content(None);
        }
        self.shown.0 = Vec::new();
    }
}

impl Drop for LyricsView {
    fn drop(&mut self) {
        self.root.set_inner_html("");
    }
}
