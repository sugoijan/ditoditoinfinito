//! A battle's shocks under the bundled skins (our `Tap Shock` element,
//! `docs/plans/noteskin-authoring.md`): Infinito, read as a play reads
//! it, draws a shock with its lane-shaped shock arrow and a chart's own
//! mine with another element, the spiked band wherever the play may be
//! given shocks, whatever its `ShockArrows` parameter says; a solo play
//! loads no shock element and its mines follow the parameter; a bundled
//! skin without the element has none (its shocks are its mines). No GPU.

mod common;

use ddi_chart::Layout;
use ddi_noteskin::build::TextureKey;
use ddi_noteskin::lua::Env;
use ddi_noteskin::params::Value;
use ddi_render::noteskin::TAP_SHOCK;

/// The file names of the textures skin `key`'s element `name` of lane
/// `lane` draws, loaded with `arrows` (Infinito's parameter) for a play
/// that may (`shocks`) be given shocks.
fn textures(key: &str, lane: usize, name: &str, arrows: bool, shocks: bool) -> Vec<String> {
    let layout = Layout::dance_single();
    let env = Env {
        shocks,
        params: [("ShockArrows".to_string(), Value::Bool(arrows))]
            .into_iter()
            .collect(),
        ..Env::default()
    };
    let reading = ddi_desktop::play::skins::Reading::start(key, &layout, env, None);
    let read = loop {
        if let Some(r) = reading.poll() {
            break r;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let prepared = read
        .and_then(|r| ddi_desktop::play::skins::build(r, &layout))
        .unwrap_or_else(|| panic!("{key} is read"));
    assert_eq!(prepared.load_error, None, "{key}");
    prepared
        .play
        .element_texture_keys(lane, name)
        .into_iter()
        .map(|k| match k {
            TextureKey::Skin { path, .. } => path,
            TextureKey::BuiltIn(n) => n,
        })
        .collect()
}

#[test]
fn infinito_draws_a_shock_and_a_chart_s_mine_with_two_elements() {
    common::indexes();
    if ddi_desktop::library::noteskins::bundled().is_empty() {
        eprintln!("no bundled noteskins' index: skipped");
        return;
    }
    let infinito = ddi_noteskin::bundled::INFINITO;
    let has = |files: &[String], part: &str| files.iter().any(|f| f.contains(part));
    // Lane 0 is Left.
    for arrows in [false, true] {
        // A stage with attacks: the shock is the lane-shaped arrow, the
        // chart's own mine the shared spiked band, whatever the parameter.
        let shock = textures(infinito, 0, TAP_SHOCK, arrows, true);
        assert!(has(&shock, "_spiral mine left base"), "{arrows}: {shock:?}");
        assert!(!has(&shock, "_spiral mine shared base"), "{arrows}: {shock:?}");
        let mine = textures(infinito, 0, "Tap Mine", arrows, true);
        assert!(has(&mine, "_spiral mine shared base"), "{arrows}: {mine:?}");
        assert!(!has(&mine, "_spiral mine left base"), "{arrows}: {mine:?}");
        // A solo play: no shock element is loaded, and the mine is what
        // the parameter says.
        assert!(textures(infinito, 0, TAP_SHOCK, arrows, false).is_empty());
        let solo = textures(infinito, 0, "Tap Mine", arrows, false);
        assert_eq!(has(&solo, "_spiral mine left base"), arrows, "{solo:?}");
        assert_eq!(has(&solo, "_spiral mine shared base"), !arrows, "{solo:?}");
    }
    // Every lane's shock is its own button's shape.
    for (lane, shape) in ["left", "down", "up", "right"].into_iter().enumerate() {
        let shock = textures(infinito, lane, TAP_SHOCK, false, true);
        assert!(
            has(&shock, &format!("_spiral mine {shape} base")),
            "{shape}: {shock:?}"
        );
    }
    // A bundled skin without the element: none, in a stage too (its
    // shocks are drawn as its mines).
    for key in ["bundled:dance/delta", "bundled:dance/peters-ddr"] {
        assert!(textures(key, 0, TAP_SHOCK, false, true).is_empty(), "{key}");
        assert!(!textures(key, 0, "Tap Mine", false, true).is_empty(), "{key}");
    }
}
