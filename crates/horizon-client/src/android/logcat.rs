//! The game's stdout and stderr in logcat: Horizon's log lines, panics and
//! anything a library prints, under the tag `Horizon`.
#![allow(unsafe_code)]
use std::ffi::{CString, c_char, c_int};
use std::io::{BufRead, BufReader};
use std::os::fd::FromRawFd;

const INFO: c_int = 4;
const WARN: c_int = 5;

unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

pub fn write(warn: bool, line: &str) {
    let Ok(text) = CString::new(line.replace('\0', " ")) else {
        return;
    };
    let priority = if warn { WARN } else { INFO };
    // SAFETY: both strings are valid and NUL terminated.
    unsafe { __android_log_write(priority, c"Horizon".as_ptr(), text.as_ptr()) };
}

/// Point fds 1 and 2 at a pipe that a thread copies to logcat line by line.
pub fn redirect() {
    let mut fds = [0; 2];
    // SAFETY: fds has room for the two descriptors pipe writes.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return;
    }
    // SAFETY: plain descriptor duplication; the pipe's write end replaces both.
    unsafe {
        libc::dup2(fds[1], 1);
        libc::dup2(fds[1], 2);
        libc::close(fds[1]);
    }
    // SAFETY: the read end is ours alone from here on.
    let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let _ = std::thread::Builder::new()
        .name("logcat".into())
        .spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                let warn = line.contains("WARN") || line.contains("ERROR") || line.contains("panicked");
                write(warn, &line);
            }
        });
}
