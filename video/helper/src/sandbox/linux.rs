//! Linux: a Landlock ruleset that grants no file and no network access
//! (best effort: older kernels skip what they lack), then a seccomp-bpf
//! allowlist of the system calls a decoder with threads needs; anything
//! else fails with `EPERM` (so the self-test can observe the denial and a
//! library probing for a feature gets a plain error). `clone` is allowed
//! only for threads and `clone3` answers `ENOSYS`, which makes glibc fall
//! back to `clone`; `prctl` only names threads (and sets no-new-privs
//! again, which seccompiler does before each filter).

use landlock::{ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr};
use seccompiler::{
    SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule,
    TargetArch,
};
use std::collections::BTreeMap;

fn arch() -> TargetArch {
    if cfg!(target_arch = "aarch64") {
        TargetArch::aarch64
    } else {
        TargetArch::x86_64
    }
}

fn landlock() -> Result<(), String> {
    let abi = ABI::V5;
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| format!("landlock: {e}"))?
        .handle_access(AccessNet::from_all(abi))
        .map_err(|e| format!("landlock: {e}"))?
        .create()
        .map_err(|e| format!("landlock: {e}"))?
        .restrict_self()
        .map_err(|e| format!("landlock: {e}"))?;
    // Not having Landlock is not fatal: seccomp below denies the same calls.
    let _ = status;
    Ok(())
}

fn rule(conditions: Vec<SeccompCondition>) -> Result<SeccompRule, String> {
    SeccompRule::new(conditions).map_err(|e| format!("seccomp rule: {e}"))
}

fn arg_eq(index: u8, value: u64) -> Result<SeccompCondition, String> {
    SeccompCondition::new(index, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, value)
        .map_err(|e| format!("seccomp condition: {e}"))
}

fn seccomp() -> Result<(), String> {
    // SAFETY: plain prctl; required before an unprivileged seccomp filter.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err("prctl(PR_SET_NO_NEW_PRIVS) failed".into());
    }
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    let allowed: &[i64] = &[
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_close,
        // No stat of any kind: `newfstatat` and `statx` take paths (and
        // Rust's `metadata` falls back from one to the other); nothing
        // here stats a descriptor after the arena is mapped.
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_mprotect,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_futex,
        libc::SYS_exit,
        libc::SYS_exit_group,
        libc::SYS_rt_sigreturn,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_sigaltstack,
        libc::SYS_getpid,
        libc::SYS_gettid,
        libc::SYS_tgkill,
        libc::SYS_sched_yield,
        libc::SYS_sched_getaffinity,
        libc::SYS_set_robust_list,
        libc::SYS_rseq,
        libc::SYS_getrandom,
        libc::SYS_clock_gettime,
        libc::SYS_clock_nanosleep,
        libc::SYS_nanosleep,
        libc::SYS_gettimeofday,
        libc::SYS_membarrier,
        libc::SYS_getrlimit,
        libc::SYS_prlimit64,
        libc::SYS_restart_syscall,
        libc::SYS_set_tid_address,
        // The second filter below; a filter can only ever tighten.
        libc::SYS_seccomp,
    ];
    for nr in allowed {
        rules.insert(*nr, Vec::new());
    }
    rules.insert(
        libc::SYS_clone,
        vec![rule(vec![
            SeccompCondition::new(
                0,
                SeccompCmpArgLen::Dword,
                SeccompCmpOp::MaskedEq(libc::CLONE_THREAD as u64),
                libc::CLONE_THREAD as u64,
            )
            .map_err(|e| format!("seccomp condition: {e}"))?,
        ])?],
    );
    // Naming threads, and the second filter's own `PR_SET_NO_NEW_PRIVS`
    // (seccompiler sets it before each filter; it only ever restricts).
    rules.insert(
        libc::SYS_prctl,
        vec![
            rule(vec![arg_eq(0, libc::PR_SET_NAME as u64)?])?,
            rule(vec![arg_eq(0, libc::PR_GET_NAME as u64)?])?,
            rule(vec![arg_eq(0, libc::PR_SET_NO_NEW_PRIVS as u64)?])?,
        ],
    );
    let allowlist = SeccompFilter::new(
        rules,
        SeccompAction::Errno(libc::EPERM as u32),
        SeccompAction::Allow,
        arch(),
    )
    .map_err(|e| format!("seccomp filter: {e}"))?;
    let program: seccompiler::BpfProgram = allowlist
        .try_into()
        .map_err(|e| format!("seccomp compile: {e}"))?;
    seccompiler::apply_filter(&program).map_err(|e| format!("seccomp apply: {e}"))?;
    // Installed last, so it is evaluated first: `clone3` gets ENOSYS, and
    // glibc's thread creation falls back to the filtered `clone`.
    let clone3 = SeccompFilter::new(
        [(libc::SYS_clone3, Vec::new())].into_iter().collect(),
        SeccompAction::Allow,
        SeccompAction::Errno(libc::ENOSYS as u32),
        arch(),
    )
    .map_err(|e| format!("seccomp filter: {e}"))?;
    let program: seccompiler::BpfProgram = clone3
        .try_into()
        .map_err(|e| format!("seccomp compile: {e}"))?;
    seccompiler::apply_filter(&program).map_err(|e| format!("seccomp apply: {e}"))?;
    Ok(())
}

pub fn enter() -> Result<&'static str, String> {
    // A hostile movie cannot take the machine's memory: FFmpeg sees
    // allocation failures past this.
    let limit = libc::rlimit {
        rlim_cur: 2 << 30,
        rlim_max: 2 << 30,
    };
    // SAFETY: plain setrlimit on this process.
    unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) };
    landlock()?;
    seccomp()?;
    Ok("seccomp+landlock")
}
