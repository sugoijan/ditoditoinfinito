//! Exact shared-engine control roles propose a profile; runtime proofs gate data.
use super::*;
use crate::swf::{ActionFingerprint, Movie};
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum EngineProfile {
    #[serde(rename = "reku-constant-menu/1")]
    RekuConstantMenu,
    #[serde(rename = "reku-constant-menu-early/1")]
    RekuConstantMenuEarly,
    #[serde(rename = "reku-scheduled-menu/1")]
    RekuScheduledMenu,
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
// Two complete shared control combinations. Mixing individually known role
// hashes is insufficient: each combination must match exactly and uniquely.
const SCHEDULED_SIGNATURES: [(&str, u32, u16, &str); 6] = [
    (
        "root",
        2,
        12,
        "4c27dd655aa86f52d4221ac05c86d1f4bb7e494a9993ef9a9cbe26129d29995f",
    ),
    (
        "root",
        4,
        12,
        "68db28cb21dde5825d9e6762ccec3d27c27f7e9c94dda4f635db10de5ab5fe89",
    ),
    (
        "root",
        5,
        12,
        "78ac47f69fbc9b958ca1c58b7222df79f18224947bc9d777b84835aaf79a82e1",
    ),
    (
        "root/sprite200",
        1,
        12,
        "6512903cbab7408065721d3f72d4deff993260b776bcc619f8358aa1b4a266d8",
    ),
    (
        "root/sprite1802",
        1,
        12,
        "943d4b523adaac952e3a13e09270ff36571261421508707996b97a519c0928d4",
    ),
    (
        "root",
        4,
        26,
        "c5f54470d0aef1d27fad117b8924ca124be1e69be3ac57c6769f00c1e1a22090",
    ),
];
const SCHEDULED_ALTERNATE: [(&str, u32, u16, &str); 6] = {
    let mut rows = SCHEDULED_SIGNATURES;
    rows[2].3 = "afbe7e98c9455cd2b075b8f1a65ae4ae236a856ffd1eb120c1e3fe0f4bab1f02";
    rows
};
type Signature = (&'static str, u32, u16, &'static str);
impl EngineProfile {
    fn variants(self) -> &'static [&'static [Signature]] {
        match self {
            Self::RekuConstantMenu => &[&SIGNATURES],
            Self::RekuConstantMenuEarly => &[&EARLY_SIGNATURES],
            Self::RekuScheduledMenu => &[&SCHEDULED_SIGNATURES, &SCHEDULED_ALTERNATE],
        }
    }
    pub fn options(self, mode: Mode) -> Result<ProjectionOptions> {
        let mut options = super::options(mode)?;
        if self == Self::RekuScheduledMenu {
            options.family = Family::ScheduledCounter;
        }
        if mode == Mode::Seven && self != Self::RekuConstantMenu {
            return Err(invalid(
                "This profile does not prove automatic seven-key controls",
            ));
        }
        Ok(options)
    }
    pub(super) fn menu_frame(self) -> f64 {
        match self {
            Self::RekuConstantMenu | Self::RekuScheduledMenu => 3.,
            Self::RekuConstantMenuEarly => 4.,
        }
    }
    pub(super) fn title_y(self) -> u64 {
        match self {
            Self::RekuConstantMenu => 200,
            Self::RekuConstantMenuEarly => 165,
            Self::RekuScheduledMenu => 193,
        }
    }
    pub(super) fn next_y(self) -> u64 {
        match self {
            Self::RekuConstantMenu => 35,
            Self::RekuConstantMenuEarly => 45,
            Self::RekuScheduledMenu => 69,
        }
    }
    pub(super) fn discovery_end(self) -> u64 {
        match self {
            Self::RekuConstantMenu => 1200,
            // Offscreen rendering must fit the unchanged process CPU budget.
            // Later starts fail explicitly rather than extending a work's run.
            Self::RekuConstantMenuEarly | Self::RekuScheduledMenu => 600,
        }
    }
    pub(super) fn avm_execution_ms(self) -> u64 {
        if self == Self::RekuScheduledMenu {
            10000
        } else {
            2000
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
    let roles = [
        &SIGNATURES[..],
        &EARLY_SIGNATURES[..],
        &SCHEDULED_SIGNATURES[..],
        &SCHEDULED_ALTERNATE[..],
    ]
    .into_iter()
    .flatten()
    .map(|(path, frame, code, _)| (*path, *frame, *code))
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
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
            EngineProfile::RekuScheduledMenu,
        ]
        .into_iter()
        .filter(|profile| {
            profile.variants().iter().any(|roles| {
                roles.iter().all(|(path, frame, code, hash)| {
                    let mut matching = rows
                        .iter()
                        .filter(|r| r.path == *path && r.frame == *frame && r.code == *code);
                    matching.next().is_some_and(|r| r.sha256 == *hash) && matching.next().is_none()
                })
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
    fn unreviewed_multimode_initialization_does_not_inherit_five_key_admission() {
        let mut rows = SCHEDULED_ALTERNATE
            .iter()
            .map(|(path, frame, code, sha)| ActionFingerprint {
                path: path.to_string(),
                frame: *frame,
                code: *code,
                bytes: 1,
                sha256: sha.to_string(),
            })
            .collect::<Vec<_>>();
        rows[0].sha256 = "1c873450251f74c86099dd3aba50b6405373e8fb399ea0eed0559f2072cb61af".into();
        assert!(identify_fingerprints(60., 8, &rows).is_err());
    }
    #[test]
    fn fingerprint_profile_requires_all_control_roles_and_rejects_avm2() {
        for profile in [
            EngineProfile::RekuConstantMenu,
            EngineProfile::RekuConstantMenuEarly,
            EngineProfile::RekuScheduledMenu,
        ] {
            for roles in profile.variants() {
                let rows = roles
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
