//! Fixed Linux x86-64 policy. This source is also the bundle/capture policy receipt.
//! Requires Landlock ABI >= 6; never drops unavailable restrictions. The child
//! applies this policy before exec, while single-threaded. No helper executable.
use super::*;
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    os::unix::fs::OpenOptionsExt,
};

const READ: u64 = (1 << 2) | (1 << 3);
const EXECUTE: u64 = 1;
const WRITE: u64 = (1 << 1) | (1 << 5) | (1 << 8) | (1 << 14);
// All filesystem rights through ABI 5, including REFER, TRUNCATE and IOCTL_DEV.
const HANDLED_FS: u64 = (1 << 16) - 1;
#[repr(C)]
struct Ruleset {
    fs: u64,
    net: u64,
    scoped: u64,
}
#[repr(C, packed)]
struct PathRule {
    access: u64,
    fd: i32,
}

pub(super) fn available() -> Result<i32> {
    // SAFETY: Version query has no input/output memory.
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0,
            1,
        )
    };
    if abi < 6 {
        return Err(Error::new(
            "unsupported-native-confinement",
            format!(
                "Linux requires Landlock ABI >= 6 (query returned {abi}: {})",
                io::Error::last_os_error()
            ),
        ));
    }
    Ok(abi as i32)
}

pub(super) struct Sandbox {
    rules: OwnedFd,
    filter: Vec<libc::sock_filter>,
    parent: libc::pid_t,
    pub(super) abi: i32,
}
impl Sandbox {
    pub(super) fn prepare(root: &Path) -> Result<Self> {
        let abi = available()?;
        // Deny TCP bind/connect as defense in depth. Seccomp denies ALL sockets,
        // including UDP, UNIX, netlink and io_uring. Scope external signals/IPC.
        let attr = Ruleset {
            fs: HANDLED_FS,
            net: 3,
            scoped: 3,
        };
        // SAFETY: Kernel copies the exact ABI struct; no retained pointers.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr,
                std::mem::size_of::<Ruleset>(),
                0,
            )
        };
        if fd < 0 {
            return Err(native::io(io::Error::last_os_error()));
        }
        // SAFETY: Successful syscall returned a new owned descriptor.
        let rules = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        let sandbox = Self {
            rules,
            abi,
            filter: filter(),
            parent: std::process::id() as libc::pid_t,
        };
        sandbox.path(&root.join("inputs"), READ, false)?;
        sandbox.path(&root.join("capture"), READ | WRITE, false)?;
        sandbox.path(
            &root.join("runtime/ddi-ruffle-probe"),
            (1 << 2) | EXECUTE,
            false,
        )?;
        // Distribution-managed dynamic libraries and Vulkan manifests only. No
        // /usr/bin, user directories, procfs, GPU nodes or general /etc access.
        for p in [
            "/usr/lib",
            "/usr/lib64",
            "/lib",
            "/lib64",
            "/usr/share/vulkan",
            "/etc/vulkan",
            "/sys/devices/system/cpu",
        ] {
            sandbox.path(Path::new(p), READ, true)?;
        }
        for p in ["/etc/ld.so.cache", "/dev/null", "/dev/urandom"] {
            sandbox.path(Path::new(p), 1 << 2, true)?;
        }
        sandbox.path(
            Path::new("/lib64/ld-linux-x86-64.so.2"),
            (1 << 2) | EXECUTE,
            false,
        )?;
        Ok(sandbox)
    }
    fn path(&self, path: &Path, access: u64, optional: bool) -> Result<()> {
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if optional && e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(native::io(e)),
        };
        let rule = PathRule {
            access,
            fd: file.as_raw_fd(),
        };
        // SAFETY: Valid ruleset/path descriptors and exact packed ABI struct.
        if unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                self.rules.as_raw_fd(),
                1,
                &rule,
                0,
            )
        } != 0
        {
            return Err(native::io(io::Error::last_os_error()));
        }
        Ok(())
    }
    /// Called only in Command::pre_exec. All allocation happened in the parent.
    pub(super) fn enforce(&self) -> io::Result<()> {
        let program = libc::sock_fprog {
            len: self.filter.len() as u16,
            filter: self.filter.as_ptr().cast_mut(),
        };
        // SAFETY: All calls below are raw syscalls over prepared buffers. Never
        // allocate, format, lock or unwind between fork and exec.
        unsafe {
            if libc::getppid() != self.parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0
            {
                return Err(io::Error::last_os_error());
            }
            // Close the race with parent death before installing PDEATHSIG.
            if libc::getppid() != self.parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            // CLOEXEC preserves Rust's exec-error pipe until exec while closing
            // any descriptors that the embedding host forgot to mark itself.
            if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) != 0
                || libc::syscall(libc::SYS_landlock_restrict_self, self.rules.as_raw_fd(), 0) != 0
                || libc::prctl(libc::PR_SET_SECCOMP, 2, &program) != 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}
