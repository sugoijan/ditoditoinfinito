#![cfg(all(
    feature = "native",
    any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64"))
))]
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
    WORKER
        .get_or_init(|| {
            let dir = scratch();
            let source = dir.join("helper.rs");
            fs::write(&source, include_str!("support/supervisor_helper.rs")).unwrap();
            let exe = dir.join("own-helper");
            let status = Command::new("rustc")
                .args(["--edition=2024", "-O"])
                .arg(&source)
                .arg("-o")
                .arg(&exe)
                .status()
                .unwrap();
            assert!(status.success());
            let identity = dir.join("build.json");
            fs::write(
                &identity,
                serde_json::to_vec(&expected_build_identity()).unwrap(),
            )
            .unwrap();
            WorkerSpec {
                executable: exe.clone(),
                sha256: sha256(&fs::read(exe).unwrap()),
                build_identity: identity.clone(),
                build_identity_sha256: sha256(&fs::read(identity).unwrap()),
            }
        })
        .clone()
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
        #[cfg(target_os = "linux")]
        ("memfd-growth", "resident-memory-limit"),
        ("file", "file-size-limit"),
        ("aggregate", "output-byte-limit"),
        #[cfg(target_os = "macos")]
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
        } else if kind == "file-size-limit" && result.run["signal"].is_null() {
            // An embedding host can inherit SIGXFSZ ignored/blocked. EFBIG is
            // also the kernel enforcing this budget, not a supervisor failure.
            assert_eq!(result.run["exitCode"], 70);
            assert!(
                fs::read_to_string(out.join("capture/stderr.txt"))
                    .unwrap()
                    .contains("file-write-denied:27")
            );
            assert_eq!(
                fs::metadata(out.join("capture/large.bin")).unwrap().len(),
                4096
            );
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
        if operation == "memfd-growth" {
            assert!(result.run["peakAnonymousFileKiB"].as_u64().unwrap() > 0);
            assert!(result.run["peakRssKiB"].as_u64().unwrap() < 32 * 1024);
            assert!(result.run["peakAccountedMemoryKiB"].as_u64().unwrap() > 32 * 1024);
        }
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

#[cfg(target_os = "linux")]
#[test]
fn linux_threads_work_but_process_ipc_descriptor_and_filesystem_escapes_fail() {
    use std::os::fd::AsRawFd;
    let root = scratch();
    let outside = root.join("outside-secret");
    fs::write(&outside, b"untouched").unwrap();
    let file = fs::File::open(&outside).unwrap();
    // Deliberately non-CLOEXEC, above stdin/stdout/stderr. The child must not
    // inherit a desktop host's open files, even if that host forgot CLOEXEC.
    // SAFETY: Valid owned input fd, test-only target slot, closed after capture.
    let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 100) };
    assert!(fd >= 100);
    let (source, request) = fixture(&root, &format!("linux-probe?fd={fd}"));
    let result = capture(&request, &worker(), &source, &root.join("out"), &mut || {
        Ok(())
    });
    // SAFETY: Release only this test's duplicate descriptor.
    unsafe {
        libc::close(fd);
    }
    let result = result.unwrap();
    assert!(!result.ready);
    assert_eq!(result.run["exitCode"], 0, "{}", result.run);
    let report = fs::read_to_string(root.join("out/capture/snapshot.json")).unwrap();
    for line in report.lines() {
        if line.starts_with("clone3=") {
            assert_eq!(line, "clone3=-1:38");
        } else if line.starts_with("thread=") {
            assert_eq!(line, "thread=true");
        } else if line.starts_with("inherited-fd=") {
            assert_eq!(line, "inherited-fd=-1");
        } else if line.contains(':') {
            assert!(line.ends_with("=-1:1"), "{line}");
        } else {
            assert!(line.ends_with("=false"), "{line}");
        }
    }
    assert_eq!(report.lines().count(), 33, "{report}");
    assert_eq!(fs::read(outside).unwrap(), b"untouched");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_missing_required_kernel_support_never_stages_or_executes() {
    const CHILD: &str = "DDI_TEST_DENY_LANDLOCK";
    if std::env::var_os(CHILD).is_some() {
        let root = scratch();
        let (source, request) = fixture(&root, "sleep");
        // Intentionally invalid worker: capability admission must precede it.
        let w = WorkerSpec {
            executable: root.join("absent"),
            sha256: "0".repeat(64),
            build_identity: root.join("absent-json"),
            build_identity_sha256: "0".repeat(64),
        };
        let out = root.join("out");
        let error = capture(&request, &w, &source, &out, &mut || Ok(())).unwrap_err();
        assert_eq!(error.code, "unsupported-native-confinement");
        assert!(!out.exists());
        return;
    }
    use std::os::unix::process::CommandExt;
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "linux_missing_required_kernel_support_never_stages_or_executes",
        ])
        .env(CHILD, "1");
    // SAFETY: Test child alone receives a stricter filter that simulates a kernel
    // without Landlock. The fixed stack array survives the synchronous syscall.
    unsafe {
        child.pre_exec(|| {
            let mut filter = [
                libc::sock_filter {
                    code: 0x20,
                    jt: 0,
                    jf: 0,
                    k: 0,
                },
                libc::sock_filter {
                    code: 0x15,
                    jt: 0,
                    jf: 1,
                    k: libc::SYS_landlock_create_ruleset as u32,
                },
                libc::sock_filter {
                    code: 0x06,
                    jt: 0,
                    jf: 0,
                    k: 0x00050000 | libc::ENOSYS as u32,
                },
                libc::sock_filter {
                    code: 0x06,
                    jt: 0,
                    jf: 0,
                    k: 0x7fff0000,
                },
            ];
            let program = libc::sock_fprog {
                len: filter.len() as u16,
                filter: filter.as_mut_ptr(),
            };
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(libc::PR_SET_SECCOMP, 2, &program) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    assert!(child.status().unwrap().success());
}
