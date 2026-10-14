//! Exact shared-engine control roles propose a profile; runtime proofs gate data.
use super::*;
use crate::swf::{ActionFingerprint, Movie};
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum EngineProfile {
    #[serde(rename = "reku-constant-menu/1")]
    RekuConstantMenu,
    #[serde(rename = "reku-constant-menu-early/1")]
    RekuConstantMenuEarly,
}
const SIGNATURES: [(&str, u32, u16, &str); 5] = [
    (
        "root",
        2,
        12,
        "e31b884f5db263c4a86c19e7a5eb8f71572966a77d858189a5a1cb3d031d07aa",
    ),
    (
        "root",
        5,
        12,
        "00472fc658b9fc0f457f9f0425cdb92faabf10bfd91662a61c6ee8749fa02a02",
    ),
    (
        "root/sprite200",
        1,
        12,
        "55979055488391f3d56a9480b6b4d0134b536d2c5a678133dea6f402c330f383",
    ),
    (
        "root/sprite1802",
        1,
        12,
        "1ccb61af5c7df86bb1d30911e5675585be7241fb4212a4b85d2b7eb107cff994",
    ),
    (
        "root",
        4,
        26,
        "81adf7cc1a2f09499ee2566b02e09019b4fd82b7950937b4241ab15d512ac5a1",
    ),
];

// Gen1's reviewed control/placement roles, not a whole-song allowlist.
const EARLY_SIGNATURES: [(&str, u32, u16, &str); 6] = [
    (
        "root",
        2,
        12,
        "0525fd2f5f23b3c4c84b4c6afbf68623140bb658acbb3368127100eb3afa120a",
    ),
    (
        "root",
        6,
        12,
        "30701fc3b947d63010337c075972ff62115779a594e8b0bbb5334ae2a39e9f8c",
    ),
    (
        "root",
        4,
        26,
        "dbf8bc29f0dc4e94d8fe3ff5516ee788bfec8433916980eef92c7654f6112f80",
    ),
    (
        "root",
        5,
        26,
        "2fd1ab9257e0e0835b524b2f86188decfebfac12c08a3854fa5c80381a86b19a",
    ),
    (
        "root/sprite200",
        1,
        12,
        "b364251a626775e58e6c5aec2756571df3e521aab244f751350fad6a4958f139",
    ),
    (
        "root/sprite200",
        3,
        12,
        "48c66a9d65c375b5a689280be6f52c208ddb8cc208967c73634739faa3e63abb",
    ),
];
impl EngineProfile {
    fn signatures(self) -> &'static [(&'static str, u32, u16, &'static str)] {
        match self {
            Self::RekuConstantMenu => &SIGNATURES,
            Self::RekuConstantMenuEarly => &EARLY_SIGNATURES,
        }
    }
    pub(super) fn menu_frame(self) -> f64 {
        match self {
            Self::RekuConstantMenu => 3.,
            Self::RekuConstantMenuEarly => 4.,
        }
    }
    pub(super) fn title_y(self) -> u64 {
        match self {
            Self::RekuConstantMenu => 200,
            Self::RekuConstantMenuEarly => 165,
        }
    }
    pub(super) fn next_y(self) -> u64 {
        match self {
            Self::RekuConstantMenu => 35,
            Self::RekuConstantMenuEarly => 45,
        }
    }
    pub(super) fn discovery_end(self) -> u64 {
        match self {
            Self::RekuConstantMenu => 1200,
            // Offscreen rendering must fit the unchanged process CPU budget.
            // Later starts fail explicitly rather than extending a work's run.
            Self::RekuConstantMenuEarly => 600,
        }
    }
    pub(super) fn render(self) -> bool {
        // Gen1 includes BitmapData.draw in ordinary chart paths. Supply the
        // reviewed offscreen renderer from initialization, without error waivers.
        matches!(self, Self::RekuConstantMenuEarly)
    }
    pub(super) fn mode(self, label: &str) -> Option<Mode> {
        match label {
            "5" => Some(Mode::Five),
            "7" if self == Self::RekuConstantMenu => Some(Mode::Seven),
            _ => None,
        }
    }
}
pub fn identify(
    movie: &Movie,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<EngineProfile> {
    Ok(inspect(movie, checkpoint)?.0)
}
pub(super) fn inspect(
    movie: &Movie,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<(EngineProfile, Vec<ActionFingerprint>)> {
    let roles = SIGNATURES
        .iter()
        .chain(&EARLY_SIGNATURES)
        .map(|(path, frame, code, _)| (*path, *frame, *code))
        .collect::<Vec<_>>();
    let fingerprints = movie.control_fingerprints(&roles, checkpoint)?;
    let profile = identify_fingerprints(movie.header.fps, movie.header.swf_version, &fingerprints)?;
    Ok((profile, fingerprints))
}
fn identify_fingerprints(
    fps: f64,
    version: u8,
    rows: &[ActionFingerprint],
) -> Result<EngineProfile> {
    if fps == 60. && version <= 8 && !rows.iter().any(|r| [72, 82].contains(&r.code)) {
        let matches = [
            EngineProfile::RekuConstantMenu,
            EngineProfile::RekuConstantMenuEarly,
        ]
        .into_iter()
        .filter(|profile| {
            profile
                .signatures()
                .iter()
                .all(|(path, frame, code, hash)| {
                    let mut matching = rows
                        .iter()
                        .filter(|r| r.path == *path && r.frame == *frame && r.code == *code);
                    matching.next().is_some_and(|r| r.sha256 == *hash) && matching.next().is_none()
                })
        })
        .collect::<Vec<_>>();
        if matches.len() == 1 {
            return Ok(matches[0]);
        }
    }
    Err(Error::new(
        "unsupported-native-profile",
        "No reviewed static native menu profile matches this movie",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_profile_requires_all_control_roles_and_rejects_avm2() {
        for profile in [
            EngineProfile::RekuConstantMenu,
            EngineProfile::RekuConstantMenuEarly,
        ] {
            let rows = profile
                .signatures()
                .iter()
                .map(|(path, frame, code, sha)| ActionFingerprint {
                    path: path.to_string(),
                    frame: *frame,
                    code: *code,
                    bytes: 1,
                    sha256: sha.to_string(),
                })
                .collect::<Vec<_>>();
            assert_eq!(identify_fingerprints(60., 8, &rows).unwrap(), profile);
            assert!(identify_fingerprints(30., 8, &rows).is_err());
            assert!(identify_fingerprints(60., 9, &rows).is_err());
            for i in 0..rows.len() {
                let mut changed = rows.clone();
                changed.remove(i);
                assert!(identify_fingerprints(60., 8, &changed).is_err());
                let mut changed = rows.clone();
                changed[i].sha256 = "0".repeat(64);
                assert!(identify_fingerprints(60., 8, &changed).is_err());
                let mut changed = rows.clone();
                changed.push(changed[i].clone());
                assert!(identify_fingerprints(60., 8, &changed).is_err());
            }
            let mut changed = rows.clone();
            changed[0].code = 82;
            assert!(identify_fingerprints(60., 8, &changed).is_err());
        }
        let hybrid = SIGNATURES
            .iter()
            .chain(&EARLY_SIGNATURES)
            .map(|(path, frame, code, sha)| ActionFingerprint {
                path: path.to_string(),
                frame: *frame,
                code: *code,
                bytes: 1,
                sha256: sha.to_string(),
            })
            .collect::<Vec<_>>();
        assert!(identify_fingerprints(60., 8, &hybrid).is_err());
    }
}
