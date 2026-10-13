//! `ddi-lua` end to end: the `cmd` rewrite as StepMania runs it, the
//! sandbox's globals, budgets, the deadline and the limits.

use ddi_lua::piccolo::{Callback, CallbackReturn, Value};
use ddi_lua::{Budget, Failure, LimitError, Outcome, Settings, Task, Vm};

const UNLIMITED: Budget = Budget {
    fuel: u64::MAX,
    deadline: f64::INFINITY,
};

/// A VM whose `print` (as Lua's: `tostring` of each argument) appends to
/// a log, read back by [`log`].
fn vm() -> Vm {
    let mut vm = Vm::new(Settings::default());
    let print = vm
        .load(
            "=print",
            b"local tostring, select, concat, log = tostring, select, table.concat, LOG_LINE
            function print(...)
                local t = {}
                for i = 1, select('#', ...) do t[i] = tostring((select(i, ...))) end
                log(concat(t, '\\t'))
            end",
        )
        .unwrap_or_else(|e| panic!("{e}"));
    vm.lua().enter(|ctx| {
        ctx.set_global("LOG", ctx.intern(b"")).unwrap();
        ctx.set_global(
            "LOG_LINE",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let line: ddi_lua::piccolo::String = stack.consume(ctx)?;
                let mut all = match ctx.get_global_value("LOG") {
                    Value::String(s) => s.as_bytes().to_vec(),
                    _ => Vec::new(),
                };
                all.extend_from_slice(line.as_bytes());
                all.push(b'\n');
                // Refused, the limit stops the script after this step.
                let _ = ctx.set_global("LOG", ctx.intern(&all));
                Ok(CallbackReturn::Return)
            }),
        )
        .unwrap();
    });
    let mut task = vm.start(&print, |_| Vec::new());
    assert_eq!(vm.run(&mut task, UNLIMITED, &mut || 0.0), Outcome::Done);
    vm
}

fn log(vm: &mut Vm) -> String {
    vm.lua().enter(|ctx| match ctx.get_global_value("LOG") {
        Value::String(s) => s.display_lossy().to_string(),
        _ => String::new(),
    })
}

fn start(vm: &mut Vm, name: &str, source: &str) -> Result<Task, Failure> {
    let f = vm.load(name, source.as_bytes())?;
    Ok(vm.start(&f, |_| Vec::new()))
}

/// Runs `source` to the end and returns what it printed, or the failure.
fn run(source: &str) -> Result<String, Failure> {
    let mut vm = vm();
    let mut task = start(&mut vm, "@test.lua", source)?;
    match vm.run(&mut task, UNLIMITED, &mut || 0.0) {
        Outcome::Done => Ok(log(&mut vm)),
        Outcome::Failed(f) => Err(f),
        Outcome::Paused => Err(Failure::Error("paused".into())),
    }
}

