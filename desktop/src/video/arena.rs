//! The shared memory a helper and the game exchange bulk bytes through
//! (`ddi_video_protocol::Layout`): created here, anonymous (unlinked or
//! sealed before the child sees it, so no other process can open it by
//! name), mapped read-write on both sides. The child gets the descriptor
//! (or, on Windows, an inheritable handle) and nothing else.

use ddi_video_protocol::Layout;
use std::ops::Range;

pub struct SharedArena {
    base: *mut u8,
    len: usize,
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    pub layout: Layout,
}

// Shared with the child by design; the parent reads it from the decoder
// thread only.
unsafe impl Send for SharedArena {}

impl SharedArena {
    pub fn len(&self) -> usize {
        self.len
    }

    /// Bytes of `range`, which must lie inside the arena. The child may be
    /// writing them at the same time: callers copy them out and check
    /// nothing against them but what the protocol already bounded.
    pub fn slice(&self, range: &Range<usize>) -> &[u8] {
        assert!(range.end <= self.len && range.start <= range.end);
        // SAFETY: the mapping lives as long as `self`; the range is checked.
        unsafe { std::slice::from_raw_parts(self.base.add(range.start), range.end - range.start) }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn slice_mut(&self, range: &Range<usize>) -> &mut [u8] {
        assert!(range.end <= self.len && range.start <= range.end);
        // SAFETY: as above; the parent writes the I/O region only while the
        // child waits for the bytes.
        unsafe {
            std::slice::from_raw_parts_mut(self.base.add(range.start), range.end - range.start)
        }
    }

    #[cfg(unix)]
    pub fn create() -> std::io::Result<SharedArena> {
        use std::os::fd::{FromRawFd, OwnedFd};
        let len = Layout::LEN;
        // SAFETY: plain libc calls; every result is checked.
        unsafe {
            #[cfg(target_os = "linux")]
            let fd = {
                let fd = libc::memfd_create(
                    c"ddi-video".as_ptr(),
                    libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
                );
                if fd < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let fd = OwnedFd::from_raw_fd(fd);
                if libc::ftruncate(std::os::fd::AsRawFd::as_raw_fd(&fd), len as libc::off_t) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // The child cannot resize it: a shorter object would fault
                // the parent's reads.
                if libc::fcntl(
                    std::os::fd::AsRawFd::as_raw_fd(&fd),
                    libc::F_ADD_SEALS,
                    libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL,
                ) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                fd
            };
            #[cfg(not(target_os = "linux"))]
            let fd = {
                static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let name = format!(
                    "/ddi-video-{}-{}",
                    std::process::id(),
                    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                );
                let cname = std::ffi::CString::new(name).expect("no NUL");
                let fd = libc::shm_open(
                    cname.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                    0o600,
                );
                if fd < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // Nameless from here on: only the descriptor reaches it.
                libc::shm_unlink(cname.as_ptr());
                let fd = OwnedFd::from_raw_fd(fd);
                // macOS lets a shared memory object be sized exactly once,
                // so the child cannot shrink it either.
                if libc::ftruncate(std::os::fd::AsRawFd::as_raw_fd(&fd), len as libc::off_t) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                fd
            };
            let p = libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                std::os::fd::AsRawFd::as_raw_fd(&fd),
                0,
            );
            if p == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }
            Ok(SharedArena {
                base: p.cast(),
                len,
                fd,
                layout: Layout::new(),
            })
        }
    }

    /// The descriptor the child inherits.
    #[cfg(unix)]
    pub fn raw_fd(&self) -> i32 {
        std::os::fd::AsRawFd::as_raw_fd(&self.fd)
    }

    #[cfg(windows)]
    pub fn create() -> std::io::Result<SharedArena> {
        use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows::Win32::Security::SECURITY_ATTRIBUTES;
        use windows::Win32::System::Memory::{
            CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE,
        };
        let len = Layout::LEN;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: true.into(),
        };
        // SAFETY: plain Win32 calls; results checked. An unnamed mapping:
        // only the inherited handle reaches it.
        unsafe {
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                Some(&attributes),
                PAGE_READWRITE,
                (len as u64 >> 32) as u32,
                len as u32,
                None,
            )
            .map_err(|e| std::io::Error::other(e.to_string()))?;
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, len);
            if view.Value.is_null() {
                let e = std::io::Error::last_os_error();
                let _ = CloseHandle(handle);
                return Err(e);
            }
            Ok(SharedArena {
                base: view.Value.cast(),
                len,
                handle,
                layout: Layout::new(),
            })
        }
    }

    /// The handle the child inherits, as its command line names it.
    #[cfg(windows)]
    pub fn raw_handle(&self) -> u64 {
        self.handle.0 as usize as u64
    }
}

impl Drop for SharedArena {
    fn drop(&mut self) {
        // SAFETY: our own mapping and handle.
        unsafe {
            #[cfg(unix)]
            {
                libc::munmap(self.base.cast(), self.len);
            }
            #[cfg(windows)]
            {
                use windows::Win32::Foundation::CloseHandle;
                use windows::Win32::System::Memory::{MEMORY_MAPPED_VIEW_ADDRESS, UnmapViewOfFile};
                let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.base.cast(),
                });
                let _ = CloseHandle(self.handle);
            }
        }
    }
}
