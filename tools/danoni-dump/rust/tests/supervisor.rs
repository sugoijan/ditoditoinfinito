#![cfg(all(feature = "native", target_os = "macos"))]
//! Confinement probes use our own native helper, never a work's code.
use ddi_danoni_export::{assets::*, native::DirectorySource, supervisor::*};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn scratch() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "ddi-supervisor-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&p).unwrap();
    p.canonicalize().unwrap()
}
fn bind(p: &Path, root: &Path) -> Binding {
    Binding {
        namespace: "source".into(),
        path: p.strip_prefix(root).unwrap().to_str().unwrap().into(),
        sha256: sha256(&fs::read(p).unwrap()),
    }
}
fn fixture(root: &Path, operation: &str) -> (DirectorySource, CaptureRequest) {
    let input = root.join("source");
    fs::create_dir(&input).unwrap();
    // Original one-frame 30 Hz blank FWS movie (RECT: 0-bit bounds).
    let mut movie = b"FWS\x06".to_vec();
    movie.extend(18u32.to_le_bytes());
    movie.extend([0x08, 0, 0, 30, 1, 0, 0x40, 0, 0, 0]);
    fs::write(input.join("fixture.swf"), movie).unwrap();
    let config = json!({"movie":"fixture.swf","sourceUrl":format!("https://example.test/{operation}"),"captureFrames":[1],"resources":[]});
    fs::write(
        input.join("config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let source = DirectorySource::new(BTreeMap::from([("source".into(), input.clone())])).unwrap();
    let request = CaptureRequest {
        config: bind(&input.join("config.json"), &input),
        movie: bind(&input.join("fixture.swf"), &input),
        provenance: None,
        limits: Limits::default(),
    };
    (source, request)
}
fn worker() -> WorkerSpec {
    static WORKER: OnceLock<WorkerSpec> = OnceLock::new();
    WORKER.get_or_init(||{
        let dir=scratch();let source=dir.join("helper.rs");
        fs::write(&source,r#"
use std::{fs,io::{self,Write},process::{Command,Stdio},time::Duration};
fn main(){
    let config=std::path::PathBuf::from(std::env::args().nth(1).unwrap());let data=fs::read_to_string(&config).unwrap();let input=config.parent().unwrap();let root=input.parent().unwrap();
    fs::write("pid.txt",std::process::id().to_string()).unwrap();
    if data.contains("/sleep"){std::thread::sleep(Duration::from_secs(30));}
    if data.contains("/memory"){let mut v=vec![0u8;256*1024*1024];for(i,x)in v.iter_mut().enumerate(){*x=(i%251)as u8;}std::hint::black_box(&v);std::thread::sleep(Duration::from_secs(30));}
    if data.contains("/cpu"){let mut x=1u64;loop{x=x.wrapping_mul(17).wrapping_add(3);std::hint::black_box(x);}}
    if data.contains("/crash"){std::process::abort();}
    if data.contains("/file"){let mut f=fs::File::create("large.bin").unwrap();loop{f.write_all(&[0;1024]).unwrap();}}
    if data.contains("/output-symlink"){std::os::unix::fs::symlink(root.parent().unwrap().join("outside-secret"),"escape").unwrap();std::thread::sleep(Duration::from_secs(30));}
    if data.contains("/aggregate"){for i in 0..8{fs::write(format!("extra-{i}"),vec![0;1024*1024]).unwrap();}std::thread::sleep(Duration::from_secs(30));}
    if data.contains("/probe"){
        let outside=root.parent().unwrap().join("outside-secret");
        let read=fs::read(&outside).is_ok();let input_read=fs::read(input.join("fixture.swf")).is_ok();let input_write=fs::write(input.join("fixture.swf"),b"changed").is_ok();
        let outside_write=fs::write(&outside,b"changed").is_ok();let link_read=fs::read(input.join("escape")).is_ok();
        let at=data.find("probe?port=").unwrap()+"probe?port=".len();
        let port=data[at..].split('"').next().unwrap();
        let network=std::net::TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().unwrap(),Duration::from_millis(100)).is_ok();
        let spawn=Command::new("/usr/bin/true").stdout(Stdio::null()).status().is_ok();let output=fs::write("allowed.txt",b"allowed").is_ok();
        println!("{{\"outsideRead\":{read},\"inputRead\":{input_read},\"inputWrite\":{input_write},\"outsideWrite\":{outside_write},\"linkRead\":{link_read},\"network\":{network},\"spawn\":{spawn},\"outputWrite\":{output}}}");
    }else{io::stdout().write_all(b"malformed snapshot").unwrap();}
}
"#).unwrap();
        let exe=dir.join("own-helper");let status=Command::new("rustc").args(["--edition=2024","-O"]).arg(&source).arg("-o").arg(&exe).status().unwrap();assert!(status.success());
        let identity=dir.join("build.json");fs::write(&identity,serde_json::to_vec(&expected_build_identity()).unwrap()).unwrap();WorkerSpec{executable:exe.clone(),sha256:sha256(&fs::read(exe).unwrap()),build_identity:identity.clone(),build_identity_sha256:sha256(&fs::read(identity).unwrap())}
    }).clone()
}
#[test]
fn original_probe_checks_os_confinement_independently_of_ruffle() {
    let root = scratch();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let (source, request) = fixture(
        &root,
        &format!("probe?port={}", listener.local_addr().unwrap().port()),
    );
    fs::write(root.join("outside-secret"), b"original test data").unwrap();
    let result = capture(&request, &worker(), &source, &root.join("out"), &mut || {
        // Deliberately inject this host-owned link before launch to test the OS
        // policy independently of the staging API (which rejects symlinks).
        // An output link would instead race the output monitor's early kill.
        let escape = root.join("out/inputs/escape");
        if escape.parent().unwrap().is_dir() && fs::symlink_metadata(&escape).is_err() {
            std::os::unix::fs::symlink(root.join("outside-secret"), &escape).unwrap();
        }
        Ok(())
    })
    .unwrap();
    assert!(!result.ready);
    assert!(root.join("out/failure.json").exists());
    assert!(!root.join("out/manifest.json").exists());
    let probe: Value =
        serde_json::from_slice(&fs::read(root.join("out/capture/snapshot.json")).unwrap()).unwrap();
    assert_eq!(probe["outsideRead"], false);
    assert_eq!(probe["outsideWrite"], false);
    assert_eq!(probe["inputRead"], true);
    assert_eq!(probe["inputWrite"], false);
    assert_eq!(probe["linkRead"], false);
    assert_eq!(probe["spawn"], false);
    assert_eq!(probe["outputWrite"], true);
    assert_eq!(
        fs::read(root.join("outside-secret")).unwrap(),
        b"original test data"
    );
    assert_eq!(probe["network"], false);
    assert!(listener.accept().is_err());
}
#[test]
fn wall_memory_file_aggregate_and_cancel_fail_without_readiness() {
    for (operation, kind) in [
        ("sleep", "wall-time-limit"),
        ("memory", "resident-memory-limit"),
        ("file", "file-size-limit"),
        ("aggregate", "output-byte-limit"),
        ("output-symlink", "output-byte-limit"),
        ("sleep", "cancelled"),
        ("cpu", "cpu-kill"),
        ("crash", "exit"),
    ] {
        let root = scratch();
        let (source, mut request) = fixture(&root, operation);
        if kind == "wall-time-limit" {
            request.limits.wall_ms = 20;
        }
        if kind == "resident-memory-limit" {
            request.limits.resident_mib = 32;
        }
        if kind == "file-size-limit" {
            request.limits.file_bytes = 4096;
        }
        if kind == "output-byte-limit" {
            request.limits.output_bytes = 1024 * 1024;
        }
        if kind == "cpu-kill" {
            request.limits.cpu_seconds = 1;
            request.limits.wall_ms = 10000;
        }
        let out = root.join("out");
        let started = Instant::now();
        let result = capture(&request, &worker(), &source, &out, &mut || {
            if kind == "cancelled" && out.join("capture/snapshot.json").exists() {
                Err(ddi_danoni_export::Error {
                    code: "cancelled",
                    message: "test cancellation".into(),
                })
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert!(!result.ready, "{kind}");
        assert!(started.elapsed() < Duration::from_secs(10), "{kind}");
        assert!(!out.join("manifest.json").exists());
        assert!(out.join("failure.json").exists());
        if kind == "cpu-kill" {
            let signal = result.run["signal"].as_i64().unwrap();
            assert!([libc::SIGKILL as i64, libc::SIGXCPU as i64].contains(&signal));
            assert!(result.run["elapsedSeconds"].as_f64().unwrap() < 5.0);
        } else if kind != "exit" {
            assert_eq!(
                result.run["termination"], kind,
                "{operation}: {}",
                result.run
            );
        } else {
            assert_ne!(result.run["exitCode"], 0);
        }
        assert!(result.run["inputsUnchanged"].as_bool().unwrap());
        if let Ok(pid) = fs::read_to_string(out.join("capture/pid.txt")) {
            let pid: libc::pid_t = pid.parse().unwrap();
            // SAFETY: Signal zero only queries existence of our synthetic child.
            assert_eq!(
                unsafe { libc::kill(pid, 0) },
                -1,
                "Child still exists after {kind}"
            );
        }
    }
}
#[test]
fn invalid_bindings_build_inputs_and_existing_destinations_never_spawn() {
    let root = scratch();
    let (source, mut request) = fixture(&root, "sleep");
    let out = root.join("out");
    let e = capture(&request, &worker(), &source, &out, &mut || {
        Err(ddi_danoni_export::Error {
            code: "cancelled",
            message: "before launch".into(),
        })
    })
    .unwrap_err();
    assert_eq!(e.code, "cancelled");
    assert!(!out.exists());
    request.movie.sha256 = "0".repeat(64);
    assert!(capture(&request, &worker(), &source, &out, &mut || Ok(())).is_err());
    assert!(!out.exists());
    request.movie = bind(&root.join("source/fixture.swf"), &root.join("source"));
    let mut w = worker();
    w.sha256 = "0".repeat(64);
    assert_eq!(
        capture(&request, &w, &source, &out, &mut || Ok(()))
            .unwrap_err()
            .code,
        "unreviewed-native-worker"
    );
    assert!(!out.exists());
    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), b"untouched").unwrap();
    assert!(capture(&request, &worker(), &source, &out, &mut || Ok(())).is_err());
    assert_eq!(fs::read(out.join("keep")).unwrap(), b"untouched");
}

#[test]
fn cancellation_after_complete_transport_prevents_final_readiness() {
    let Some(job) = std::env::var_os("DDI_DANONI_SUPERVISOR_SYNTHETIC_JOB") else {
        eprintln!("SKIP: set DDI_DANONI_SUPERVISOR_SYNTHETIC_JOB to an original synthetic job");
        return;
    };
    let root = scratch();
    let out = root.join("late-cancel");
    let result = capture_job(Path::new(&job), &out, &mut || {
        if out.join("run.json").exists() {
            Err(ddi_danoni_export::Error {
                code: "cancelled",
                message: "after complete transport".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert!(!result.ready);
    assert_eq!(result.run["exitCode"], 0);
    assert_eq!(result.run["termination"], "cancelled");
    assert_eq!(result.failure.unwrap().code, "cancelled");
    assert!(!out.join("manifest.json").exists());
    let failure: Value =
        serde_json::from_slice(&fs::read(out.join("failure.json")).unwrap()).unwrap();
    assert!(failure["bindings"].is_null());
    assert_eq!(failure["run"]["termination"], "cancelled");
}
