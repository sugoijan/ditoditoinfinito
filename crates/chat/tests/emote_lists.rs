//! The emote providers' global sets as they answered on 2026-11-20,
//! recorded once with the maintainer's leave and kept outside the
//! repository (third-party data: never committed), read by the real
//! parsers (round TW5, `docs/plans/multiplayer-twitch.md`). Opt-in:
//!
//! `DDI_EMOTE_LISTS=<dir> cargo test -p ddi-chat --test emote_lists -- --ignored --nocapture`
//!
//! where `<dir>` has `7tv-global.json`, `bttv-global.json` and
//! `ffz-global.json`. What they held then: 7TV 45 emotes, 21 animated, 3
//! of them zero-width (left out: 42, 18 animated), 4 under an alias;
//! BetterTTV 65, one animated, 9 modifiers (left out: 56); FrankerFaceZ
//! 23 in three sets, 15 in its two default sets, 2 of them hidden and 4
//! modifiers (left out: 9), none animated.

use ddi_chat::emotes::{bttv_global, ffz_global, seven_tv_global};

fn list(name: &str) -> Option<String> {
    let dir = std::env::var_os("DDI_EMOTE_LISTS")?;
    Some(std::fs::read_to_string(std::path::Path::new(&dir).join(name)).expect(name))
}

#[test]
#[ignore]
fn the_recorded_global_sets_are_read() {
    let (Some(seven), Some(bttv), Some(ffz)) = (
        list("7tv-global.json"),
        list("bttv-global.json"),
        list("ffz-global.json"),
    ) else {
        eprintln!("DDI_EMOTE_LISTS is not set: nothing to read");
        return;
    };
    let animated = |l: &ddi_chat::emotes::List| l.iter().filter(|(_, e, _)| e.animated).count();

    let s = seven_tv_global(&seven);
    eprintln!("7TV: {} read, {} animated", s.len(), animated(&s));
    assert_eq!((s.len(), animated(&s)), (42, 18));
    // The set's name is the word; its file the 2x WebP (the animated one
    // of an animated emote, else the still), on 7TV's image host.
    let raw: serde_json::Value = serde_json::from_str(&seven).unwrap();
    for e in raw["emotes"].as_array().unwrap().iter().filter(|e| e["flags"] == 0) {
        let word = e["name"].as_str().unwrap();
        let (_, emote, image) = s.iter().find(|(w, ..)| w == word).unwrap_or_else(|| panic!("{word}"));
        assert_eq!(emote.animated, e["data"]["animated"] == true, "{word}");
        assert!(image.url.starts_with("https://cdn.7tv.app/emote/"), "{}", image.url);
        // (A still emote's still file is its `2x.webp` itself.)
        assert!(image.url.ends_with("/2x.webp"), "{word}: {}", image.url);
        assert!(image.proxy.as_deref().is_some_and(|p| p.starts_with("emotes/7tv/emote/")));
    }

    let b = bttv_global(&bttv);
    eprintln!("BetterTTV: {} read, {} animated", b.len(), animated(&b));
    assert_eq!((b.len(), animated(&b)), (56, 1));
    assert!(b.iter().all(|(_, _, i)| i.url.starts_with("https://cdn.betterttv.net/emote/") && i.url.ends_with("/2x")));

    let f = ffz_global(&ffz);
    eprintln!("FrankerFaceZ: {} read, {} animated", f.len(), animated(&f));
    assert_eq!((f.len(), animated(&f)), (9, 0));
    assert!(f.iter().all(|(_, _, i)| i.url.starts_with("https://cdn.frankerfacez.com/emote/") && i.url.ends_with("/2")));
}
