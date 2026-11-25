//! Holds `flow_core::combining`'s table of the Mac's spacing marks to the
//! libc these tests run on: every mark in it is one the Mac's `wcwidth`
//! gives no width and unicode-width (the emulator's) a cell, and every
//! letter the two measure so apart is in it, but for those that aren't
//! spacing marks, listed here. A macOS that measures them otherwise fails
//! here, and the table is made again from it.
#![cfg(target_os = "macos")]

use std::ffi::CString;

use flow_core::combining::is_mac_spacing_mark;
use unicode_width::UnicodeWidthChar;

extern "C" {
    fn wcwidth(c: libc::wchar_t) -> libc::c_int;
}

/// What the Mac's libc measures the same way that are no spacing marks:
/// prepended concatenation marks and format characters, the line and
/// paragraph separators, and Tifinagh's consonant joiner, none of which
/// goes on the letter before.
const NOT_MARKS: &[(u32, u32)] = &[
    (0x0600, 0x0604), (0x06DD, 0x06DD), (0x2028, 0x2029), (0x2D7F, 0x2D7F), (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD), (0x110CD, 0x110CD), (0x13430, 0x1343F),
];

#[test]
fn the_macs_spacing_marks_are_those_its_wcwidth_gives_no_width_and_the_emulator_a_cell() {
    let utf8 = CString::new("en_US.UTF-8").unwrap();
    assert!(!unsafe { libc::setlocale(libc::LC_CTYPE, utf8.as_ptr()) }.is_null());
    let mut apart = Vec::new();
    for c in (0..=0x10FFFF).filter_map(char::from_u32) {
        let mac = unsafe { wcwidth(c as libc::wchar_t) };
        let (measured_apart, listed) = (mac == 0 && c.width() == Some(1), is_mac_spacing_mark(c));
        let not_a_mark = NOT_MARKS.iter().any(|&(first, last)| (first..=last).contains(&(c as u32)));
        if measured_apart != (listed || not_a_mark) || (listed && not_a_mark) {
            apart.push(format!("U+{:04X} (wcwidth {mac}, listed {listed})", c as u32));
        }
    }
    assert!(apart.is_empty(), "{}", apart.join(", "));
}
