//! MF is optional even at process startup (Windows N may lack mfplat.dll).
//! Load only from System32, and retain the module until every COM object dies.
use super::*;
use std::ffi::c_void;
use windows::{
    Win32::{
        Foundation::FreeLibrary,
        System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
    },
    core::{HRESULT, PCSTR, w},
};
type Startup = unsafe extern "system" fn(u32, u32) -> HRESULT;
type Shutdown = unsafe extern "system" fn() -> HRESULT;
type Manager = unsafe extern "system" fn(*mut u32, *mut *mut c_void) -> HRESULT;
type Enumerate = unsafe extern "system" fn(
    GUID,
    MFT_ENUM_FLAG,
    *const MFT_REGISTER_TYPE_INFO,
    *const MFT_REGISTER_TYPE_INFO,
    *mut *mut *mut c_void,
    *mut u32,
) -> HRESULT;
type Create = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;
type Buffer = unsafe extern "system" fn(u32, *mut *mut c_void) -> HRESULT;
struct Library(HMODULE);
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}
pub(super) struct Api {
    startup: Startup,
    shutdown: Shutdown,
    manager: Manager,
    enumerate: Enumerate,
    media_type: Create,
    sample: Create,
    buffer: Buffer,
    _library: Library,
}
impl Api {
    pub fn load() -> Result<Self, String> {
        // SAFETY: fixed OS DLL name, restricted search; the guard closes it on
        // partial symbol resolution. Each export's ABI is its Windows SDK ABI.
        unsafe {
            let library = Library(super::w(
                LoadLibraryExW(w!("mfplat.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32),
                "load mfplat.dll",
            )?);
            macro_rules! get {
                ($name:literal, $ty:ty) => {
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(
                        GetProcAddress(library.0, PCSTR(concat!($name, "\0").as_ptr()))
                            .ok_or(concat!("MF missing export ", $name))?,
                    )
                };
            }
            Ok(Self {
                startup: get!("MFStartup", Startup),
                shutdown: get!("MFShutdown", Shutdown),
                manager: get!("MFCreateDXGIDeviceManager", Manager),
                enumerate: get!("MFTEnumEx", Enumerate),
                media_type: get!("MFCreateMediaType", Create),
                sample: get!("MFCreateSample", Create),
                buffer: get!("MFCreateMemoryBuffer", Buffer),
                _library: library,
            })
        }
    }
    pub fn start(&self) -> Result<(), String> {
        unsafe { super::w((self.startup)(MF_VERSION, MFSTARTUP_FULL).ok(), "startup") }
    }
    pub fn stop(&self) {
        unsafe {
            let _ = (self.shutdown)();
        }
    }
    pub fn manager(&self) -> Result<(IMFDXGIDeviceManager, u32), String> {
        let mut token = 0;
        let manager =
            unsafe { self.object("DXGI manager", |out| (self.manager)(&mut token, out)) }?;
        Ok((manager, token))
    }
    pub fn enumerate(
        &self,
        input: &MFT_REGISTER_TYPE_INFO,
        output: &MFT_REGISTER_TYPE_INFO,
    ) -> Result<Activations, String> {
        let mut list = Activations {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        let mut count = 0;
        // SAFETY: typed descriptors and aligned out pointers. The successful
        // array is owned by Activations, which releases all records and memory.
        unsafe {
            let result = (self.enumerate)(
                MFT_CATEGORY_VIDEO_DECODER,
                MFT_ENUM_FLAG_SYNCMFT
                    | MFT_ENUM_FLAG_ASYNCMFT
                    | MFT_ENUM_FLAG_HARDWARE
                    | MFT_ENUM_FLAG_SORTANDFILTER,
                input,
                output,
                (&mut list.ptr as *mut *mut Option<IMFActivate>).cast(),
                &mut count,
            );
            list.len = count as usize;
            super::w(result.ok(), "decoder enumeration")?;
        }
        Ok(list)
    }
    pub fn media_type(&self) -> Result<IMFMediaType, String> {
        unsafe { self.object("media type", |out| (self.media_type)(out)) }
    }
    pub fn sample(&self) -> Result<IMFSample, String> {
        unsafe { self.object("sample", |out| (self.sample)(out)) }
    }
    pub fn buffer(&self, len: u32) -> Result<IMFMediaBuffer, String> {
        unsafe { self.object("buffer", |out| (self.buffer)(len, out)) }
    }
    /// Each caller fixes T to the exact interface returned by its SDK export.
    unsafe fn object<T: Interface>(
        &self,
        what: &str,
        call: impl FnOnce(*mut *mut c_void) -> HRESULT,
    ) -> Result<T, String> {
        let mut ptr = std::ptr::null_mut();
        let result = call(&mut ptr);
        // SAFETY: any returned interface is an owned reference of exactly T.
        // Own it before checking HRESULT so a partial result is released too.
        let object = (!ptr.is_null()).then(|| unsafe { T::from_raw(ptr) });
        super::w(result.ok(), what)?;
        object.ok_or_else(|| format!("MF null {what}"))
    }
}
