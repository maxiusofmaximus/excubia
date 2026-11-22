//! Windows only: the Job Object a document reader runs in. Closing the job kills
//! whatever is still inside it, so a reader cannot outlive the host, cannot start
//! a child process and cannot take the desktop's handles or clipboard.

use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, THREADENTRY32, TH32CS_SNAPTHREAD,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicUIRestrictions,
    JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECTINFOCLASS,
    JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_DISPLAYSETTINGS, JOB_OBJECT_UILIMIT_EXITWINDOWS,
    JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
};
use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

/// Most memory one reader process may commit. Real documents stay far below it.
pub const READER_MEMORY_LIMIT: usize = 1024 * 1024 * 1024;

/// A kernel handle, closed on drop.
struct Handle {
    raw: HANDLE,
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: `raw` came from a successful Win32 call and is closed only here.
        unsafe { CloseHandle(self.raw) };
    }
}

/// The job a reader lives in. Dropping it kills whatever is still inside.
pub struct Job {
    handle: Handle,
}

impl Job {
    fn set<T>(&self, class: JOBOBJECTINFOCLASS, info: &T) -> io::Result<()> {
        // SAFETY: `info` points to a live `T` whose size is passed alongside it.
        let ok = unsafe {
            SetInformationJobObject(
                self.handle.raw,
                class,
                (info as *const T).cast::<c_void>(),
                size_of::<T>() as u32,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Puts the suspended `child` in a new job with its limits, then lets it run.
/// On error the child is still suspended and the caller kills it.
pub fn isolate(child: &Child) -> io::Result<Job> {
    // SAFETY: null attributes and name make an anonymous job.
    let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // From here on the handle is closed when `job` drops, even on a later error.
    let job = Job {
        handle: Handle { raw },
    };

    // SAFETY: an all-zero struct is a valid starting point for these plain fields.
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    // One active process: the reader itself. It cannot start a child.
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    limits.ProcessMemoryLimit = READER_MEMORY_LIMIT;
    job.set(JobObjectExtendedLimitInformation, &limits)?;

    let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
        UIRestrictionsClass: JOB_OBJECT_UILIMIT_HANDLES
            | JOB_OBJECT_UILIMIT_READCLIPBOARD
            | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
            | JOB_OBJECT_UILIMIT_DESKTOP
            | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
            | JOB_OBJECT_UILIMIT_EXITWINDOWS
            | JOB_OBJECT_UILIMIT_GLOBALATOMS
            | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS,
    };
    job.set(JobObjectBasicUIRestrictions, &ui)?;

    // SAFETY: both handles are live; the process handle belongs to `child`.
    if unsafe { AssignProcessToJobObject(job.handle.raw, child.as_raw_handle()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    resume(child.id())?;
    Ok(job)
}

/// Resumes the threads of `pid`. `std` drops the primary thread's handle at spawn,
/// so the threads are found through a toolhelp snapshot.
fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: a thread snapshot takes only flags and a pid (0 means all processes).
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Handle { raw };

    // SAFETY: an all-zero THREADENTRY32 is valid once `dwSize` is set.
    let mut entry: THREADENTRY32 = unsafe { zeroed() };
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    let mut found = false;
    let mut more = unsafe { Thread32First(snapshot.raw, &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: plain open of a thread of our own child; checked below.
            let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            let thread = Handle { raw };
            // SAFETY: `thread` is a live handle with THREAD_SUSPEND_RESUME.
            if unsafe { ResumeThread(thread.raw) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            found = true;
        }
        // SAFETY: same snapshot and entry as above.
        more = unsafe { Thread32Next(snapshot.raw, &mut entry) } != 0;
    }
    if found {
        Ok(())
    } else {
        Err(io::Error::other("the suspended CLI has no thread"))
    }
}
