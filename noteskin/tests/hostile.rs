//! Imported skins are untrusted: odd names, broken ini files, redirect
//! loops, deep chains and long inputs must give errors, never panics or
//! runaway work.

use ddi_noteskin::hints::Hints;
use ddi_noteskin::ini::{Ini, string_to_float, string_to_int};
use ddi_noteskin::load::{ENGINE_ELEMENTS, EXPLOSION_PARTS};
use ddi_noteskin::{Button, Folder, NoteDisplay, Skin};

/// A small deterministic generator (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }

    /// Text from fragments that matter to the parsers.
    fn text(&mut self, parts: usize) -> String {
        const BITS: &[&str] = &[
            "[",
            "]",
            "=",
            "\\",
            "\n",
            "\r\n",
            ";",
            "#",
            "//",
            "--",
            "/",
            "-",
            " ",
            "\t",
            "(",
            ")",
            "res ",
            "doubleres",
            "x",
            "0",
            "7",
            "99999999999",
            "Down",
            "Tap Note",
            "Fallback ",
            ".redir",
            ".png",
            ".lua",
            ".txt",
            "_",
            "/",
            "é",
            "\u{feff}",
            "NoteDisplay",
            "Global",
            "FallbackNoteSkin",
            "metrics.ini",
            "e",
            ".",
            "+",
        ];
        (0..parts).map(|_| self.pick(BITS)).collect()
    }
}

#[test]
fn random_text_never_panics_the_parsers() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20_000 {
        let n = (rng.next() % 40) as usize;
        let t = rng.text(n);
        let ini = Ini::parse(&t);
        for s in ini.section_names() {
            let _ = ini.get(s, "x");
        }
        let h = Hints::parse(&t);
        let sheet = h.sheet((rng.next() % 5000) as u32, (rng.next() % 5000) as u32, 8192);
        if sheet.frame_count() > 0 {
            let _ = sheet.frame_rect(rng.next());
        }
        let _ = sheet.frame_size();
        let _ = sheet.from_padded([1.0, 1.0]);
        let _ = string_to_float(&t);
        let _ = string_to_int(&t);
    }
}

#[test]
fn random_skins_load_or_fail_cleanly() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for case in 0..2_000 {
        let names = ["a", "b", "default", "common", "A"];
        let folders: Vec<Folder> = (0..1 + rng.next() % 4)
            .map(|_| {
                let files: Vec<String> = (0..rng.next() % 12)
                    .map(|_| {
                        let n = (rng.next() % 8) as usize;
                        let base = rng.text(n);
                        format!(
                            "{} {}{}",
                            rng.pick(&["Down", "Fallback", "_x", "Left", ""]),
                            base,
                            rng.pick(&[".png", ".redir", ".lua", ".txt", "", "/x.png"])
                        )
                    })
                    .chain(std::iter::once("metrics.ini".to_string()))
                    .collect();
                let texts = files
                    .iter()
                    .filter(|f| f.ends_with(".redir") || f.as_str() == "metrics.ini")
                    .map(|f| {
                        let t = if f == "metrics.ini" {
                            format!(
                                "[Global]\nFallbackNoteSkin={}\n{}",
                                rng.pick(&names),
                                rng.text(10)
                            )
                        } else {
                            let n = (rng.next() % 4) as usize;
                            format!("{} {}", rng.pick(&["Down", "Fallback", ""]), rng.text(n))
                        };
                        (f.clone(), t)
                    })
                    .collect();
                Folder {
                    game: rng.pick(&["dance", "common"]).into(),
                    name: rng.pick(&names).into(),
                    files,
                    texts,
                }
            })
            .collect();
        let Ok(skin) = Skin::load(&folders, "dance", rng.pick(&names)) else {
            continue;
        };
        assert!(skin.chain.len() < 20, "case {case}");
        for b in Button::ALL {
            for e in ENGINE_ELEMENTS.iter().chain(EXPLOSION_PARTS) {
                let _ = skin.resolve(b, e);
            }
            let _ = NoteDisplay::read(&skin, b.name());
        }
    }
}

