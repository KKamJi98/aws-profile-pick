//! Writing to stdout and stderr.
//!
//! awspick 2.x printed through Python's `logging` (configured as
//! `LEVEL: message` on stderr) and through `print`. Every message here keeps
//! that shape. Write errors are ignored: a closed stderr must not turn a
//! successful profile switch into a panic, and a closed stdout (EPIPE) ends
//! the program quietly instead of with Rust's "failed printing" panic.

use std::io::{self, Write};

pub fn err_raw(s: &str) {
    let mut e = io::stderr().lock();
    let _ = e.write_all(s.as_bytes());
    let _ = e.flush();
}

pub fn eprintln(s: &str) {
    err_raw(&format!("{s}\n"));
}

pub fn info(s: &str) {
    eprintln(&format!("INFO: {s}"));
}

pub fn warning(s: &str) {
    eprintln(&format!("WARNING: {s}"));
}

pub fn error(s: &str) {
    eprintln(&format!("ERROR: {s}"));
}

/// Print one line to stdout. EPIPE and other write errors are swallowed.
pub fn println(s: &str) {
    let mut o = io::stdout().lock();
    let _ = writeln!(o, "{s}");
    let _ = o.flush();
}
