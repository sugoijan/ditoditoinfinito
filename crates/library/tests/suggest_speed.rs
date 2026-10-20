//! Suggestions over a large library (the step 4 review's bench): 23,000
//! titles, artists and packs. Prints the time a keystroke takes (run with
//! `--release -- --nocapture` for the real numbers) and checks that
//! typing on, which searches only what the last query matched, gives what
//! a fresh search gives.

use ddi_library::suggest::{Kind, Suggester};

fn library() -> (Vec<String>, Vec<String>, Vec<String>) {
    let adj = [
        "Silver", "Crimson", "Midnight", "Electric", "Paper", "Velvet", "Neon", "Quiet", "Broken",
        "Golden", "Hidden", "Frozen", "Lucky", "Distant", "Rapid", "Gentle", "Wild", "Hollow",
        "Bright", "Last",
    ];
    let noun = [
        "Sakura",
        "Drops",
        "Dream",
        "Signal",
        "Garden",
        "Engine",
        "Harbor",
        "Comet",
        "Mirror",
        "Rain",
        "さくら",
        "ゆめ",
        "東京",
    ];
    let titles = (0..20_000)
        .map(|i| {
            format!(
                "{} {} {} {}",
                adj[i % 20],
                noun[(i / 20) % 13],
                adj[(i * 7) % 20],
                i
            )
        })
        .collect();
    let artists = (0..20_000)
        .map(|i| format!("Artist number {}", i % 3000))
        .collect();
    let packs = (0..400).map(|i| format!("Pack {i:03}")).collect();
    (titles, artists, packs)
}

fn build<'a>(t: &'a [String], a: &'a [String], p: &'a [String]) -> Suggester {
    let f = |s: &str| s.to_string();
    let c = t
        .iter()
        .map(|t| (t.as_str(), "", Kind::Title))
        .chain(a.iter().map(|a| (a.as_str(), "", Kind::Artist)))
        .chain(p.iter().map(|p| (p.as_str(), "", Kind::Pack)));
    Suggester::new(c, &f)
}

#[test]
fn keystrokes_over_a_large_library() {
    let f = |s: &str| s.to_string();
    let (t, a, p) = library();
    let t0 = std::time::Instant::now();
    let s = build(&t, &a, &p);
    eprintln!("build {:?}, {} entries", t0.elapsed(), s.len());
    for q in ["a", "e", "si", "sil", "silver d", "さ", "1", "zzz"] {
        let fresh = build(&t, &a, &p);
        let t0 = std::time::Instant::now();
        let want = fresh.suggest(q, &f, 6);
        eprintln!("{q:?}: {:?} (fresh)", t0.elapsed());
        // Typed one letter at a time on one suggester: the same answer.
        let mut typed = String::new();
        let mut got = Vec::new();
        for c in q.chars() {
            typed.push(c);
            let t0 = std::time::Instant::now();
            got = s.suggest(&typed, &f, 6);
            eprintln!("  {typed:?}: {:?}", t0.elapsed());
        }
        assert_eq!(got, want, "{q}");
    }
    assert!(s.suggest("silver d", &f, 6)[0].text.starts_with("Silver D"));
}
