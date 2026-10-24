//! Converted charts keep the original chart's visual timeline.

use ddi_chart::formats::danoni::{self, Dos, DosFlags};
use ddi_engine::lyrics::LyricTrack;
use ddi_engine::sprites::SpriteTrack;

#[test]
fn converted_charts_keep_their_lyrics_backgrounds_and_masks() {
    let mut song = danoni::import(&Dos::parse(
        "|musicTitle=Test,Artist|musicUrl=test.ogg|blankFrame=0|difData=5,Hard,3.5|left_data=60|space_data=90|right_data=120|word_data=0,0,Original lyrics|back_data=0,0,background.png|mask_data=0,0,Overlay,,5,6,24,#f00|",
        DosFlags::default(),
    )).unwrap().songs.remove(0);
    song.add_chart_versions();
    for reverse in [false, true] {
        let original = LyricTrack::new(&song, 0, reverse, true);
        let converted = LyricTrack::new(&song, 1, reverse, true);
        assert!(!original.is_empty());
        assert_eq!(original.at(1.0), converted.at(1.0));
        let original = SpriteTrack::new(&song, 0, reverse);
        let converted = SpriteTrack::new(&song, 1, reverse);
        assert!(!original.is_empty());
        assert_eq!(original.sources(), converted.sources());
        assert_eq!(original.at(1.0), converted.at(1.0));
    }
}
