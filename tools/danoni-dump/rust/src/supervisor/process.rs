//! Shared Unix resource monitoring, cancellation and owned-process teardown.
use super::*;
use std::{
    mem::MaybeUninit,
    os::unix::process::{CommandExt, ExitStatusExt},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn timestamp() -> Result<String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| invalid(e.to_string()))?
        .as_secs();
    let seconds = libc::time_t::try_from(seconds).map_err(|_| invalid("Timestamp overflow"))?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    // SAFETY: Valid time_t and writable tm; gmtime_r is thread-safe.
    let tm = unsafe {
        if libc::gmtime_r(&seconds, tm.as_mut_ptr()).is_null() {
            return Err(native::io(std::io::Error::last_os_error()));
        }
        tm.assume_init()
    };
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    ))
}
struct Running {
    child: Child,
    armed: bool,
}
impl Running {
    fn kill_group(&mut self) -> bool {
        // SAFETY: The child starts a new session, hence PGID == child PID;
        // only that owned group is signaled. No PID supplied by a work is used.
        let sent = unsafe { libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL) };
        let group_stopped =
            sent == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        let child_stopped = match self.child.kill() {
            Ok(()) => true,
            Err(e) => e.raw_os_error() == Some(libc::ESRCH),
        };
        group_stopped && child_stopped
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if self.armed {
            self.kill_group();
            let _ = self.child.wait();
        }
    }
}
struct MemorySample {
    rss: u64,
    anonymous_files: u64,
}
#[cfg(target_os = "macos")]
fn resident(pid: u32) -> Result<MemorySample> {
    let mut info = MaybeUninit::<libc::proc_taskinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_taskinfo>();
    // SAFETY: Native API receives a valid PID, exact writable struct buffer and size.
    let returned = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    if returned != size as i32 {
        return Err(Error::new(
            "native-memory-monitor",
            "Cannot read child resident memory",
        ));
    }
    // SAFETY: proc_pidinfo filled the complete struct, verified above.
    Ok(MemorySample {
        rss: unsafe { info.assume_init().pti_resident_size },
        anonymous_files: 0,
    })
}
#[cfg(target_os = "linux")]
fn resident(pid: u32) -> Result<MemorySample> {
    // The parent reads only its owned child's proc entry. No procfs is exposed
    // to the worker. A missing/invalid sample takes the same fail-closed path.
    let value = fs::read_to_string(format!("/proc/{pid}/statm")).map_err(native::io)?;
    let pages = value
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| Error::new("native-memory-monitor", "Invalid child RSS"))?;
    // SAFETY: sysconf has no pointer arguments or side effects.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size <= 0 {
        return Err(invalid("Cannot determine page size"));
    }
    let rss = pages
        .checked_mul(size as u64)
        .ok_or_else(|| invalid("RSS overflow"))?;
    // memfd pages written without mapping are not counted in RSS. Include
    // allocated anonymous-file blocks so that path cannot bypass the budget.
    // Mapped memfd pages may be counted twice: deliberately conservative.
    use std::os::unix::fs::MetadataExt;
    let mut seen = std::collections::BTreeSet::new();
    let mut anonymous_files = 0u64;
    for entry in fs::read_dir(format!("/proc/{pid}/fd")).map_err(native::io)? {
        let path = entry.map_err(native::io)?.path();
        let target = match fs::read_link(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(native::io(e)),
        };
        if !target
            .as_os_str()
            .as_encoded_bytes()
            .starts_with(b"/memfd:")
        {
            continue;
        }
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(native::io(e)),
        };
        if seen.insert((meta.dev(), meta.ino())) {
            anonymous_files = anonymous_files.saturating_add(meta.blocks().saturating_mul(512));
        }
    }
    Ok(MemorySample {
        rss,
        anonymous_files,
    })
}
fn output_size(root: &Path, limits: &Limits) -> Result<()> {
    let mut total = 0u64;
    let mut files = 0;
    for e in fs::read_dir(root.join("capture")).map_err(native::io)? {
        let m = fs::symlink_metadata(e.map_err(native::io)?.path()).map_err(native::io)?;
        if !m.is_file() || m.file_type().is_symlink() || m.len() > limits.file_bytes {
            return Err(Error::new(
                "native-output-limit",
                "Invalid or oversized worker output",
            ));
        }
        total = total.saturating_add(m.len());
        files += 1;
    }
    if total > limits.output_bytes || files > 32 {
        return Err(Error::new(
            "native-output-limit",
            "Aggregate output size/count exceeded",
        ));
    }
    Ok(())
}
fn settle_monitor_exit(
    reason: &mut Option<&'static str>,
    error: &mut Option<String>,
    status: std::process::ExitStatus,
    stop_confirmed: bool,
) -> Option<Value> {
    if *reason == Some("memory-monitor-unavailable") && stop_confirmed && status.success() {
        let race = json!({"error":error.take(),"resolution":"Owned group stop confirmed; reaped child had already completed normally"});
        *reason = None;
        Some(race)
    } else {
        None
    }
}
pub(super) fn run(
    root: &Path,
    limits: &Limits,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    let executable = root.join("runtime/ddi-ruffle-probe");
    let input = root.join("inputs");
    let output = root.join("capture");
    let started_at = timestamp()?;
    let stdout = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("snapshot.json"))
        .map_err(native::io)?;
    let stderr = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("stderr.txt"))
        .map_err(native::io)?;
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .args([
                "-D",
                &format!("WORKER={}", executable.display()),
                "-D",
                &format!("INPUT={}", input.display()),
                "-D",
                &format!("OUTPUT={}", output.display()),
                "-f",
            ])
            .arg(root.join("runtime/worker.sb"))
            .arg(executable)
            .arg(input.join("capture.json"));
        command
    };
    #[cfg(target_os = "linux")]
    let sandbox = super::linux::Sandbox::prepare(root)?;
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new(&executable);
        command.arg(input.join("capture.json"));
        command
    };
    command
        .current_dir(&output)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &output)
        .env("TMPDIR", &output)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    #[cfg(target_os = "linux")]
    command
        .env("MESA_SHADER_CACHE_DISABLE", "true")
        .env("LP_NUM_THREADS", "2")
        .env("XDG_RUNTIME_DIR", &output);
    #[cfg(target_os = "linux")]
    let confinement = json!({"landlockAbi":sandbox.abi,"requiredLandlockAbi":6,"seccomp":"x86_64-allowlist/1","parentDeathSignal":libc::SIGKILL});
    #[cfg(target_os = "macos")]
    let confinement = json!({"backend":"sandbox-exec"});
    let cpu = limits.cpu_seconds;
    let file = limits.file_bytes;
    // SAFETY: After fork this closure calls only libc syscalls and obtains an
    // errno-based io::Error on failure; no allocation, locks or Rust formatting.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_CPU, cpu),
                (libc::RLIMIT_FSIZE, file),
                (libc::RLIMIT_NOFILE, 128),
                (libc::RLIMIT_CORE, 0),
            ] {
                let limit = libc::rlimit {
                    rlim_cur: value as libc::rlim_t,
                    rlim_max: value as libc::rlim_t,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            #[cfg(target_os = "linux")]
            sandbox.enforce()?;
            Ok(())
        });
    }
    let started = Instant::now();
    let mut peak = 0;
    let mut peak_accounted = 0;
    let mut peak_anonymous = 0;
    let mut reason = None;
    let mut monitor_error = None;
    let mut monitor_exit_race = None;
    let child = command.spawn();
    let status = match child {
        Err(e) => {
            reason = Some("spawn-failed");
            monitor_error = Some(e.to_string());
            None
        }
        Ok(child) => {
            let mut running = Running { child, armed: true };
            let status = 'monitor: loop {
                if let Err(e) = checkpoint() {
                    reason = Some("cancelled");
                    monitor_error = Some(e.to_string());
                    break None;
                }
                match running.child.try_wait() {
                    Ok(Some(status)) => {
                        running.armed = false;
                        break Some(status);
                    }
                    Err(e) => {
                        reason = Some("wait-failed");
                        monitor_error = Some(e.to_string());
                        break None;
                    }
                    Ok(None) => {}
                }
                if started.elapsed() >= Duration::from_millis(limits.wall_ms) {
                    reason = Some("wall-time-limit");
                    break None;
                }
                match resident(running.child.id()) {
                    Ok(sample) => {
                        let accounted = sample.rss.saturating_add(sample.anonymous_files);
                        peak = peak.max(sample.rss);
                        peak_anonymous = peak_anonymous.max(sample.anonymous_files);
                        peak_accounted = peak_accounted.max(accounted);
                        if accounted > limits.resident_mib * 1024 * 1024 {
                            reason = Some("resident-memory-limit");
                            break None;
                        }
                    }
                    Err(e) => {
                        // proc_pidinfo may lose an exit race. A completed child is
                        // reaped; an unmonitorable live child is always terminated.
                        // Task teardown can precede wait-status availability.
                        // Allow at most 10 ms for this exit race, never resume a
                        // live unmonitorable process or omit the fail-closed stop.
                        for attempt in 0..=5 {
                            if let Err(error) = checkpoint() {
                                reason = Some("cancelled");
                                monitor_error = Some(error.to_string());
                                break 'monitor None;
                            }
                            if let Ok(Some(status)) = running.child.try_wait() {
                                running.armed = false;
                                break 'monitor Some(status);
                            }
                            if started.elapsed() >= Duration::from_millis(limits.wall_ms) {
                                reason = Some("wall-time-limit");
                                break 'monitor None;
                            }
                            if attempt < 5 {
                                std::thread::sleep(Duration::from_millis(2));
                            }
                        }
                        reason = Some("memory-monitor-unavailable");
                        monitor_error = Some(e.to_string());
                        break None;
                    }
                }
                if let Err(e) = output_size(root, limits) {
                    reason = Some("output-byte-limit");
                    monitor_error = Some(e.to_string());
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            let stop_confirmed = status.is_none() && running.kill_group();
            // Reap before the guard can be dropped; do not leave workers alive
            // when callbacks, IO or validation fail.
            let status = match status {
                Some(s) => s,
                None => running.child.wait().map_err(native::io)?,
            };
            running.armed = false;
            monitor_exit_race =
                settle_monitor_exit(&mut reason, &mut monitor_error, status, stop_confirmed);
            Some(status)
        }
    };
    let signal = status.and_then(|s| s.signal());
    let exit = status
        .map(|s| s.code().unwrap_or_else(|| -s.signal().unwrap_or(1)))
        .unwrap_or(1);
    if reason.is_none() && signal == Some(libc::SIGXFSZ) {
        reason = Some("file-size-limit");
    }
    if reason.is_none() && signal == Some(libc::SIGXCPU) {
        reason = Some("cpu-time-limit");
    }
    Ok(
        json!({"schema":"ddi-ruffle-run/experimental-1","producer":PRODUCER,"confinement":confinement,"exitCode":exit,"signal":signal,"termination":reason,"startedAt":started_at,"elapsedSeconds":started.elapsed().as_secs_f64(),"peakRssKiB":peak.div_ceil(1024),"peakAccountedMemoryKiB":peak_accounted.div_ceil(1024),"peakAnonymousFileKiB":peak_anonymous.div_ceil(1024),"monitorError":monitor_error,"monitorExitRace":monitor_exit_race,"limits":{"wallSeconds":limits.wall_ms as f64/1000.0,"residentMiB":limits.resident_mib,"cpuSeconds":limits.cpu_seconds,"fileBytes":limits.file_bytes,"outputBytes":limits.output_bytes,"openFiles":128,"coreBytes":0,"pollIntervalMs":10}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_monitor_race_never_clears_live_failure_signals_or_other_limits() {
        for (initial, raw, stopped, allowed) in [
            ("memory-monitor-unavailable", 0, true, true),
            ("memory-monitor-unavailable", libc::SIGKILL, true, false),
            ("memory-monitor-unavailable", libc::SIGXCPU, true, false),
            ("memory-monitor-unavailable", 1 << 8, true, false),
            ("memory-monitor-unavailable", 0, false, false),
            ("wall-time-limit", 0, true, false),
            ("cancelled", 0, true, false),
            ("resident-memory-limit", 0, true, false),
            ("output-byte-limit", 0, true, false),
        ] {
            let mut reason = Some(initial);
            let mut error = Some("missing task during teardown".to_string());
            let race = settle_monitor_exit(
                &mut reason,
                &mut error,
                std::process::ExitStatus::from_raw(raw),
                stopped,
            );
            assert_eq!(race.is_some(), allowed, "{initial}/{raw}/{stopped}");
            if allowed {
                assert!(reason.is_none() && error.is_none());
                assert_eq!(race.unwrap()["error"], "missing task during teardown");
            } else {
                assert_eq!(reason, Some(initial));
                assert!(error.is_some());
            }
        }
    }
}
