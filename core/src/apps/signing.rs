//! Who signed a download, and how anyone can tell: the keys Flow takes
//! downloads signed with ([`KEYS`], its update key alone today), each one's
//! fingerprint as OpenSSL works it out from its PEM ([`fingerprint`],
//! [`pem`]), and what the Mac's `codesign` says of a program inside one
//! ([`code_signature`]).
//!
//! A download's Ed25519 signature names no one and carries no date, and
//! nothing signs the catalog itself, only each download. So the name
//! flowapp shows is the one this table gives the key that checked it, never
//! one the catalog gives, and the date is the catalog's word
//! (`Download::signed`), which `app_catalog` writes as it adds a download
//! (`docs/design/flowapp-signatures.md`).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// A key Flow takes downloads signed with (base64, Ed25519), and the words
/// that name whose it is.
pub struct Key {
    pub key: &'static str,
    name: &'static str,
}

/// Every key flowapp knows, with whose it is.
pub const KEYS: &[Key] = &[Key { key: crate::UPDATE_PUBLIC_KEY, name: "flowapp.signer_flow" }];

/// Where Flow-releases keeps Flow's key as a PEM file, for checking a
/// download with OpenSSL alone.
pub const KEY_URL: &str = "https://raw.githubusercontent.com/arcanii/Flow-releases/main/flow-update.pem";

/// What comes before an Ed25519 key's 32 bytes in its DER
/// SubjectPublicKeyInfo, the form a PEM file holds (RFC 8410).
const ED25519_SPKI: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];

/// The name of whoever holds `key` (base64), in the user's words, or none
/// for a key that isn't one of Flow's.
pub fn signer(key: &str) -> Option<&'static str> {
    KEYS.iter().find(|k| k.key == key.trim()).map(|k| crate::i18n::t(k.name))
}

/// The name of whoever holds the key whose fingerprint is `fingerprint`,
/// as `installed.json` keeps it, or none for one that isn't Flow's.
pub fn signer_of(fingerprint: &str) -> Option<&'static str> {
    KEYS.iter().find(|k| self::fingerprint(k.key).is_some_and(|f| f.eq_ignore_ascii_case(fingerprint.trim()))).map(|k| crate::i18n::t(k.name))
}

/// `key` (base64) as DER, or none where it isn't 32 bytes.
fn der(key: &str) -> Option<Vec<u8>> {
    let raw: [u8; 32] = STANDARD.decode(key.trim()).ok()?.try_into().ok()?;
    let mut der = ED25519_SPKI.to_vec();
    der.extend_from_slice(&raw);
    Some(der)
}

/// `key`'s fingerprint: the SHA-256 of its DER, in hex, as `openssl pkey
/// -pubin -in key.pem -outform DER | openssl dgst -sha256` says it.
pub fn fingerprint(key: &str) -> Option<String> {
    der(key).map(|der| super::sha256(&der))
}

/// A fingerprint's first eight digits, in fours, as the tables show it.
pub fn short(fingerprint: &str) -> String {
    grouped(fingerprint.get(..8).unwrap_or(fingerprint))
}

/// A fingerprint in fours, as `info` shows it whole.
pub fn grouped(hex: &str) -> String {
    hex.as_bytes().chunks(4).map(|four| String::from_utf8_lossy(four).into_owned()).collect::<Vec<_>>().join(" ")
}

/// `key` (base64) as a PEM file, which OpenSSL reads.
pub fn pem(key: &str) -> Option<String> {
    der(key).map(|der| format!("-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n", STANDARD.encode(der)))
}

/// Who signed a program, as `codesign -dvv` tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeSigned {
    /// By a certificate: its first authority (`Developer ID Application:
    /// … (TEAM)`), its team, and when, where the signature carries a
    /// timestamp.
    Signed { authority: String, team: String, timestamp: String },
    /// Signed by no one, as the linker signs every program for Apple's chips.
    AdHoc,
    /// Not signed at all.
    Unsigned,
}

