//! Small re-implementations of the Python behaviour that awspick 2.x relied on.
//!
//! The Rust port has to produce the same bytes as the Python release it
//! replaces, and several of those bytes come straight out of CPython: what
//! `str.strip()` treats as whitespace, how `repr()` quotes a path, and the
//! wording of `UnicodeDecodeError` and `OSError`. Keeping them in one place
//! makes the dependency on CPython wording explicit.

use std::io;
use std::path::Path;

/// `str.isspace()` for one character.
///
/// CPython counts U+001C..U+001F (the information separators) as whitespace;
/// Unicode `White_Space`, which `char::is_whitespace` follows, does not.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `str.strip()` with no argument.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.rstrip()` with no argument.
pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `str.lower()`.
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `repr()` of a `str`.
pub fn repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if !is_printable(c) => {
                let n = c as u32;
                if n < 0x100 {
                    out.push_str(&format!("\\x{n:02x}"));
                } else if n < 0x10000 {
                    out.push_str(&format!("\\u{n:04x}"));
                } else {
                    out.push_str(&format!("\\U{n:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `str.isprintable()` for the characters that turn up in paths and profile
/// names: controls, separators other than the ASCII space, and the common
/// format characters. Unassigned code points are not covered.
fn is_printable(c: char) -> bool {
    let n = c as u32;
    !(n < 0x20
        || (0x7f..=0xa0).contains(&n)
        || n == 0xad
        || n == 0x1680
        || (0x2000..=0x200f).contains(&n)
        || (0x2028..=0x202f).contains(&n)
        || (0x205f..=0x206f).contains(&n)
        || n == 0x3000
        || n == 0xfeff
        || (0xfff9..=0xfffb).contains(&n))
}

/// `repr()` of a `pathlib.PosixPath`.
pub fn repr_posix_path(p: &Path) -> String {
    format!("PosixPath({})", repr_str(&p.to_string_lossy()))
}

/// Decode bytes the way CPython's strict UTF-8 codec does, returning the
/// message `str(UnicodeDecodeError)` would carry on failure.
///
/// The position is absolute. CPython reports it relative to the chunk the
/// text layer was decoding, which only differs for files over 8 KiB.
pub fn decode_utf8(bytes: &[u8]) -> Result<String, String> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => {
            let start = e.valid_up_to();
            let (end, reason) = match e.error_len() {
                None => (bytes.len(), "unexpected end of data"),
                Some(n) => {
                    let lead = bytes[start];
                    let reason = if (0xc2..=0xf4).contains(&lead) {
                        "invalid continuation byte"
                    } else {
                        "invalid start byte"
                    };
                    (start + n, reason)
                }
            };
            if end - start > 1 {
                Err(format!(
                    "'utf-8' codec can't decode bytes in position {}-{}: {}",
                    start,
                    end - 1,
                    reason
                ))
            } else {
                Err(format!(
                    "'utf-8' codec can't decode byte 0x{:02x} in position {}: {}",
                    bytes[start], start, reason
                ))
            }
        }
    }
}

/// `str(OSError)` for an error raised on `path`.
pub fn os_error(e: &io::Error, path: &Path) -> String {
    match e.raw_os_error() {
        Some(code) => format!(
            "[Errno {}] {}: {}",
            code,
            strerror(e),
            repr_str(&path.to_string_lossy())
        ),
        None => e.to_string(),
    }
}

/// The C library's `strerror` text, which `io::Error` formats as
/// "<text> (os error N)".
pub fn strerror(e: &io::Error) -> String {
    let s = e.to_string();
    match s.rfind(" (os error ") {
        Some(i) => s[..i].to_owned(),
        None => s,
    }
}

/// Universal-newline translation applied by Python text-mode reads.
pub fn universal_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// `os.path.basename`.
pub fn basename(s: &str) -> &str {
    match s.rfind('/') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_matches_python_whitespace() {
        assert_eq!(strip("\u{1c} a \u{a0}\n"), "a");
        assert_eq!(rstrip("a \u{1f}"), "a");
    }

    #[test]
    fn repr_quotes_like_python() {
        assert_eq!(repr_str("a"), "'a'");
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(repr_str("a'\""), "'a\\'\"'");
        assert_eq!(repr_str("x\n"), "'x\\n'");
    }

    #[test]
    fn utf8_errors_match_cpython_wording() {
        assert_eq!(
            decode_utf8(b"alias caf\xe9=b\n").unwrap_err(),
            "'utf-8' codec can't decode byte 0xe9 in position 9: invalid continuation byte"
        );
        assert_eq!(
            decode_utf8(b"a\xffb").unwrap_err(),
            "'utf-8' codec can't decode byte 0xff in position 1: invalid start byte"
        );
        assert_eq!(
            decode_utf8(b"ab\xe3\x81").unwrap_err(),
            "'utf-8' codec can't decode bytes in position 2-3: unexpected end of data"
        );
        assert_eq!(
            decode_utf8(b"\xe3\x81A").unwrap_err(),
            "'utf-8' codec can't decode bytes in position 0-1: invalid continuation byte"
        );
    }

    #[test]
    fn basename_like_posixpath() {
        assert_eq!(basename("/bin/zsh"), "zsh");
        assert_eq!(basename("zsh"), "zsh");
        assert_eq!(basename("/a/b/"), "");
    }
}