fn stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}
fn eq(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: 0x15,
        jt,
        jf,
        k,
    }
}
fn filter() -> Vec<libc::sock_filter> {
    const ALLOW: u32 = 0x7fff0000;
    const DENY: u32 = 0x00050000 | libc::EPERM as u32;
    const ENOSYS: u32 = 0x00050000 | libc::ENOSYS as u32;
    // Audit architecture check also rejects compat ABIs; x32 syscall numbers
    // never appear in the allowlist. An unknown architecture kills the process.
    let mut b = vec![
        stmt(0x20, 4),
        eq(0xc000003e, 1, 0),
        stmt(0x06, 0x80000000),
        stmt(0x20, 0),
    ];
    // glibc probes clone3 first. Its indirect argument structure cannot be
    // validated by classic BPF: ENOSYS forces its checked clone fallback.
    b.extend([eq(libc::SYS_clone3 as u32, 0, 1), stmt(0x06, ENOSYS)]);
    // Only pthread-style clone: no process creation, namespaces or exit signal.
    let required = libc::CLONE_VM | libc::CLONE_SIGHAND | libc::CLONE_THREAD;
    let allowed = required
        | libc::CLONE_FS
        | libc::CLONE_FILES
        | libc::CLONE_SYSVSEM
        | libc::CLONE_SETTLS
        | libc::CLONE_PARENT_SETTID
        | libc::CLONE_CHILD_CLEARTID
        | libc::CLONE_CHILD_SETTID;
    b.extend([
        eq(libc::SYS_clone as u32, 0, 12),
        stmt(0x20, 20),
        eq(0, 1, 0),
        stmt(0x06, DENY),
        stmt(0x20, 16),
        stmt(0x54, !(allowed as u32)),
        eq(0, 1, 0),
        stmt(0x06, DENY),
        stmt(0x20, 16),
        stmt(0x54, required as u32),
        eq(required as u32, 1, 0),
        stmt(0x06, DENY),
        stmt(0x06, ALLOW),
    ]);
    // No fork/vfork, socket APIs, ptrace/process_vm, mount, namespace, kernel
    // control, io_uring, BPF, perf, execveat, keyring, sysv IPC or device ioctl.
    // Landlock constrains paths for open/exec/unlink and scopes tgkill signals.
    for n in [
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_pread64,
        libc::SYS_pwrite64,
        libc::SYS_close,
        libc::SYS_lseek,
        libc::SYS_open,
        libc::SYS_openat,
        libc::SYS_access,
        libc::SYS_faccessat,
        libc::SYS_faccessat2,
        libc::SYS_stat,
        libc::SYS_lstat,
        libc::SYS_fstat,
        libc::SYS_newfstatat,
        libc::SYS_statx,
        libc::SYS_statfs,
        libc::SYS_fstatfs,
        libc::SYS_readlink,
        libc::SYS_readlinkat,
        libc::SYS_getdents64,
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_mprotect,
        libc::SYS_brk,
        libc::SYS_madvise,
        libc::SYS_msync,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_rt_sigsuspend,
        libc::SYS_tgkill,
        libc::SYS_futex,
        libc::SYS_futex_waitv,
        libc::SYS_set_tid_address,
        libc::SYS_set_robust_list,
        libc::SYS_rseq,
        libc::SYS_arch_prctl,
        libc::SYS_sched_yield,
        libc::SYS_sched_getaffinity,
        libc::SYS_sched_getparam,
        libc::SYS_sched_getscheduler,
        libc::SYS_getcpu,
        libc::SYS_clock_gettime,
        libc::SYS_clock_getres,
        libc::SYS_clock_nanosleep,
        libc::SYS_gettimeofday,
        libc::SYS_nanosleep,
        libc::SYS_time,
        libc::SYS_getpid,
        libc::SYS_gettid,
        libc::SYS_getppid,
        libc::SYS_getuid,
        libc::SYS_geteuid,
        libc::SYS_getgid,
        libc::SYS_getegid,
        libc::SYS_getrandom,
        libc::SYS_uname,
        libc::SYS_sysinfo,
        libc::SYS_getrlimit,
        libc::SYS_getrusage,
        libc::SYS_dup,
        libc::SYS_dup2,
        libc::SYS_dup3,
        libc::SYS_pipe,
        libc::SYS_pipe2,
        libc::SYS_poll,
        libc::SYS_ppoll,
        libc::SYS_select,
        libc::SYS_pselect6,
        libc::SYS_epoll_create1,
        libc::SYS_epoll_ctl,
        libc::SYS_epoll_wait,
        libc::SYS_epoll_pwait,
        libc::SYS_eventfd2,
        libc::SYS_ftruncate,
        libc::SYS_truncate,
        libc::SYS_unlink,
        libc::SYS_unlinkat,
        libc::SYS_fsync,
        libc::SYS_fdatasync,
        libc::SYS_getcwd,
        libc::SYS_chdir,
        libc::SYS_execve,
        libc::SYS_exit,
        libc::SYS_exit_group,
    ] {
        b.extend([eq(n as u32, 0, 1), stmt(0x06, ALLOW)]);
    }
    // Lavapipe allocates its buffers in anonymous shared-memory files. Permit
    // only CLOEXEC + sealing, with no huge pages or explicit executable flag.
    // Landlock denies executing these unlinked files; execveat is also denied.
    b.extend([
        eq(libc::SYS_memfd_create as u32, 0, 6),
        stmt(0x20, 28),
        eq(0, 0, 3),
        stmt(0x20, 24),
        eq(libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING, 0, 1),
        stmt(0x06, ALLOW),
        stmt(0x06, DENY),
    ]);
    // Only descriptor/flags operations, never F_SETOWN/F_SETSIG or leases.
    b.extend([
        eq(libc::SYS_fcntl as u32, 0, 17),
        stmt(0x20, 28),
        eq(0, 1, 0),
        stmt(0x06, DENY),
        stmt(0x20, 24),
    ]);
    for command in [
        libc::F_DUPFD,
        libc::F_GETFD,
        libc::F_SETFD,
        libc::F_GETFL,
        libc::F_SETFL,
        libc::F_DUPFD_CLOEXEC,
    ] {
        b.extend([eq(command as u32, 0, 1), stmt(0x06, ALLOW)]);
    }
    b.push(stmt(0x06, DENY));
    // prlimit64 may only read this process's limits (pid=0, new_limit=NULL).
    b.extend([
        eq(libc::SYS_prlimit64 as u32, 0, 9),
        stmt(0x20, 16),
        eq(0, 0, 7),
        stmt(0x20, 20),
        eq(0, 0, 5),
        stmt(0x20, 32),
        eq(0, 0, 3),
        stmt(0x20, 36),
        eq(0, 0, 1),
        stmt(0x06, ALLOW),
    ]);
    b.push(stmt(0x06, DENY));
    b
}
