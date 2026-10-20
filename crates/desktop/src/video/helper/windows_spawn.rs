//! Windows: starting the helper with a restricted primary token (every
//! privilege removed, a null restricting SID, untrusted integrity), inside
//! a job that allows no other process and no user-interface handles, with
//! only its three pipe ends and the arena's handle inherited; its main
//! thread borrows this process's own token (restricted only of privileges)
//! until the helper calls `RevertToSelf`, so the loader can map the
//! executable first (Chromium's two-token start). Written against the API
//! and cross-compiled here; unverified on Windows, where the helper's own
//! self-test decides whether it serves.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{
    CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, DuplicateTokenEx, PSID, SID_AND_ATTRIBUTES,
    SecurityImpersonation, SetTokenInformation, TOKEN_ALL_ACCESS, TOKEN_MANDATORY_LABEL,
    TokenImpersonation, TokenIntegrityLevel, WRITE_RESTRICTED,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_DISPLAYSETTINGS, JOB_OBJECT_UILIMIT_EXITWINDOWS,
    JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation, SetInformationJobObject,
};

/// `SE_GROUP_INTEGRITY` (winnt.h): the label SID's attribute.
const SE_GROUP_INTEGRITY: u32 = 0x20;
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcessToken,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, SetThreadToken, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

/// The running helper.
pub struct Process {
    process: HANDLE,
    thread: HANDLE,
    job: HANDLE,
}

impl Process {
    pub fn kill(&mut self) {
        // SAFETY: our own handles.
        unsafe {
            let _ = TerminateProcess(self.process, 1);
            WaitForSingleObject(self.process, 5000);
            let _ = CloseHandle(self.thread);
            let _ = CloseHandle(self.process);
            let _ = CloseHandle(self.job);
        }
    }
}

pub struct Pipe(std::fs::File);

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// A pipe: `(ours, the child's)`, the child's end inheritable.
fn pipe(child_reads: bool) -> Result<(HANDLE, HANDLE), String> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    // SAFETY: plain Win32 calls; results checked.
    unsafe {
        CreatePipe(&mut read, &mut write, None, 0).map_err(|e| format!("CreatePipe: {e}"))?;
        let (ours, theirs) = if child_reads {
            (write, read)
        } else {
            (read, write)
        };
        SetHandleInformation(theirs, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT)
            .map_err(|e| format!("SetHandleInformation: {e}"))?;
        Ok((ours, theirs))
    }
}

fn sid(text: &str) -> Result<PSID, String> {
    let mut sid = PSID::default();
    let text = wide(OsStr::new(text));
    // SAFETY: a NUL-terminated SID string; the few bytes allocated live as
    // long as the process.
    unsafe {
        ConvertStringSidToSidW(PCWSTR(text.as_ptr()), &mut sid)
            .map_err(|e| format!("ConvertStringSidToSid: {e}"))?;
    }
    Ok(sid)
}

/// The two tokens: the lockdown primary token, and the initial
/// impersonation token for the loader.
fn tokens() -> Result<(HANDLE, HANDLE), String> {
    // SAFETY: plain Win32 calls on this process's own token.
    unsafe {
        let mut ours = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_ALL_ACCESS, &mut ours)
            .map_err(|e| format!("OpenProcessToken: {e}"))?;
        // Lockdown: no privileges, everything restricted to the null SID,
        // so every access check against a securable object fails.
        let null = sid("S-1-0-0")?;
        let restricting = [SID_AND_ATTRIBUTES {
            Sid: null,
            Attributes: 0,
        }];
        let mut lockdown = HANDLE::default();
        CreateRestrictedToken(
            ours,
            DISABLE_MAX_PRIVILEGE | WRITE_RESTRICTED,
            None,
            None,
            Some(&restricting),
            &mut lockdown,
        )
        .map_err(|e| format!("CreateRestrictedToken: {e}"))?;
        // Untrusted integrity.
        let label = sid("S-1-16-0")?;
        let mandatory = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: label,
                Attributes: SE_GROUP_INTEGRITY,
            },
        };
        SetTokenInformation(
            lockdown,
            TokenIntegrityLevel,
            (&mandatory as *const TOKEN_MANDATORY_LABEL).cast(),
            std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32,
        )
        .map_err(|e| format!("SetTokenInformation: {e}"))?;
        // Initial: ours without privileges, as an impersonation token.
        let mut initial_primary = HANDLE::default();
        CreateRestrictedToken(
            ours,
            DISABLE_MAX_PRIVILEGE,
            None,
            None,
            None,
            &mut initial_primary,
        )
        .map_err(|e| format!("CreateRestrictedToken (initial): {e}"))?;
        let mut initial = HANDLE::default();
        DuplicateTokenEx(
            initial_primary,
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenImpersonation,
            &mut initial,
        )
        .map_err(|e| format!("DuplicateTokenEx: {e}"))?;
        let _ = CloseHandle(initial_primary);
        let _ = CloseHandle(ours);
        Ok((lockdown, initial))
    }
}

