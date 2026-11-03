use super::*;
use crate::backgrounds::{BgImage, BgSegment, MovieEffect, layers};
use ddi_chart::formats::sm::parse_simfile;

/// `bpms` (`#BPMS`), notes on beats 4 and 68 (measures 1 and 17); 120 BPM
/// puts beat `b` at `b / 2` s.
fn song_with(bpms: &str, tags: &str) -> Song {
    let mut measures = vec!["0000"; 18];
    measures[1] = "1000";
    measures[17] = "1000";
    let notes = measures.join("\n,\n");
    let text = format!(
        "#TITLE:t;#OFFSET:0;#BPMS:{bpms};#BACKGROUND:bg.png;{tags}\n\
         #NOTES:dance-single::Easy:1::\n{notes}\n;"
    );
    parse_simfile(&text, "sm").unwrap()
}

fn song(tags: &str) -> Song {
    song_with("0=120", tags)
}

const POOL: [&str; 2] = ["RandomMovies/P/one.avi", "RandomMovies/P/two.mpg"];

fn pool() -> Vec<String> {
    POOL.iter().map(|s| s.to_string()).collect()
}

/// The first layer with `pool`, `unshowable` and the song's own movies.
fn first(
    s: &Song,
    images: &[&str],
    movies: &[&str],
    unshowable: &[&str],
    pool: &[String],
) -> Vec<BgSegment> {
    let [first, _] = layers(s, images, movies, &[] as &[&str], unshowable, pool, true);
    first
}

fn shown(seg: &[BgSegment]) -> Vec<(f64, String)> {
    seg.iter()
        .map(|s| {
            let what = match &s.image {
                BgImage::Song => "song".to_string(),
                BgImage::File(f) => f.clone(),
                BgImage::Movie(m) => m.rsplit('/').next().unwrap().to_string(),
                other => format!("{other:?}"),
            };
            (s.seconds, what)
        })
        .collect()
}

fn pairs(list: &[(f64, &str)]) -> Vec<(f64, String)> {
    list.iter().map(|(t, s)| (*t, s.to_string())).collect()
}

#[test]
fn a_song_without_changes_plays_the_pool_every_four_measures_from_its_first_note() {
    let seg = first(&song(""), &[], &[], &[], &pool());
    // From the start (beat −1000), then every 16 beats from the first
    // note (beat 4), cut, at rate 1, then the song background at the last
    // note (beat 68).
    assert_eq!(
        shown(&seg),
        pairs(&[
            (f64::NEG_INFINITY, "song"),
            (-500.0, "one.avi"),
            (10.0, "two.mpg"),
            (18.0, "one.avi"),
            (26.0, "two.mpg"),
            (34.0, "song"),
        ])
    );
    for s in &seg[1..5] {
        assert_eq!((s.fade, s.rate, s.effect), (0.0, 1.0, MovieEffect::Normal));
    }
}

#[test]
fn without_a_pool_nothing_changes() {
    let seg = first(&song(""), &[], &[], &[], &[]);
    assert_eq!(shown(&seg), pairs(&[(f64::NEG_INFINITY, "song")]));
}

#[test]
fn the_folders_one_movie_comes_before_random_ones() {
    let seg = first(&song(""), &[], &["clip.avi"], &[], &pool());
    assert_eq!(
        shown(&seg),
        pairs(&[(f64::NEG_INFINITY, "song"), (0.0, "clip.avi")])
    );
    // Even when it cannot play here: StepMania plays it.
    let seg = first(&song(""), &[], &[], &["clip.mkv"], &pool());
    assert_eq!(shown(&seg), pairs(&[(f64::NEG_INFINITY, "song")]));
}

