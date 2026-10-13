//! Runs on local files only, never committed (third-party skins stay
//! local; the limit cases are the fork's):
//!
//! - `DDI_LUA_CORPUS=<dir>[:<dir>…]` compiles every `.lua` under the
//!   folders, `cmd` rewritten, and reports failures and compile times.
//! - `DDI_LUA_CASES=<cases.txt>` runs the piccolo fork's limit cases
//!   (`cargo test --test limits -- --ignored export_cases` there) through
//!   a [`Vm`] with the default settings, one step per [`Vm::run`], a host
//!   stop after 2 s, and checks each outcome and, in release builds, that
//!   no step took over 10 ms.
//!
//! Run the test binary under a memory watchdog: these are adversarial.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ddi_lua::piccolo::{Callback, CallbackReturn};
use ddi_lua::{Budget, Failure, Outcome, Settings, Vm};

fn lua_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            lua_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
        {
            out.push(path);
        }
    }
}

#[test]
fn local_corpus() {
    let Some(dirs) = std::env::var_os("DDI_LUA_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        lua_files(&dir, &mut files);
    }
    files.sort();
    let mut failed = Vec::new();
    let mut slowest = (Duration::ZERO, PathBuf::new(), 0);
    let mut total = Duration::ZERO;
    for path in &files {
        let Ok(source) = std::fs::read(path) else {
            continue;
        };
        let mut vm = Vm::new(Settings::default());
        let start = Instant::now();
        let result = vm.load(&format!("@{}", path.display()), &source);
        let took = start.elapsed();
        total += took;
        if took > slowest.0 {
            slowest = (took, path.clone(), source.len());
        }
        if let Err(e) = result {
            failed.push(format!("{e}"));
        }
    }
    println!(
        "{} files, {} failed; compile total {:.1} ms, slowest {:.2} ms ({}, {} bytes)",
        files.len(),
        failed.len(),
        total.as_secs_f64() * 1e3,
        slowest.0.as_secs_f64() * 1e3,
        slowest.1.display(),
        slowest.2
    );
    for f in &failed {
        println!("  {f}");
    }
    assert!(
        failed.is_empty(),
        "{} files failed to compile",
        failed.len()
    );
}

/// Cases whose outcome differs from the fork's (which runs them in its
/// default, Lua 5.4 mode) because a [`Vm`] follows Lua 5.1 and has no
/// `load`.
const LUA51_OUTCOMES: &[(&str, &str, &str)] = &[
    (
        "C12b",
        "ok",
        "5.1's table.insert uses raw access and the raw length",
    ),
    ("C12c", "ok", "5.1's table.remove uses the raw length"),
    ("C12d", "ok", "5.1's table.concat uses raw access"),
    ("C15b", "error", "5.1's table.concat: no value at the index"),
    (
        "M13",
        "ok",
        "no load: the error stays inside the coroutines",
    ),
    ("R6a", "error", "no load"),
    ("R6b", "error", "no load"),
    ("R6c", "error", "no load"),
    ("I1e", "ok", "1 % 0 is nan in 5.1"),
    ("I1f", "ok", "1 // 0 is a float in 5.1 mode"),
    ("I1g", "ok", "as I1e, folded"),
    ("I1h", "ok", "as I1f, folded"),
    ("I2b", "ok", "5.1's format truncates %d arguments"),
    ("I2g", "ok", "5.1's table.insert sets past the end"),
    (
        "C6",
        "ok",
        "5.1's pcall catches the stack overflow the recursion ends in",
    ),
    (
        "I2h",
        "error",
        "5.1 counts in floats: maxinteger - 1 is 2^63, too many to unpack",
    ),
    (
        "I3a",
        "host_stop",
        "5.1 counts in floats: 2^63 + 1 is 2^63, the loop never ends",
    ),
    ("I3b", "ok", "5.1's loop with a zero step runs no iteration"),
];

struct Case {
    id: String,
    expected: Vec<String>,
    source: Vec<u8>,
}

