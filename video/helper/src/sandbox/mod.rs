//! Confining this process before it touches a movie: no files, no network,
//! no other processes; only the inherited pipes, the mapped arena, memory
//! and threads. Each OS has its own mechanism; `self_test` then tries to
//! break out and refuses to serve if anything works. A failure here is an
//! exit with `exit::SANDBOX`, which the game shows as a decoder that is not
//! available, never as a decoder that runs unconfined.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::io;

/// Enters the sandbox; the name of what holds this process.
pub fn enter() -> Result<&'static str, String> {
    #[cfg(target_os = "macos")]
    {
        macos::enter()
    }
    #[cfg(target_os = "linux")]
    {
        linux::enter()
    }
    #[cfg(windows)]
    {
        windows::enter()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Err("no sandbox is implemented for this system".to_string())
    }
}

/// What must still work inside: a thread, the clock, memory.
fn still_works() -> Result<(), String> {
    let t = std::thread::Builder::new()
        .name("ddi-video-probe".into())
        .spawn(|| 40 + 2)
        .map_err(|e| format!("a thread cannot be started inside the sandbox: {e}"))?;
    if t.join().map_err(|_| "the probe thread panicked")? != 42 {
        return Err("the probe thread miscomputed".into());
    }
    let _ = std::time::Instant::now();
    let v = vec![7u8; 4 << 20];
    if v[v.len() - 1] != 7 {
        return Err("memory misbehaves".into());
    }
    Ok(())
}

fn leak<T>(what: &str, r: io::Result<T>) -> Result<(), String> {
    match r {
        Ok(_) => Err(format!("the sandbox is open: {what} succeeded")),
        Err(_) => Ok(()),
    }
}

/// Tries what the sandbox must deny: reading and writing files, listing a
/// directory, reaching the network, starting a process. Any success is a
/// hole, and this process will not decode.
pub fn self_test() -> Result<(), String> {
    #[cfg(unix)]
    let (dir, file, program) = ("/", "/dev/null", "/usr/bin/true");
    #[cfg(windows)]
    let (dir, file, program) = (
        "C:\\",
        "C:\\Windows\\win.ini",
        "C:\\Windows\\System32\\cmd.exe",
    );
    leak("opening a file", std::fs::File::open(file))?;
    leak("opening a directory", std::fs::File::open(dir))?;
    leak("listing a directory", std::fs::read_dir(dir))?;
    leak(
        "creating a file",
        std::fs::File::create(std::env::temp_dir().join("ddi-video-sandbox-probe")),
    )?;
    leak("reading a file's metadata", std::fs::metadata(file))?;
    // Localhost answers at once when reachable (refused or connected), so
    // a failure to even try is what a closed sandbox looks like.
    match std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 9)),
        std::time::Duration::from_millis(500),
    ) {
        Ok(_) => return Err("the sandbox is open: a TCP connection succeeded".into()),
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            return Err("the sandbox is open: the network is reachable".into());
        }
        Err(_) => {}
    }
    leak(
        "binding a UDP socket",
        std::net::UdpSocket::bind("127.0.0.1:0"),
    )?;
    leak(
        "starting a process",
        std::process::Command::new(program)
            .arg("/c")
            .arg("exit")
            .status(),
    )?;
    still_works()
}
