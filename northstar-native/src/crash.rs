//! Prints a backtrace on a hard fault (access violation etc.) so a crash in FFI
//! code can be located; Rust's panic hook does not see those.
#![cfg(windows)]

use std::ffi::c_void;

#[repr(C)]
struct ExceptionRecord {
    code: u32,
    flags: u32,
    record: *mut ExceptionRecord,
    address: *mut c_void,
}

#[repr(C)]
struct ExceptionPointers {
    record: *mut ExceptionRecord,
    context: *mut c_void,
}

#[link(name = "kernel32")]
extern "system" {
    fn SetUnhandledExceptionFilter(filter: Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>) -> *mut c_void;
}

unsafe extern "system" fn report(pointers: *mut ExceptionPointers) -> i32 {
    let (code, address) = pointers
        .as_ref()
        .and_then(|p| p.record.as_ref())
        .map_or((0, std::ptr::null_mut()), |r| (r.code, r.address));
    eprintln!("FATAL: exception {code:#x} at {address:?} on thread {:?}\n{:?}", std::thread::current().name(), backtrace::Backtrace::new());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    1 // EXCEPTION_EXECUTE_HANDLER: terminate after reporting
}

pub fn install() {
    unsafe { SetUnhandledExceptionFilter(Some(report)) };
}
