//! The shared memory the parent created, mapped read-write: movie bytes
//! come in through its I/O region, frames and packets go out through the
//! others (`ddi_video_protocol::Layout`). Mapped before the sandbox closes
//! (a mapping of an inherited descriptor needs no file access).

use ddi_video_protocol::Layout;
use std::ops::Range;

pub struct Arena {
    base: *mut u8,
    len: usize,
    pub layout: Layout,
}

// The mapping is shared with the parent by design; this process touches
// it from one thread at a time (the decoder's own threads never see it).
unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

impl Arena {
    pub fn len(&self) -> usize {
        self.len
    }

    /// Bytes of `range`, which must lie inside the arena.
    pub fn slice(&self, range: &Range<usize>) -> &[u8] {
        assert!(range.end <= self.len && range.start <= range.end);
        // SAFETY: the mapping lives as long as `self`; the range is checked.
        unsafe { std::slice::from_raw_parts(self.base.add(range.start), range.end - range.start) }
    }

    /// The same, writable.
    #[allow(clippy::mut_from_ref)]
    pub fn slice_mut(&self, range: &Range<usize>) -> &mut [u8] {
        assert!(range.end <= self.len && range.start <= range.end);
        // SAFETY: as above; callers never hold two overlapping slices.
        unsafe {
            std::slice::from_raw_parts_mut(self.base.add(range.start), range.end - range.start)
        }
    }

    /// A raw pointer and length for the shim's frame buffer.
    pub fn raw(&self, range: &Range<usize>) -> (*mut u8, usize) {
        assert!(range.end <= self.len && range.start <= range.end);
        // SAFETY: checked above.
        (
            unsafe { self.base.add(range.start) },
            range.end - range.start,
        )
    }

    #[cfg(unix)]
    pub fn map(fd: i32, len: usize) -> Result<Arena, String> {
        if len != Layout::LEN {
            return Err(format!("arena of {len} bytes, expected {}", Layout::LEN));
        }
        // SAFETY: plain libc calls on a descriptor the parent handed over.
        unsafe {
            let mut st: libc::stat = std::mem::zeroed();
            if libc::fstat(fd, &mut st) != 0 {
                return Err("cannot stat the arena".into());
            }
            if st.st_size < 0 || st.st_size as u64 != len as u64 {
                return Err(format!("arena is {} bytes, expected {len}", st.st_size));
            }
            let p = libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            if p == libc::MAP_FAILED {
                return Err("cannot map the arena".into());
            }
            libc::close(fd);
            Ok(Arena {
                base: p.cast(),
                len,
                layout: Layout::new(),
            })
        }
    }

    #[cfg(windows)]
    pub fn map(handle: u64, len: usize) -> Result<Arena, String> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Memory::{FILE_MAP_ALL_ACCESS, MapViewOfFile};
        if len != Layout::LEN {
            return Err(format!("arena of {len} bytes, expected {}", Layout::LEN));
        }
        // SAFETY: the handle was inherited from the parent; a bad one fails.
        let view = unsafe {
            MapViewOfFile(
                HANDLE(handle as isize as *mut _),
                FILE_MAP_ALL_ACCESS,
                0,
                0,
                len,
            )
        };
        if view.Value.is_null() {
            return Err("cannot map the arena".into());
        }
        Ok(Arena {
            base: view.Value.cast(),
            len,
            layout: Layout::new(),
        })
    }
}