#[test]
fn time_signatures_set_the_measures_and_bpm_changes_on_a_measure_add_one() {
    // 3/4 from beat 20: 12 beats a change from there.
    let seg = first(
        &song("#TIMESIGNATURES:0=4=4,20=3=4;"),
        &[],
        &[],
        &[],
        &pool(),
    );
    let times: Vec<f64> = seg.iter().map(|s| s.seconds).collect();
    assert_eq!(
        times,
        [f64::NEG_INFINITY, -500.0, 10.0, 16.0, 22.0, 28.0, 34.0]
    );
    // A BPM change at beat 40, a measure's start (and one at 42, not).
    // (Three movies, so that it is not the one already showing: a change
    // to what shows is none.)
    let s = song_with("0=120,40=120.5,42=120", "");
    let three: Vec<String> = ["a.avi", "b.avi", "c.avi"]
        .iter()
        .map(|m| format!("RandomMovies/{m}"))
        .collect();
    let seg = first(&s, &[], &[], &[], &three);
    let beats: Vec<f64> = seg[1..]
        .iter()
        .map(|g| s.timing.beat_at(g.seconds).round())
        .collect();
    assert_eq!(beats, [-1000.0, 20.0, 36.0, 40.0, 52.0, 68.0]);
}

#[test]
fn a_random_change_is_replaced_up_to_the_next_change() {
    let s = song("#BGCHANGES:4=a.png=1=0=0=1,20=-random-=1=0=0=1,52=a.png=1=0=0=1;");
    let seg = first(&s, &["a.png"], &[], &[], &pool());
    // StepMania's own quirk: in the song's first time signature the first
    // random change goes to beat −1000, before the changes before it, so
    // a.png stays until the second one (beat 36).
    assert_eq!(
        shown(&seg),
        pairs(&[
            (f64::NEG_INFINITY, "song"),
            (-500.0, "one.avi"),
            (2.0, "a.png"),
            (18.0, "two.mpg"),
            (26.0, "a.png"),
            (34.0, "song"),
        ])
    );
}

#[test]
fn a_random_change_keeps_its_rate_transition_and_effect_not_its_colors() {
    let s = song_with(
        "0=120",
        "#TIMESIGNATURES:0=4=4,8=4=4;\
         #BGCHANGES:4=a.png=1=0=0=1,20=-random-=0.5=1=0=0=OverlayAdd=b.png==#FF0000=;",
    );
    let seg = first(&s, &["a.png", "b.png"], &[], &[], &pool());
    // Outside the first time signature: no beat −1000.
    assert_eq!(
        shown(&seg),
        pairs(&[
            (f64::NEG_INFINITY, "song"),
            (2.0, "a.png"),
            (10.0, "one.avi"),
            (18.0, "two.mpg"),
            (26.0, "one.avi"),
            (34.0, "song"),
        ])
    );
    let made = &seg[2];
    assert_eq!((made.rate, made.fade), (0.5, 1.0));
    assert_eq!(
        made.look.effect,
        crate::backgrounds::effects::Effect::OverlayAdd
    );
    assert_eq!(made.look.file2, None);
    assert_eq!(made.look.color1, [1.0; 4]);
}

#[test]
fn a_last_random_change_after_another_makes_nothing() {
    // StepMania measures the last one up to the first change the earlier
    // one made, which is before it.
    let s = song("#BGCHANGES:8=-random-=1=0=0=1,40=-random-=1=0=0=1,99999=-nosongbg-=1=0=0=0;");
    let seg = first(&s, &[], &[], &[], &pool());
    assert_eq!(
        shown(&seg),
        pairs(&[
            (f64::NEG_INFINITY, "song"),
            (-500.0, "one.avi"),
            (12.0, "two.mpg"),
        ])
    );
}

#[test]
fn a_file_found_nowhere_takes_the_next_random_movie() {
    let s = song(
        "#BGCHANGES:4=gone.png=1=0=0=0=StretchNoLoop,8=a.png=1=0=0=1,12=white reverse flash=1=0=0=1,\
         16=broken.mkv=1=0=0=1,20=-random-=1=0=0=1,36=a.png=1=0=0=1;",
    );
    let seg = first(&s, &["a.png"], &[], &["broken.mkv"], &pool());
    // gone.png takes the first (looping: not its change's effect), the
    // `-random-` change the next; a stock animation and a movie that
    // exists but cannot play show the song background.
    assert_eq!(
        shown(&seg),
        pairs(&[
            (f64::NEG_INFINITY, "song"),
            (-500.0, "two.mpg"),
            (2.0, "one.avi"),
            (4.0, "a.png"),
            (6.0, "song"),
            (18.0, "a.png"),
            (34.0, "song"),
        ])
    );
    assert_eq!(seg[2].effect, MovieEffect::Normal);
    // Without a pool, the song background as before.
    let seg = first(&s, &["a.png"], &[], &["broken.mkv"], &[]);
    assert!(!seg.iter().any(|g| matches!(g.image, BgImage::Movie(_))));
    assert_eq!(seg[1].image, BgImage::File("a.png".into()));
}