#[test]
fn cmd_calls_methods_on_self() {
    let out = run(r#"
        local log = {}
        local proxy = setmetatable({}, { __index = function(_, k)
            return function(self, ...)
                local args = {}
                for i = 1, select('#', ...) do args[i] = tostring((select(i, ...))) end
                log[#log + 1] = k .. "(" .. table.concat(args, ",") .. ")"
                return self
            end
        end })
        local function color(s) return "c" .. s end
        local f = cmd(diffusealpha,0;zoom,1.1;effectcolor1,color("1,1,1,0");
            playcommand,"Set";;finishtweening;x,1,2,3;y,...)
        f(proxy, 7, 8)
        print(table.concat(log, ";"))
        local g = cmd(queuecommand,cmd(zoom,2);sleep,(function() local a = 1; return a end)())
        log = {}
        g(proxy)
        print(table.concat(log, ";"))
    "#);
    let out = out.unwrap_or_default();
    assert!(out.starts_with(
        "diffusealpha(0);zoom(1.1);effectcolor1(c1,1,1,0);playcommand(Set);finishtweening();x(1,2,3);y(7,8)\n"
    ));
    assert!(
        out.contains("queuecommand(") && out.ends_with(");sleep(1)\n"),
        "{out}"
    );
}

#[test]
fn errors_keep_their_lines() {
    let src = "local t = cmd(\n  zoom,1;\n  x,\n    2\n)\nerror('here')";
    assert_eq!(run(src), Err(Failure::Error("test.lua:6: here".into())));
    // A syntax error after a cmd(...), on its line.
    let src = "local t = cmd(zoom,1;\n x,2)\nlocal = 3";
    match run(src) {
        Err(Failure::Compile(m)) => assert!(m.starts_with("test.lua:3:"), "{m}"),
        r => panic!("{r:?}"),
    }
    match run("x = 1\nlocal t = cmd(zoom 2)") {
        Err(Failure::Compile(m)) => assert_eq!(m, "test.lua:2: ')' expected near '2'"),
        r => panic!("{r:?}"),
    }
}

#[test]
fn sandbox_globals() {
    let out = run(r#"
        for _, name in ipairs({"io", "os", "debug", "package", "require", "dofile",
                "loadfile", "load", "loadstring", "collectgarbage", "module",
                "setfenv", "getfenv", "newproxy"}) do
            if _G[name] ~= nil then print("present: " .. name) end
        end
        print(_VERSION, _G == _G._G, unpack({1, 2}))
        print(table.getn({1, 2, 3}), math.mod(7, 3), math.pow(2, 10), 1 / 3, 10 / 2)
        for w in string.gfind("a b", "%a") do print(w) end
        print(("Tap Note"):lower(), ("x"):rep(3), string.format("%5.2f", 1 / 3))
        print(math.random(1, 100), math.random(1, 100))
    "#);
    assert_eq!(
        out.as_deref(),
        Ok(
            "Lua 5.1\ttrue\t1\t2\n3\t1\t1024\t0.33333333333333\t5\na\nb\ntap note\txxx\t 0.33\n66\t9\n"
        )
    );
}

#[test]
fn random_is_seeded_by_the_host() {
    let draw = |seed| {
        let mut vm = Vm::new(Settings {
            random_seed: seed,
            ..Settings::default()
        });
        let f = vm.load("=r", b"return math.random(1, 1000000)");
        let Ok(f) = f else { return None };
        let mut task = vm.start(&f, |_| Vec::new());
        (vm.run(&mut task, UNLIMITED, &mut || 0.0) == Outcome::Done).then(|| {
            vm.lua()
                .enter(|ctx| ctx.fetch(&task.results()[0]).display().to_string())
        })
    };
    assert_eq!(draw(1), draw(1));
    assert_ne!(draw(1), draw(2));
}

#[test]
fn fuel_pauses_and_resumes() {
    let mut vm = vm();
    let src = "local n = 0 for i = 1, 100000 do n = n + i end print(n)";
    let mut task = start(&mut vm, "=loop", src).unwrap_or_else(|e| panic!("{e}"));
    let budget = Budget {
        fuel: 10_000,
        deadline: f64::INFINITY,
    };
    let mut runs = 0;
    loop {
        runs += 1;
        match vm.run(&mut task, budget, &mut || 0.0) {
            Outcome::Paused => continue,
            Outcome::Done => break,
            o => panic!("{o:?}"),
        }
    }
    assert!(runs > 10, "{runs} runs");
    assert!(task.fuel_used() > 100_000, "{}", task.fuel_used());
    assert_eq!(log(&mut vm), "5000050000\n");
    // A finished task keeps its outcome.
    assert_eq!(vm.run(&mut task, budget, &mut || 0.0), Outcome::Done);
}

#[test]
fn deadline_stops_endless_loops() {
    for src in [
        "while true do end",
        "while true do pcall(function() while true do end end) end",
        "local co = coroutine.create(function() while true do end end) coroutine.resume(co)",
    ] {
        let mut vm = vm();
        let mut task = start(&mut vm, "=spin", src).unwrap_or_else(|e| panic!("{e}"));
        let mut clock = 0.0;
        let outcome = vm.run(
            &mut task,
            Budget {
                fuel: u64::MAX,
                deadline: 100.0,
            },
            &mut || {
                clock += 1.0;
                clock
            },
        );
        assert_eq!(outcome, Outcome::Failed(Failure::Deadline), "{src}");
        assert_eq!(task.steps(), 100, "{src}");
    }
}

#[test]
fn limits_are_not_caught() {
    let mut vm = vm();
    let src = "local s = 'x' for i = 1, 40 do s = s .. s end";
    let mut task = start(&mut vm, "=m1", src).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        vm.run(&mut task, UNLIMITED, &mut || 0.0),
        Outcome::Failed(Failure::Limit(LimitError::StringLength))
    );
    let src = "while true do pcall(function() local t = {} \
        while true do t[#t + 1] = ('x'):rep(2^19) .. #t end end) end";
    let mut task = start(&mut vm, "=m9", src).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        vm.run(&mut task, UNLIMITED, &mut || 0.0),
        Outcome::Failed(Failure::Limit(LimitError::Memory))
    );
    assert!(vm.memory() <= 32 << 20, "{}", vm.memory());
    // The VM stays usable.
    let mut task = start(&mut vm, "=after", "print('ok')").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(vm.run(&mut task, UNLIMITED, &mut || 0.0), Outcome::Done);
}

#[test]
fn compile_failures() {
    let mut vm = Vm::new(Settings {
        source_len: 100,
        ..Settings::default()
    });
    match vm.load("@big.lua", &[b' '; 101]) {
        Err(Failure::Compile(m)) => assert_eq!(m, "big.lua: source is longer than 100 bytes"),
        r => panic!("{:?}", r.err()),
    }
    let mut vm = Vm::new(Settings {
        compile_work: 1_000,
        ..Settings::default()
    });
    let src = "local x = 1\n".repeat(1_000);
    match vm.load("@long.lua", src.as_bytes()) {
        Err(Failure::Compile(m)) => assert!(m.contains("too complex"), "{m}"),
        r => panic!("{:?}", r.err()),
    }
    // Nested 100,000 deep: the fork's recursion guard.
    let src = format!("x = {}1{}", "(".repeat(100_000), ")".repeat(100_000));
    let mut vm = Vm::new(Settings {
        source_len: 1 << 20,
        ..Settings::default()
    });
    assert!(matches!(
        vm.load("=deep", src.as_bytes()),
        Err(Failure::Compile(_))
    ));
}

#[test]
fn uncaught_error_values() {
    assert_eq!(
        run("error({})"),
        Err(Failure::Error("(error object is a table value)".into()))
    );
    assert_eq!(run("error(42, 0)"), Err(Failure::Error("42".into())));
    // A `__tostring` that never ends is not run to describe the error.
    assert_eq!(
        run("error(setmetatable({}, {__tostring = function() while true do end end}))"),
        Err(Failure::Error("(error object is a table value)".into()))
    );
    match run("local t = nil; t.x = 1") {
        Err(Failure::Error(m)) => assert!(!m.is_empty()),
        r => panic!("{r:?}"),
    }
}
