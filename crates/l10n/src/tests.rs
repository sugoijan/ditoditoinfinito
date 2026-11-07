use super::*;

/// Every message formats in every language, with no Fluent error and
/// with what each variable was given in it.
#[test]
fn every_message_formats_in_every_language() {
    for language in all() {
        assert!(set_language(language.tag));
        for (id, variables) in MESSAGES {
            let args: Vec<(&str, Arg)> = variables
                .iter()
                .map(|(name, kind)| {
                    (
                        *name,
                        match kind {
                            ArgKind::Number => Arg::Number(7.0),
                            ArgKind::String => Arg::Str("⟨value⟩"),
                        },
                    )
                })
                .collect();
            let l = chain(language)
                .into_iter()
                .find(|l| bundle(l).has_message(id))
                .unwrap_or_else(|| panic!("{id} is in no bundle for {}", language.tag));
            let b = bundle(l);
            let pattern = b.get_message(id).and_then(|m| m.value()).unwrap();
            let mut fluent = FluentArgs::new();
            for (name, value) in &args {
                match value {
                    Arg::Number(n) => fluent.set(*name, *n),
                    Arg::Str(s) => fluent.set(*name, *s),
                }
            }
            let mut errors = Vec::new();
            let text = b.format_pattern(pattern, Some(&fluent), &mut errors);
            assert!(errors.is_empty(), "{id} in {}: {errors:?}", language.tag);
            assert!(!text.trim().is_empty(), "{id} in {} is empty", language.tag);
            for (name, kind) in *variables {
                // One only selected on (`$platform`, the shells' wording,
                // `locales/README.md`; an editor's view) is never written.
                if *kind == ArgKind::String && writes(pattern, name) {
                    assert!(
                        text.contains("⟨value⟩"),
                        "{id} in {} leaves out ${name}: {text}",
                        language.tag
                    );
                }
            }
        }
    }
    set_language("en");
}

/// Whether `pattern` writes variable `name` out (as a placeable, in any
/// variant), rather than only selecting on it.
fn writes(pattern: &fluent_syntax::ast::Pattern<&str>, name: &str) -> bool {
    use fluent_syntax::ast::{Expression, InlineExpression, PatternElement};
    fn inline(e: &InlineExpression<&str>, name: &str) -> bool {
        match e {
            InlineExpression::VariableReference { id } => id.name == name,
            InlineExpression::FunctionReference { arguments, .. } => {
                arguments.positional.iter().any(|a| inline(a, name))
            }
            InlineExpression::Placeable { expression } => expression_writes(expression, name),
            _ => false,
        }
    }
    fn expression_writes(e: &Expression<&str>, name: &str) -> bool {
        match e {
            Expression::Select { variants, .. } => variants.iter().any(|v| writes(&v.value, name)),
            Expression::Inline(i) => inline(i, name),
        }
    }
    pattern.elements.iter().any(|e| match e {
        PatternElement::Placeable { expression } => expression_writes(expression, name),
        PatternElement::TextElement { .. } => false,
    })
}

#[test]
fn messages_are_spoken_in_the_current_language() {
    set_language("en");
    assert_eq!(msg::start::singleplayer().as_str(), "Singleplayer");
    assert_eq!(
        msg::options::language_system("English").as_str(),
        "As the system (English)"
    );
    set_language("ja");
    assert_eq!(msg::start::singleplayer().as_str(), "シングルプレイ");
    assert_eq!(
        msg::common::hint_move("↑↓").as_str(),
        "↑↓ 移動",
        "data is never transformed"
    );
    // The game's name, a term, is the same in every language.
    assert_eq!(msg::common::game_name().as_str(), "Dito Dito Infinito");
    set_language("en");
}

#[test]
fn pseudo_languages_mark_messages_and_data() {
    set_language("qps-ploc");
    let text = msg::start::singleplayer();
    // 12 letters, 5 more as padding.
    assert_eq!(text.as_str(), "«Šíñĝļépļáýéŕ~~~~~»");
    // A variable's value is data: as given, inside the message.
    assert_eq!(msg::common::hint_move("↑↓").as_str(), "«↑↓ Móvé~~~»");
    assert_eq!(UiText::data("Song").as_str(), "‹Song›");
    set_language("qps-wide");
    assert_eq!(msg::start::quit().as_str(), "「Ｑｕｉｔ」");
    set_language("en");
    assert_eq!(UiText::data("Song").as_str(), "Song");
}