/// What `report` (`codesign -dvv`'s, from its standard error) says of who
/// signed a program.
pub fn code_signature(report: &str) -> CodeSigned {
    let value = |name: &str| report.lines().find_map(|line| line.strip_prefix(name).and_then(|rest| rest.strip_prefix('='))).map(str::trim);
    if let Some(authority) = value("Authority") {
        let team = value("TeamIdentifier").filter(|team| *team != "not set").unwrap_or_default();
        return CodeSigned::Signed { authority: authority.to_string(), team: team.to_string(), timestamp: value("Timestamp").unwrap_or_default().to_string() };
    }
    if value("Signature") == Some("adhoc") {
        return CodeSigned::AdHoc;
    }
    CodeSigned::Unsigned
}

/// The files of an app signed alike, as `info` shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGroup {
    /// Who signed them, and the first one's timestamp.
    pub signed: CodeSigned,
    pub files: usize,
}

/// `signatures` grouped by who signed them (a certificate's authority, ad
/// hoc, none), in the order each first comes.
pub fn group(signatures: impl IntoIterator<Item = CodeSigned>) -> Vec<CodeGroup> {
    let mut groups: Vec<CodeGroup> = Vec::new();
    for signed in signatures {
        let alike = |g: &CodeGroup| match (&g.signed, &signed) {
            (CodeSigned::Signed { authority: a, .. }, CodeSigned::Signed { authority: b, .. }) => a == b,
            (a, b) => a == b,
        };
        match groups.iter_mut().find(|g| alike(g)) {
            Some(group) => group.files += 1,
            None => groups.push(CodeGroup { signed, files: 1 }),
        }
    }
    groups
}

