//! Local prepared CPU artifacts, deliberately separate from personal scores.
use super::Request;
use ddi_engine::cpu::{MAX_BYTES, Plan, Profile, Skills};
use serde::Deserialize;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct CpuRequest {
    pub source_file: Option<PathBuf>,
    pub profile: Option<Arc<Plan>>,
    pub level: usize,
    pub seed: u64,
    pub skills: Skills,
}

#[derive(Deserialize)]
struct Prepared {
    source_file: PathBuf,
    chart_index: usize,
    profile: Profile,
}

pub fn read(path: &str, strength: &str, seed: u64) -> Result<Request, String> {
    let level = ddi_engine::cpu::strength(strength)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("CPU artifact exceeds 64 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("CPU artifact exceeds 64 MiB".into());
    }
    let prepared: Prepared =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid CPU artifact: {e}"))?;
    prepared.profile.validate()?;
    if !prepared.source_file.is_absolute() {
        return Err("CPU source path must be absolute".into());
    }
    let mut request = Request::song(
        &format!("cpu-{}", prepared.profile.chart_digest),
        prepared.chart_index,
        true,
    );
    request.cpu = Some(CpuRequest {
        source_file: Some(prepared.source_file),
        profile: Some(Arc::new(Plan::Prepared(prepared.profile))),
        level,
        seed,
        skills: Skills::default(),
    });
    Ok(request)
}

pub fn arguments(args: &[String]) -> Result<Option<Request>, String> {
    let get = |flag: &str| -> Result<Option<&str>, String> {
        args.iter()
            .position(|a| a == flag)
            .map(|i| {
                args.get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .map(String::as_str)
                    .ok_or_else(|| format!("{flag} needs a value"))
            })
            .transpose()
    };
    let automatic = args.iter().any(|a| a == "--cpu=1");
    if automatic {
        if args.iter().any(|a| {
            [
                "--cpu",
                "--edit",
                "--shared",
                "--share",
                "--replay",
                "--calibrate",
                "--auto",
                "--auto=pad",
                "--bias",
            ]
            .contains(&a.as_str())
        }) {
            return Err(
                "--cpu=1 needs --play SONG and cannot be combined with another automated mode"
                    .into(),
            );
        }
        let song = get("--play")?.ok_or("--cpu=1 needs --play SONG")?;
        let chart = get("--chart")?
            .unwrap_or("0")
            .parse()
            .map_err(|_| "CPU chart index must be a nonnegative integer")?;
        let level = ddi_engine::cpu::strength(get("--cpu-strength")?.unwrap_or("normal"))?;
        let seed = get("--cpu-seed")?
            .unwrap_or("1")
            .parse()
            .map_err(|_| "CPU seed must be an unsigned integer")?;
        let mut request = Request::song(song, chart, true);
        request.cpu = Some(CpuRequest {
            source_file: None,
            profile: None,
            level,
            seed,
            skills: Skills::parse(get("--cpu-skills")?.unwrap_or("balanced"))?,
        });
        return Ok(Some(request));
    }
    let Some(path) = get("--cpu")? else {
        if args
            .iter()
            .any(|a| a == "--cpu-strength" || a == "--cpu-seed" || a == "--cpu-skills")
        {
            return Err("CPU settings need --cpu FILE".into());
        }
        return Ok(None);
    };
    if args.iter().any(|a| {
        [
            "--play",
            "--edit",
            "--shared",
            "--share",
            "--chart",
            "--replay",
            "--calibrate",
            "--auto",
            "--auto=pad",
            "--bias",
        ]
        .contains(&a.as_str())
    }) {
        return Err("--cpu selects its own chart and cannot be combined with play/replay/calibration/autoplay flags".into());
    }
    let strength = get("--cpu-strength")?.unwrap_or("normal");
    let seed = get("--cpu-seed")?
        .unwrap_or("1")
        .parse::<u64>()
        .map_err(|_| "CPU seed must be an unsigned integer")?;
    if Skills::parse(get("--cpu-skills")?.unwrap_or("balanced"))? != Skills::default() {
        return Err("CPU skill modifiers require automatic mode (--cpu=1)".into());
    }
    read(path, strength, seed).map(Some)
}
