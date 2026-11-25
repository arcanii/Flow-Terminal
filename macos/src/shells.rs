//! The shells' environment on macOS. The shells themselves are the shared
//! Unix ones (`flow_core::shell::unix_profiles`).

use std::path::Path;

use objc2_foundation::NSLocale;

/// The languages in System Settings, most wanted first ("ja-JP").
pub fn preferred_languages() -> Vec<String> {
    let list = NSLocale::preferredLanguages();
    (0..list.count()).map(|i| list.objectAtIndex(i).to_string()).collect()
}

/// Apps opened from the Finder or the Dock get no LANG, so shells and the
/// programs they run would fall back to ASCII. Like Terminal, pick the
/// user's UTF-8 locale unless one is already set.
pub fn env() -> Vec<(String, String)> {
    let set = |name| std::env::var(name).is_ok_and(|v| !v.is_empty());
    if set("LC_ALL") || set("LC_CTYPE") || set("LANG") {
        return Vec::new();
    }
    let identifier = NSLocale::currentLocale().localeIdentifier().to_string();
    let installed = |name: &str| Path::new("/usr/share/locale").join(name).is_dir();
    vec![("LANG".into(), utf8_locale(&identifier, installed))]
}

/// "en_GB" or "en_GB@rg=chzzzz" -> "en_GB.UTF-8" when the system has that
/// locale, else the language's main one ("de_DE.UTF-8"), else en_US.UTF-8.
fn utf8_locale(identifier: &str, installed: impl Fn(&str) -> bool) -> String {
    let base = identifier.split('@').next().unwrap_or_default().replace('-', "_");
    let mut parts = base.split('_');
    let lang = parts.next().unwrap_or_default().to_ascii_lowercase();
    // Skip a script part such as "Hans" in zh_Hans_CN.
    let region = parts.rev().find(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_uppercase()));
    let candidates = [region.map(|r| format!("{lang}_{r}.UTF-8")), Some(format!("{lang}_{}.UTF-8", lang.to_uppercase()))];
    candidates
        .into_iter()
        .flatten()
        .find(|name| !lang.is_empty() && installed(name))
        .unwrap_or_else(|| "en_US.UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_an_installed_utf8_locale() {
        let installed = |name: &str| ["en_US.UTF-8", "en_GB.UTF-8", "de_DE.UTF-8", "zh_CN.UTF-8"].contains(&name);
        assert_eq!(utf8_locale("en_GB", installed), "en_GB.UTF-8");
        assert_eq!(utf8_locale("en_US@rg=uszzzz", installed), "en_US.UTF-8");
        assert_eq!(utf8_locale("de_AT", installed), "de_DE.UTF-8");
        assert_eq!(utf8_locale("zh-Hans_CN", installed), "zh_CN.UTF-8");
        assert_eq!(utf8_locale("tlh", installed), "en_US.UTF-8");
        assert_eq!(utf8_locale("", installed), "en_US.UTF-8");
    }
}