#[test]
fn the_system_chooses_a_shipped_language() {
    set_system_languages(["fr-FR", "en_GB.UTF-8"]);
    assert_eq!(system_language().tag, "en");
    // Japanese is a preview: never the system's choice, always the
    // player's.
    set_system_languages(["ja-JP"]);
    assert_eq!(choose(None).tag, "en");
    assert_eq!(choose(Some("ja")).tag, "ja");
    assert_eq!(current().tag, "ja");
    // A tag no language has: the system's.
    assert_eq!(choose(Some("xx")).tag, "en");
    // A run's language beats the settings until the player chooses.
    assert!(force("qps-ploc"));
    assert_eq!(choose(Some("ja")).tag, "qps-ploc");
    unforce();
    assert_eq!(choose(Some("ja")).tag, "ja");
    assert!(!force("xx"));
    set_system_languages(Vec::<String>::new());
    set_language("en");
}

#[test]
fn negotiation_matches_regions_and_old_names() {
    static LANGS: &[Language] = &[
        Language {
            tag: "en",
            name: "English",
            status: Status::Shipped,
            fallback: None,
            decimal: ".",
            group: ",",
            min_grouping: 1,
            faces: &[],
        },
        Language {
            tag: "en-GB",
            name: "English (UK)",
            status: Status::Shipped,
            fallback: Some("en"),
            decimal: ".",
            group: ",",
            min_grouping: 1,
            faces: &[],
        },
        Language {
            tag: "fil",
            name: "Filipino",
            status: Status::Shipped,
            fallback: None,
            decimal: ".",
            group: ",",
            min_grouping: 1,
            faces: &[],
        },
    ];
    let all: Vec<&Language> = LANGS.iter().collect();
    let pick = |wanted: &[&str]| {
        let wanted: Vec<String> = wanted.iter().map(|s| s.to_string()).collect();
        negotiate(&wanted, &all).map(|l| l.tag)
    };
    assert_eq!(pick(&["en_GB.UTF-8"]), Some("en-GB"));
    assert_eq!(pick(&["en-US"]), Some("en"));
    assert_eq!(pick(&["tl-PH"]), Some("fil"));
    assert_eq!(pick(&["de", "fil"]), Some("fil"));
    assert_eq!(pick(&["de"]), None);
}

#[test]
fn numbers_take_the_language_s_marks() {
    assert_eq!(number::write("1234", true, ".", ",", 1), "1,234");
    assert_eq!(
        number::write("1234567.25", true, ",", "\u{a0}", 1),
        "1\u{a0}234\u{a0}567,25"
    );
    // Spanish and Polish leave four digits alone.
    assert_eq!(number::write("1234", true, ",", ".", 2), "1234");
    assert_eq!(number::write("12345", true, ",", ".", 2), "12.345");
    assert_eq!(number::write("-1234", false, ".", ",", 1), "-1234");
    assert_eq!(number::write("7", true, ".", ",", 1), "7");
}

#[test]
fn a_regional_variant_falls_back_to_its_parent_then_english() {
    let ja = find("ja").unwrap();
    assert_eq!(
        chain(ja).iter().map(|l| l.tag).collect::<Vec<_>>(),
        ["ja", "en"]
    );
}

#[test]
fn the_language_s_faces_follow_the_theme_s_face() {
    set_language("en");
    assert_eq!(
        font_stack("Iosevka Term, system-ui, sans-serif"),
        "Iosevka Term, system-ui, sans-serif"
    );
    set_language("ja");
    assert_eq!(
        font_stack("Iosevka Term, system-ui, sans-serif"),
        "Iosevka Term, Noto Sans CJK JP, system-ui, sans-serif"
    );
    assert_eq!(font_stack("Iosevka Term"), "Iosevka Term, Noto Sans CJK JP");
    let weights: Vec<u16> = current().face_files().map(|f| f.weight).collect();
    assert_eq!(weights, [400, 600, 700]);
    set_language("en");
}

#[test]
fn a_run_takes_the_language_s_face_only_where_it_has_every_wide_character() {
    let stack = "Iosevka Term, Noto Sans CJK JP, system-ui, sans-serif";
    assert_eq!(face_has("Noto Sans CJK JP", 'あ'), Some(true));
    assert_eq!(face_has("Noto Sans CJK JP", '言'), Some(true));
    assert_eq!(face_has("Noto Sans CJK JP", '鬱'), Some(false));
    assert_eq!(face_has("Noto Sans CJK JP", 'A'), None);
    assert_eq!(face_has("Iosevka Term", 'あ'), None);
    // A message, Latin, kana: as named.
    assert_eq!(stack_for(stack, "言語 (Language)"), stack);
    assert_eq!(stack_for(stack, "Options"), stack);
    assert!(matches!(stack_for(stack, "Options"), Cow::Borrowed(_)));
    // A title with a kanji the cut lacks: the system's faces, whole.
    assert_eq!(
        stack_for(stack, "鬱くしいあなた"),
        "Iosevka Term, system-ui, sans-serif"
    );
}