/// The fork's `cases.txt`: a line `<id> <outcomes,…> <length>` before each
/// source.
fn read_cases(data: &[u8]) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut pos = 0;
    while let Some(nl) = data
        .get(pos..)
        .and_then(|r| r.iter().position(|&b| b == b'\n'))
    {
        let header = String::from_utf8_lossy(&data[pos..pos + nl]).to_string();
        let mut parts = header.split(' ');
        let (Some(id), Some(expected), Some(len)) = (parts.next(), parts.next(), parts.next())
        else {
            break;
        };
        let Ok(len) = len.parse::<usize>() else { break };
        let start = pos + nl + 1;
        let Some(source) = data.get(start..start + len) else {
            break;
        };
        cases.push(Case {
            id: id.into(),
            expected: expected.split(',').map(String::from).collect(),
            source: source.to_vec(),
        });
        pos = start + len + 1;
    }
    cases
}

#[test]
fn limit_cases() {
    let Some(path) = std::env::var_os("DDI_LUA_CASES") else {
        return;
    };
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{e}"));
    let only = std::env::var("DDI_LUA_ONLY").ok();
    let mut bad = Vec::new();
    println!(
        "| case | expected | outcome | steps | fuel | memory MiB | longest step ms | load ms |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for case in read_cases(&data) {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|id| id == case.id))
        {
            continue;
        }
        let mut vm = Vm::new(Settings::default());
        // The cases print; the sandbox has no `print`.
        vm.lua().enter(|ctx| {
            ctx.set_global(
                "print",
                Callback::from_fn(&ctx, |_, _, _| Ok(CallbackReturn::Return)),
            )
            .unwrap();
        });
        let load_start = Instant::now();
        let loaded = vm.load("=case", &case.source);
        let load = load_start.elapsed();
        let mut steps = 0;
        let mut fuel = 0;
        let mut longest = Duration::ZERO;
        let outcome = match loaded {
            Err(Failure::Limit(_)) => "limit",
            Err(_) => "compile_error",
            Ok(f) => {
                let mut task = vm.start(&f, |_| Vec::new());
                let start = Instant::now();
                let stop = 2.0;
                let step_fuel = vm.settings().step_fuel as u64;
                let o = loop {
                    let t = Instant::now();
                    let o = vm.run(
                        &mut task,
                        Budget {
                            fuel: step_fuel,
                            deadline: stop,
                        },
                        &mut || start.elapsed().as_secs_f64(),
                    );
                    longest = longest.max(t.elapsed());
                    match o {
                        Outcome::Paused => continue,
                        o => break o,
                    }
                };
                steps = task.steps();
                fuel = task.fuel_used();
                match o {
                    Outcome::Done => "ok",
                    Outcome::Failed(Failure::Deadline | Failure::Fuel) => "host_stop",
                    Outcome::Failed(Failure::Limit(_)) => "limit",
                    Outcome::Failed(Failure::Error(_)) => "error",
                    Outcome::Failed(Failure::Compile(_)) | Outcome::Paused => "compile_error",
                }
            }
        };
        let expected: Vec<String> = match LUA51_OUTCOMES.iter().find(|(id, ..)| *id == case.id) {
            Some((_, o, _)) => vec![(*o).to_string()],
            None => case.expected.clone(),
        };
        let memory = vm.memory() as f64 / f64::from(1 << 20);
        println!(
            "| {} | {} | {} | {} | {} | {:.1} | {:.2} | {:.2} |",
            case.id,
            expected.join(","),
            outcome,
            steps,
            fuel,
            memory,
            longest.as_secs_f64() * 1e3,
            load.as_secs_f64() * 1e3
        );
        if !expected.iter().any(|e| e == outcome) {
            bad.push(format!(
                "{}: {outcome}, expected {}",
                case.id,
                expected.join(",")
            ));
        }
        if !cfg!(debug_assertions)
            && (longest > Duration::from_millis(10) || load > Duration::from_millis(50))
        {
            bad.push(format!(
                "{}: step {:.2} ms, load {:.2} ms",
                case.id,
                longest.as_secs_f64() * 1e3,
                load.as_secs_f64() * 1e3
            ));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