fn job() -> Result<HANDLE, String> {
    // SAFETY: plain Win32 calls; results checked.
    unsafe {
        let job = CreateJobObjectW(None, None).map_err(|e| format!("CreateJobObject: {e}"))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(|e| format!("SetInformationJobObject: {e}"))?;
        let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
            UIRestrictionsClass: JOB_OBJECT_UILIMIT_DESKTOP
                | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
                | JOB_OBJECT_UILIMIT_EXITWINDOWS
                | JOB_OBJECT_UILIMIT_GLOBALATOMS
                | JOB_OBJECT_UILIMIT_HANDLES
                | JOB_OBJECT_UILIMIT_READCLIPBOARD
                | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
                | JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
        };
        SetInformationJobObject(
            job,
            JobObjectBasicUIRestrictions,
            (&ui as *const JOBOBJECT_BASIC_UI_RESTRICTIONS).cast(),
            std::mem::size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
        )
        .map_err(|e| format!("SetInformationJobObject (UI): {e}"))?;
        Ok(job)
    }
}

type Spawned = (Process, Pipe, Pipe, Pipe);

pub fn spawn(path: &Path, arena: u64) -> Result<Spawned, String> {
    let (stdin_ours, stdin_theirs) = pipe(true)?;
    let (stdout_ours, stdout_theirs) = pipe(false)?;
    let (stderr_ours, stderr_theirs) = pipe(false)?;
    let arena_handle = HANDLE(arena as usize as *mut _);
    let (lockdown, initial) = tokens()?;
    let job = job()?;
    let command = format!(
        "\"{}\" --serve --arena {} --arena-len {}",
        path.display(),
        arena,
        ddi_video_protocol::Layout::LEN
    );
    let mut command = wide(OsStr::new(&command));
    let inherited = [stdin_theirs, stdout_theirs, stderr_theirs, arena_handle];
    // SAFETY: Win32 calls with the structures they document; every handle
    // is ours and every result checked.
    unsafe {
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut buffer = vec![0u8; size];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(buffer.as_mut_ptr().cast());
        InitializeProcThreadAttributeList(Some(list), 1, None, &mut size)
            .map_err(|e| format!("InitializeProcThreadAttributeList: {e}"))?;
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(inherited.as_ptr().cast()),
            std::mem::size_of_val(&inherited),
            None,
            None,
        )
        .map_err(|e| format!("UpdateProcThreadAttribute: {e}"))?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin_theirs;
        startup.StartupInfo.hStdOutput = stdout_theirs;
        startup.StartupInfo.hStdError = stderr_theirs;
        startup.lpAttributeList = list;
        let mut info = PROCESS_INFORMATION::default();
        // An empty environment block.
        let mut environment = [0u16, 0u16];
        let created = CreateProcessAsUserW(
            Some(lockdown),
            None,
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            true,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            Some(environment.as_mut_ptr().cast()),
            None,
            &startup.StartupInfo,
            &mut info,
        );
        DeleteProcThreadAttributeList(list);
        for h in inherited.iter().take(3) {
            let _ = CloseHandle(*h);
        }
        created.map_err(|e| format!("cannot start the video decoder ({}): {e}", path.display()))?;
        let mut process = Process {
            process: info.hProcess,
            thread: info.hThread,
            job,
        };
        let started = (|| -> Result<(), String> {
            AssignProcessToJobObject(job, info.hProcess)
                .map_err(|e| format!("AssignProcessToJobObject: {e}"))?;
            SetThreadToken(Some(&info.hThread), Some(initial))
                .map_err(|e| format!("SetThreadToken: {e}"))?;
            if ResumeThread(info.hThread) == u32::MAX {
                return Err("ResumeThread failed".into());
            }
            Ok(())
        })();
        let _ = CloseHandle(lockdown);
        let _ = CloseHandle(initial);
        if let Err(e) = started {
            process.kill();
            return Err(e);
        }
        Ok((
            process,
            Pipe(std::fs::File::from_raw_handle(stdin_ours.0.cast())),
            Pipe(std::fs::File::from_raw_handle(stdout_ours.0.cast())),
            Pipe(std::fs::File::from_raw_handle(stderr_ours.0.cast())),
        ))
    }
}
