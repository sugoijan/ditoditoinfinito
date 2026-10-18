use std::{
    fs,
    io::{self, Write},
    process::{Command, Stdio},
    time::Duration,
};
fn main() {
    let config = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let data = fs::read_to_string(&config).unwrap();
    let input = config.parent().unwrap();
    let root = input.parent().unwrap();
    fs::write("pid.txt", std::process::id().to_string()).unwrap();
    if data.contains("/sleep") {
        std::thread::sleep(Duration::from_secs(30));
    }
    #[cfg(target_os = "linux")]
    if data.contains("/memfd-growth") {
        use std::os::fd::FromRawFd;
        unsafe extern "C" {
            fn syscall(n: i64, ...) -> i64;
        }
        let mut files = Vec::new();
        for _ in 0..8 {
            let fd = unsafe { syscall(319, c"unmapped-budget-probe".as_ptr(), 3u32) };
            assert!(fd >= 0);
            let mut file = unsafe { fs::File::from_raw_fd(fd as i32) };
            for _ in 0..128 {
                file.write_all(&[0x5a; 65536]).unwrap();
            }
            files.push(file);
        }
        std::hint::black_box(&files);
        std::thread::sleep(Duration::from_secs(30));
    }
    if data.contains("/memory") {
        let mut v = vec![0u8; 256 * 1024 * 1024];
        for (i, x) in v.iter_mut().enumerate() {
            *x = (i % 251) as u8;
        }
        std::hint::black_box(&v);
        std::thread::sleep(Duration::from_secs(30));
    }
    if data.contains("/cpu") {
        let mut x = 1u64;
        loop {
            x = x.wrapping_mul(17).wrapping_add(3);
            std::hint::black_box(x);
        }
    }
    if data.contains("/crash") {
        std::process::abort();
    }
    if data.contains("/file") {
        let mut f = fs::File::create("large.bin").unwrap();
        loop {
            if let Err(error) = f.write_all(&[0; 1024]) {
                eprintln!("file-write-denied:{}", error.raw_os_error().unwrap_or(0));
                std::process::exit(70);
            }
        }
    }
    if data.contains("/output-symlink") {
        std::os::unix::fs::symlink(root.parent().unwrap().join("outside-secret"), "escape")
            .unwrap();
        std::thread::sleep(Duration::from_secs(30));
    }
    if data.contains("/aggregate") {
        for i in 0..8 {
            fs::write(format!("extra-{i}"), vec![0; 1024 * 1024]).unwrap();
        }
        std::thread::sleep(Duration::from_secs(30));
    }
    #[cfg(target_os = "linux")]
    if data.contains("/linux-probe") {
        let at = data.find("linux-probe?fd=").unwrap() + "linux-probe?fd=".len();
        let fd = data[at..].split('"').next().unwrap().parse().unwrap();
        linux_probe(input, root, fd);
        return;
    }
    if data.contains("/probe") {
        let outside = root.parent().unwrap().join("outside-secret");
        let read = fs::read(&outside).is_ok();
        let input_read = fs::read(input.join("fixture.swf")).is_ok();
        let input_write = fs::write(input.join("fixture.swf"), b"changed").is_ok();
        let outside_write = fs::write(&outside, b"changed").is_ok();
        let link_read = fs::read(input.join("escape")).is_ok();
        let at = data.find("probe?port=").unwrap() + "probe?port=".len();
        let port = data[at..].split('"').next().unwrap();
        let network = std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(100),
        )
        .is_ok();
        let spawn = Command::new("/usr/bin/true")
            .stdout(Stdio::null())
            .status()
            .is_ok();
        let output = fs::write("allowed.txt", b"allowed").is_ok();
        println!(
            "{{\"outsideRead\":{read},\"inputRead\":{input_read},\"inputWrite\":{input_write},\"outsideWrite\":{outside_write},\"linkRead\":{link_read},\"network\":{network},\"spawn\":{spawn},\"outputWrite\":{output}}}"
        );
    } else {
        io::stdout().write_all(b"malformed snapshot").unwrap();
    }
}

#[cfg(target_os = "linux")]
fn linux_probe(input: &std::path::Path, root: &std::path::Path, inherited: i32) {
    unsafe extern "C" {
        fn syscall(n: i64, ...) -> i64;
    }
    let denied = |name, n, args: [i64; 6]| {
        // Original test helper only: requests forbidden operations and checks
        // EPERM. No acquired script or system state mutation is involved.
        let result = unsafe { syscall(n, args[0], args[1], args[2], args[3], args[4], args[5]) };
        println!(
            "{name}={result}:{}",
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
        );
    };
    let parent = unsafe { syscall(110) };
    for (name, n, args) in [
        ("fork", 57, [0; 6]),
        ("vfork", 58, [0; 6]),
        ("clone-process", 56, [17, 0, 0, 0, 0, 0]),
        (
            "clone-namespace",
            56,
            [0x10000 | 0x100 | 0x800 | 0x10000000, 0, 0, 0, 0, 0],
        ),
        ("clone3", 435, [0, 0, 0, 0, 0, 0]),
        ("socket-tcp", 41, [2, 1, 0, 0, 0, 0]),
        ("socket-udp", 41, [2, 2, 0, 0, 0, 0]),
        ("socket-ipv6", 41, [10, 1, 0, 0, 0, 0]),
        ("socket-unix", 41, [1, 1, 0, 0, 0, 0]),
        ("socket-netlink", 41, [16, 3, 0, 0, 0, 0]),
        ("socketpair", 53, [1, 1, 0, 0, 0, 0]),
        ("ptrace", 101, [16, parent, 0, 0, 0, 0]),
        ("process-vm", 310, [parent, 0, 0, 0, 0, 0]),
        ("io-uring", 425, [1, 0, 0, 0, 0, 0]),
        ("memfd-huge", 319, [0, 4, 0, 0, 0, 0]),
        ("memfd-exec", 319, [0, 16, 0, 0, 0, 0]),
        ("execveat", 322, [-1, 0, 0, 0, 0, 0]),
        ("unshare", 272, [0x10000000, 0, 0, 0, 0, 0]),
        ("setns", 308, [-1, 0, 0, 0, 0, 0]),
        ("mount", 165, [0; 6]),
        ("signal-parent", 234, [parent, parent, 28, 0, 0, 0]),
        ("prlimit-parent", 302, [parent, 7, 0, 0, 0, 0]),
        ("prlimit-set-low", 302, [0, 7, 1, 0, 0, 0]),
        ("prlimit-set-high", 302, [0, 7, 1i64 << 32, 0, 0, 0]),
        ("compat-x32", 0x40000000, [0; 6]),
    ] {
        denied(name, n, args);
    }
    println!("thread={}", std::thread::spawn(|| 42).join().unwrap() == 42);
    println!(
        "truncate={}",
        fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(root.parent().unwrap().join("outside-secret"))
            .is_ok()
    );
    println!(
        "symlink={}",
        std::os::unix::fs::symlink(input.join("fixture.swf"), "escape").is_ok()
    );
    println!(
        "hardlink={}",
        fs::hard_link(input.join("fixture.swf"), "link").is_ok()
    );
    println!("mkdir={}", fs::create_dir("directory").is_ok());
    println!(
        "runtime-write={}",
        fs::write(root.join("runtime/build-identity.json"), b"bad").is_ok()
    );
    println!("proc-read={}", fs::read("/proc/self/environ").is_ok());
    let mut byte = 0u8;
    println!("inherited-fd={}", unsafe {
        syscall(0, inherited, &mut byte as *mut u8, 1usize)
    });
}
