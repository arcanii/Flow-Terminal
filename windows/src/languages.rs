//! The user's display languages, which Flow's own follows when its Language
//! setting is Automatic.

use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

/// The Windows display language and its fallbacks, most wanted first
/// ("ja-JP", "en-US"). Empty if Windows doesn't say, and Flow then goes by
/// LANG like other systems.
pub fn preferred() -> Vec<String> {
    let (mut count, mut len) = (0u32, 0u32);
    // The first call asks how long the list is, the second fills it in.
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut len) == 0 || len == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut len) == 0 {
            return Vec::new();
        }
        buf.truncate(len as usize);
        split_multi_string(&buf)
    }
}

/// A Windows multi-string ("ja-JP\0en-US\0\0") as its strings.
fn split_multi_string(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_string_splits_at_its_nulls() {
        let buf: Vec<u16> = "ja-JP\0en-US\0\0".encode_utf16().collect();
        assert_eq!(split_multi_string(&buf), ["ja-JP", "en-US"]);
        assert!(split_multi_string(&[0, 0]).is_empty());
    }

    #[test]
    fn windows_names_at_least_one_display_language() {
        let languages = preferred();
        eprintln!("display languages: {languages:?}");
        assert!(!languages.is_empty());
        for tag in &languages {
            assert!(tag.split('-').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric())), "{tag}");
        }
    }
}
