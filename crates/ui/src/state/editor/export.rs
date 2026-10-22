//! A song edited, saved as a copy (both shells' export): its simfile and
//! a file name for it.

use ddi_chart::Song;
use ddi_chart::formats::sm_write;

/// `song` written as `.sm` when `sm`, else `.ssc`: the text and its
/// extension.
pub fn simfile(song: &Song, sm: bool) -> (String, &'static str) {
    if sm {
        (sm_write::write_sm(song), "sm")
    } else {
        (sm_write::write_ssc(song), "ssc")
    }
}

/// A file name from a title: what file systems take.
pub fn file_stem(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "song".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_from_titles() {
        assert_eq!(file_stem("A/B: c?"), "A_B_ c_");
        assert_eq!(file_stem(" .. "), "song");
    }
}
