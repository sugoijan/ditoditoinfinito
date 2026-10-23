use ddi_danoni_desktop_probe::{Job, audit, resume_job, snapshot, url_job};
use ddi_danoni_export::{import_job::Candidate, managed_store::Store, runtime::HostConfig};
use serde_json::json;
use std::{error::Error, fs, path::PathBuf, thread, time::Duration};

fn finish(job: Job) -> Result<serde_json::Value, Box<dyn Error>> {
    while !job.is_finished() {
        if let Some(progress) = job.take_progress() {
            eprintln!("{progress}");
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(serde_json::to_value(job.finish()?)?)
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3
        || !matches!(args[0].as_str(), "ready" | "url" | "folders")
        || (args[0] == "url" && args.len() != 4)
    {
        return Err(
            "Usage: ready NEW_EXTERNAL_RUN READY_DIRECTORY... | url NEW_EXTERNAL_RUN HOST_JSON URL | folders NEW_EXTERNAL_RUN SONG_FOLDER..."
                .into(),
        );
    }
    if std::env::var_os("DDI_HOME").is_none_or(|v| v.is_empty()) {
        return Err("Set DDI_HOME to an isolated scratch folder".into());
    }
    let root = PathBuf::from(&args[1]);
    let parent = root.parent().ok_or("Output parent")?.canonicalize()?;
    if parent.ancestors().any(|p| p.join(".git").exists()) {
        return Err("Third-party outputs must stay outside Git checkouts".into());
    }
    fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    if args[0] == "folders" {
        let paths = args[2..].iter().map(PathBuf::from).collect::<Vec<_>>();
        let (songs, scan) = ddi_desktop::library::scan_folders(&paths);
        if songs.is_empty()
            || !scan.failed.is_empty()
            || !scan.missing.is_empty()
            || !scan.duplicates.is_empty()
        {
            return Err(format!("Incomplete scan: {scan:?}").into());
        }
        let report = json!({"schema":"ddi-danoni-desktop-probe/1","passed":true,"input":"folders",
            "probeLockSha256":ddi_danoni_export::assets::sha256(include_bytes!("../../../../Cargo.lock")),
            "folders":paths,"playback":audit(&songs)?,"managedImportTested":false});
        fs::write(
            root.join("report.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("{}", root.join("report.json").display());
        return Ok(());
    }
    let library = root.join("library");
    drop(Store::create(&library)?);
    let mut runs = Vec::new();
    if args[0] == "ready" {
        for (i, ready) in args[2..].iter().enumerate() {
            let candidate = Candidate::bind(&PathBuf::from(ready))?;
            let installed = finish(resume_job(candidate.clone(), library.clone())?)?;
            fs::write(
                root.join(format!("install-{i}.json")),
                serde_json::to_vec_pretty(&installed)?,
            )?;
            let repeat = finish(resume_job(candidate, library.clone())?)?;
            if repeat["installation"]["status"] != "unchanged" {
                return Err("Repeat import must be unchanged".into());
            }
            runs.push(json!({"source":ready,"installed":installed,"repeat":repeat}));
        }
    } else {
        let mut config = HostConfig::read(&PathBuf::from(&args[2]))?;
        config.library = library.clone();
        config.staging = root.clone();
        let installed = finish(url_job(config, args[3].clone(), root.join("candidate"))?)?;
        fs::write(
            root.join("install-0.json"),
            serde_json::to_vec_pretty(&installed)?,
        )?;
        let candidate = Candidate::bind(&root.join("candidate"))?;
        let repeat = finish(resume_job(candidate, library.clone())?)?;
        if repeat["installation"]["status"] != "unchanged" {
            return Err("Repeat import must be unchanged".into());
        }
        runs.push(json!({"source":args[3],"installed":installed,"repeat":repeat}));
    }
    let (revision, songs) = snapshot(&library)?;
    if songs.is_empty() {
        return Err("No installed songs".into());
    }
    let result = audit(&songs)?;
    let report = json!({"schema":"ddi-danoni-desktop-probe/1","passed":true,
        "input":args[0],"probeLockSha256":ddi_danoni_export::assets::sha256(include_bytes!("../../../../Cargo.lock")),"revision":revision,"imports":runs,"playback":result});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", root.join("report.json").display());
    Ok(())
}