#[test]
fn long_inputs_stay_linear() {
    // A megabyte of continued lines, a long section name, long names.
    let big = "a=b\\\n".repeat(200_000) + "end";
    let ini = Ini::parse(&format!("[S]\n{big}\n[{}]\nk=v\n", "s".repeat(100_000)));
    assert!(ini.get("S", "a").is_some_and(|v| v.len() > 100_000));
    let name = format!("x {}x{}.png", "9".repeat(50_000), "9".repeat(50_000));
    assert_eq!(Hints::parse(&name).frames, None);
    let parens = "(".repeat(5_000) + "res 1x1";
    assert_eq!(Hints::parse(&parens).res, None);

    // A thousand redirects in a row: stopped after a hundred.
    let mut files = Vec::new();
    let mut texts = Vec::new();
    for i in 0..1000 {
        files.push(format!("Down R{i}.redir"));
        texts.push((format!("Down R{i}.redir"), format!("Down R{}", i + 1)));
    }
    let folders = [Folder {
        game: "dance".into(),
        name: "s".into(),
        files,
        texts,
    }];
    let skin = Skin::load(&folders, "dance", "s").unwrap();
    assert!(skin.get_path("Down", "R0").is_err());
}

#[test]
fn hostile_models_stay_linear_and_bounded() {
    use ddi_noteskin::model::{MAX_BONES, MAX_TRIANGLES, bones, materials, meshes};
    let start = std::time::Instant::now();
    // A vertex whose number runs on for a megabyte: read in one pass.
    let field = format!("{}x{}", "9".repeat(500_000), "9".repeat(500_000));
    let text = format!("Meshes: 1\n\"m\" 0 0\n1\n0 {field} 0 0 0 0 -1\n0\n0\n");
    assert!(meshes(&text).is_ok());
    // Bones chained by name, as many as allowed, and one more.
    let chain = |n: usize| {
        let mut t = format!("Bones: {n}\n");
        for i in 0..n {
            t += &format!("\"b{i}\"\n\"b{}\"\n0 0 0 0 0 0 0\n0\n0\n", i + 1);
        }
        t
    };
    assert_eq!(bones(&chain(MAX_BONES)).unwrap().bones.len(), MAX_BONES);
    assert!(bones(&chain(MAX_BONES + 1)).is_err());
    // Counts with nothing behind them: errors, without reserving for them.
    let blank = "\n".repeat(1_000_000);
    assert!(materials(&format!("Materials: 999999\n{blank}")).is_err());
    assert!(meshes(&format!("Meshes: 999999\n{blank}")).is_err());
    // More triangles than a model may have.
    let tris = "0 0 0 0 0 0 0 1\n".repeat(MAX_TRIANGLES + 1);
    let text = format!(
        "Meshes: 1\n\"m\" 0 0\n1\n0 0 0 0 0 0 -1\n0\n{}\n{tris}",
        MAX_TRIANGLES + 1
    );
    assert!(meshes(&text).is_err());
    assert!(start.elapsed().as_secs_f64() < 5.0, "{:?}", start.elapsed());
}

#[test]
fn big_skins_stay_fast() {
    // 80k files matching one prefix.
    let files: Vec<String> = (0..80_000)
        .map(|i| format!("Down Tap Note {i}.png"))
        .collect();
    let folders = [Folder {
        game: "dance".into(),
        name: "s".into(),
        files,
        texts: Vec::new(),
    }];
    let skin = Skin::load(&folders, "dance", "s").unwrap();
    for b in Button::ALL {
        let _ = skin.resolve(b, "Tap Note");
    }
    assert_eq!(skin.warnings().len(), 1);
    // A redirect to a prefix 20k files share, a hundred times over.
    let mut files: Vec<String> = (0..20_000).map(|i| format!("Down A{i}.png")).collect();
    files.push("Down A.redir".into());
    let folders = [Folder {
        game: "dance".into(),
        name: "s".into(),
        files,
        texts: vec![("Down A.redir".into(), "Down A".into())],
    }];
    let skin = Skin::load(&folders, "dance", "s").unwrap();
    let _ = skin.get_path("Down", "A");
    assert!(skin.warnings().len() <= 100);
    // A megabyte of keys, twenty thousand sections.
    let keys: String = (0..80_000).map(|i| format!("K{i}=v\n")).collect();
    let ini = Ini::parse(&format!("[S]\n{keys}"));
    assert_eq!(ini.get("S", "K79999"), Some("v"));
    let sections: String = (0..20_000).map(|i| format!("[S{i}]\nk=v\n")).collect();
    let mut merged = Ini::parse(&sections);
    merged.merge_under(&Ini::parse(&sections));
    assert_eq!(merged.get("S19999", "k"), Some("v"));
    // A long name of `(res 1x1 ` repeats.
    let name = "(res 1x1 ".repeat(8_000) + ")";
    assert_eq!(Hints::parse(&name).res, Some((1, 1)));
}