#[test]
fn the_pool_is_the_packs_folder_else_the_top_one() {
    let s = song("");
    let shared = [
        "RandomMovies/P/b.AVI",
        "RandomMovies/p/a.ogv",
        "RandomMovies/P/sub/c.avi",
        "RandomMovies/P/d.mp4",
        "RandomMovies/top.mpeg",
        "SongMovies/P/e.avi",
    ];
    let mut got = random_pool(&s, "u-1", "P", &shared, None);
    got.sort();
    assert_eq!(got, ["RandomMovies/P/b.AVI", "RandomMovies/p/a.ogv"]);
    assert_eq!(
        random_pool(&s, "u-1", "Q", &shared, None),
        ["RandomMovies/top.mpeg"]
    );
    assert!(random_pool(&s, "u-1", "Q", &shared[..4], None).is_empty());
}

#[test]
fn the_pool_shuffles_alike_for_a_song_and_keeps_ten() {
    let s = song("");
    let shared: Vec<String> = (0..30)
        .map(|i| format!("RandomMovies/m{i:02}.avi"))
        .collect();
    let a = random_pool(&s, "u-1", "P", &shared, None);
    assert_eq!(a.len(), NUM_BACKGROUNDS);
    let mut reversed = shared.clone();
    reversed.reverse();
    assert_eq!(random_pool(&s, "u-1", "P", &reversed, None), a);
    assert_ne!(random_pool(&s, "u-2", "P", &shared, None), a);
}

#[test]
fn a_genre_mapping_narrows_the_pool_unless_nothing_matches() {
    let shared = [
        "RandomMovies/P/rock1.avi",
        "RandomMovies/P/calm.avi",
        "RandomMovies/P/rock2.avi",
    ];
    let ini = "[GenreToSection]\nRock=Loud\nAmbient=Quiet\n\n[Loud]\nrock1.avi=\nROCK2.avi=1\n\n[Quiet]\nnone.avi=\n";
    let mut rock = random_pool(&song("#GENRE:rock;"), "u-1", "P", &shared, Some(ini));
    rock.sort();
    assert_eq!(
        rock,
        ["RandomMovies/P/rock1.avi", "RandomMovies/P/rock2.avi"]
    );
    assert_eq!(
        random_pool(&song("#GENRE:Ambient;"), "u-1", "P", &shared, Some(ini)).len(),
        3
    );
    assert_eq!(
        random_pool(&song(""), "u-1", "P", &shared, Some(ini)).len(),
        3
    );
}

#[test]
fn a_dancing_onigiri_work_has_none() {
    let mut s = song("");
    s.source.format = SourceFormat::Danoni;
    assert!(random_pool(&s, "u-1", "P", &POOL, None).is_empty());
}

#[test]
fn the_span_is_stepmanias_first_and_last_beat() {
    let s = song("");
    assert_eq!(span(&s), Some((4.0, 68.0)));
    // An edit beside other charts does not count; alone it does.
    let text = "#TITLE:t;#OFFSET:0;#BPMS:0=120;\n\
                #NOTES:dance-single::Easy:1::\n0000\n,\n1000\n;\n\
                #NOTES:dance-single::Edit:1::\n1000\n,\n0000\n,\n0000\n,\n1000\n;";
    let s = parse_simfile(text, "sm").unwrap();
    // One note: its first and last second are one, so the first is 0.
    assert_eq!(span(&s), Some((0.0, 4.0)));
    let edit = "#TITLE:t;#OFFSET:0;#BPMS:0=120;\n\
                #NOTES:dance-single::Edit:1::\n0000\n,\n1000\n,\n0000\n,\n1000\n;";
    assert_eq!(span(&parse_simfile(edit, "sm").unwrap()), Some((4.0, 12.0)));
}