/// Whether `head`, a file's first bytes, begin a Mach-O program or library
/// (thin, either width, or fat), as `codesign` signs.
pub fn is_mach_o(head: &[u8]) -> bool {
    let Some(magic) = head.get(..4) else { return false };
    matches!(magic, [0xfe, 0xed, 0xfa, 0xce] | [0xfe, 0xed, 0xfa, 0xcf] | [0xce, 0xfa, 0xed, 0xfe] | [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe] | [0xbe, 0xba, 0xfe, 0xca])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flow's key's fingerprint, as OpenSSL 4.1.0 worked it out from the
    /// PEM on the Mac (2026-11-20).
    const FLOW_FINGERPRINT: &str = "f1866b49b0b42871463772bf1c9d77fae2091c32dc01681ae5d752f13008b4d0";

    #[test]
    fn flows_key_has_the_fingerprint_openssl_gives_its_pem() {
        assert_eq!(fingerprint(crate::UPDATE_PUBLIC_KEY).as_deref(), Some(FLOW_FINGERPRINT));
        assert_eq!(short(FLOW_FINGERPRINT), "f186 6b49");
        assert_eq!(grouped(FLOW_FINGERPRINT).split(' ').count(), 16);
        assert_eq!(grouped(FLOW_FINGERPRINT).replace(' ', ""), FLOW_FINGERPRINT);
    }

    #[test]
    fn flows_key_as_a_pem_is_what_openssl_wrote() {
        let written = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA2emwe9ELY5aktN4yvpnu8bIBdwemm7wqgZz6Fexmdwo=\n-----END PUBLIC KEY-----\n";
        assert_eq!(pem(crate::UPDATE_PUBLIC_KEY).as_deref(), Some(written));
    }

    #[test]
    fn a_key_that_isnt_32_bytes_has_no_fingerprint_or_pem() {
        assert_eq!(fingerprint("AAAA"), None);
        assert_eq!(pem("not base64"), None);
    }

    #[test]
    fn flows_key_is_flows_and_another_is_no_ones() {
        crate::i18n::set_language("en", &[]);
        assert_eq!(signer(crate::UPDATE_PUBLIC_KEY), Some("Flow"));
        assert_eq!(signer(&format!(" {}\n", crate::UPDATE_PUBLIC_KEY)), Some("Flow"));
        assert_eq!(signer("2emwe9ELY5aktN4yvpnu8bIBdwemm7wqgZz6Fexmdwp="), None);
    }

    #[test]
    fn codesigns_reports_say_who_signed_a_program() {
        // Microsoft's onnxruntime, as its release has it.
        let microsoft = "Executable=/x/libonnxruntime.1.31.0.dylib\nIdentifier=libonnxruntime.1\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=347476 flags=0x0(none) hashes=10853+2 location=embedded\nSignature size=9012\nAuthority=Developer ID Application: Microsoft Corporation (UBF8T346G9)\nAuthority=Developer ID Certification Authority\nAuthority=Apple Root CA\nTimestamp=9 Oct 2026 at 07:24:18\nInfo.plist=not bound\nTeamIdentifier=UBF8T346G9\nSealed Resources=none\nInternal requirements count=1 size=176\n";
        assert_eq!(
            code_signature(microsoft),
            CodeSigned::Signed { authority: "Developer ID Application: Microsoft Corporation (UBF8T346G9)".into(), team: "UBF8T346G9".into(), timestamp: "9 Oct 2026 at 07:24:18".into() }
        );
        // One a release signed again with its own Developer ID, with the
        // hardened runtime.
        let resigned = "Executable=/x/libonnxruntime.dylib\nIdentifier=com.example.onnxruntime\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20500 size=87051 flags=0x10000(runtime) hashes=2714+2 location=embedded\nSignature size=8975\nAuthority=Developer ID Application: Someone (ABCDE12345)\nAuthority=Developer ID Certification Authority\nAuthority=Apple Root CA\nTimestamp=18 Nov 2026 at 08:15:35\nInfo.plist=not bound\nTeamIdentifier=ABCDE12345\nRuntime Version=26.2.0\n";
        assert!(matches!(code_signature(resigned), CodeSigned::Signed { team, .. } if team == "ABCDE12345"));
        // What the linker signs a Rust program with.
        let linker = "Executable=/x/flowapp\nIdentifier=flowapp\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=33832 flags=0x20002(adhoc,linker-signed) hashes=1054+0 location=embedded\nSignature=adhoc\nInfo.plist=not bound\nTeamIdentifier=not set\nSealed Resources=none\nInternal requirements=none\n";
        assert_eq!(code_signature(linker), CodeSigned::AdHoc);
        assert_eq!(code_signature("/x/README.md: code object is not signed at all\n"), CodeSigned::Unsigned);
    }

    #[test]
    fn a_fingerprint_names_the_key_it_was_worked_out_from() {
        crate::i18n::set_language("en", &[]);
        assert_eq!(signer_of(FLOW_FINGERPRINT), Some("Flow"));
        assert_eq!(signer_of(&FLOW_FINGERPRINT.to_uppercase()), Some("Flow"));
        assert_eq!(signer_of("00"), None);
    }

    #[test]
    fn programs_signed_alike_are_grouped_in_the_order_they_come() {
        let by = |team: &str, at: &str| CodeSigned::Signed { authority: format!("Developer ID Application: Someone ({team})"), team: team.into(), timestamp: at.into() };
        let groups = group([by("A1", "1 Nov"), CodeSigned::AdHoc, by("A1", "2 Nov"), by("B2", "3 Nov"), CodeSigned::AdHoc]);
        assert_eq!(groups, [CodeGroup { signed: by("A1", "1 Nov"), files: 2 }, CodeGroup { signed: CodeSigned::AdHoc, files: 2 }, CodeGroup { signed: by("B2", "3 Nov"), files: 1 }]);
        assert!(group([]).is_empty());
    }

    #[test]
    fn mach_o_files_are_known_by_their_first_bytes() {
        assert!(is_mach_o(&[0xcf, 0xfa, 0xed, 0xfe, 0x0c]));
        assert!(is_mach_o(&[0xca, 0xfe, 0xba, 0xbe]));
        assert!(!is_mach_o(b"#!/bin/sh\n"));
        assert!(!is_mach_o(&[0xcf, 0xfa]));
    }
}
