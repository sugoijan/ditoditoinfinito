//! The real executable over anonymous OS pipes: no config/progress/result files.
use ddi_importer_protocol::{Event, Request, VERSION};
use std::{
    io,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn child() -> (Owned, mpsc::Receiver<io::Result<Option<Event>>>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ddi-danoni-importer"))
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            let event = ddi_importer_protocol::read::<Event>(&mut stdout);
            let stop = !matches!(event, Ok(Some(_)));
            if tx.send(event).is_err() || stop {
                break;
            }
        }
    });
    (Owned(child), rx)
}
fn next(rx: &mpsc::Receiver<io::Result<Option<Event>>>) -> Event {
    rx.recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap()
        .unwrap()
}
fn wait(child: &mut Owned) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn executable_handshake_shutdown_and_eof_are_framed() {
    for explicit in [false, true] {
        let (mut child, rx) = child();
        ddi_importer_protocol::write(
            child.0.stdin.as_mut().unwrap(),
            &Request::Hello { version: VERSION },
        )
        .unwrap();
        assert!(matches!(
            next(&rx),
            Event::Hello {
                version: VERSION,
                ..
            }
        ));
        if explicit {
            ddi_importer_protocol::write(child.0.stdin.as_mut().unwrap(), &Request::Shutdown)
                .unwrap();
        } else {
            child.0.stdin.take();
        }
        assert_eq!(next(&rx), Event::Bye);
        wait(&mut child);
        assert!(
            rx.recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()
                .is_none()
        );
    }
}
#[test]
fn executable_rejects_truncated_frame_without_stdout_diagnostics() {
    use std::io::Write;
    let (mut child, rx) = child();
    child.0.stdin.as_mut().unwrap().write_all(&[0, 0]).unwrap();
    child.0.stdin.take();
    assert!(matches!(next(&rx), Event::Fatal {code,..} if code == "invalid-frame"));
    assert_eq!(next(&rx), Event::Bye);
    wait(&mut child);
}
