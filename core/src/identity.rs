//! The numbers an old workstation's start screen shows: its serial, its
//! Ethernet address and the host ID that follows from it. They're made up,
//! but the same at every launch on one computer and different on another:
//! seeded from the OS's own id for the machine (`Platform::machine_id`),
//! hashed with a phrase of Flow's so the numbers can't be traced back to it,
//! with SHA-256, which doesn't change between builds as std's hasher may;
//! where the OS gives none, from a seed made once and kept beside the config.
//! Theme details' **Its real numbers** shows the computer's own instead
//! (`Platform::real_numbers`), read only while it's on. Linux's files and
//! SMBIOS's system serial are read here, so they're tested on any OS.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

/// The computer's own Ethernet address and serial number, where the OS says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RealNumbers {
    pub ether: Option<[u8; 6]>,
    pub serial: Option<String>,
}

/// The seed of a computer the OS calls `id`.
pub fn seed_of(id: &str) -> u64 {
    let hash = Sha256::new().chain_update(b"Flow's old workstations, made up\0").chain_update(id.trim().as_bytes()).finalize();
    u64::from_le_bytes(hash[..8].try_into().expect("eight of SHA-256's bytes"))
}

/// This computer's seed: from `id` where the OS gave one, else the one kept
/// in `kept`, made and written there the first time.
pub fn seed(id: Option<&str>, kept: &Path) -> u64 {
    if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
        return seed_of(id);
    }
    if let Some(seed) = fs::read_to_string(kept).ok().and_then(|text| text.trim().parse().ok()) {
        return seed;
    }
    let made = seed_of(&format!("{:?} {}", std::time::SystemTime::now(), std::process::id()));
    if let Some(dir) = kept.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(error) = fs::write(kept, format!("{made}\n")) {
        log::warn!("could not keep the start screens' seed in {}: {error}", kept.display());
    }
    made
}

/// Linux's id for the machine, under `root` (`/`): systemd's
/// `/etc/machine-id`, else D-Bus's.
pub fn linux_machine_id(root: &Path) -> Option<String> {
    ["etc/machine-id", "var/lib/dbus/machine-id"]
        .iter()
        .find_map(|path| fs::read_to_string(root.join(path)).ok())
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// Linux's real numbers, under `root` (`/`): the address of the first
/// network interface a device backs (`/sys/class/net/*/device`, which a
/// loopback, a bridge or a container's interface hasn't), and the system's
/// serial from DMI, which mostly only root may read, so it stays made up.
pub fn linux_real(root: &Path) -> RealNumbers {
    let net = root.join("sys/class/net");
    let mut names: Vec<String> = fs::read_dir(&net).into_iter().flatten().flatten().filter_map(|e| e.file_name().into_string().ok()).collect();
    names.sort();
    let ether = names
        .iter()
        .filter(|name| net.join(name).join("device").exists())
        .find_map(|name| fs::read_to_string(net.join(name).join("address")).ok().and_then(|a| mac_address(&a)));
    let serial = fs::read_to_string(root.join("sys/class/dmi/id/product_serial")).ok().and_then(|s| usable_serial(&s));
    RealNumbers { ether, serial }
}

/// An Ethernet address written `a4:83:e7:0f:02:c1`, unless it's all zeros.
pub fn mac_address(text: &str) -> Option<[u8; 6]> {
    let bytes: Vec<u8> = text.trim().split([':', '-']).map(|b| u8::from_str_radix(b, 16).ok()).collect::<Option<_>>()?;
    let address: [u8; 6] = bytes.try_into().ok()?;
    (address != [0; 6]).then_some(address)
}

/// SMBIOS's system serial number, from `RawSMBIOSData` (eight bytes, then
/// the table): structure type 1's string numbered at 0x07, unless it's a
/// placeholder a maker left.
pub fn smbios_serial(raw: &[u8]) -> Option<String> {
    let length = u32::from_le_bytes(raw.get(4..8)?.try_into().ok()?) as usize;
    let t = &raw[8..raw.len().min(8usize.saturating_add(length))];
    let mut p = 0;
    while p + 4 <= t.len() {
        let (kind, len) = (t[p], t[p + 1] as usize);
        if len < 4 || p + len > t.len() {
            return None;
        }
        let strings = &t[p + len..];
        let end = strings.windows(2).position(|w| w == [0, 0])?;
        if kind == 1 && len > 0x07 {
            let index = t[p + 0x07] as usize;
            return strings[..end].split(|&b| b == 0).nth(index.checked_sub(1)?).and_then(|s| usable_serial(&String::from_utf8_lossy(s)));
        }
        if kind == 127 {
            return None;
        }
        p += len + end + 2;
    }
    None
}

/// A serial as a start screen may show it: letters and digits only, at most
/// 16, and none of the placeholders makers leave in its place.
pub fn usable_serial(text: &str) -> Option<String> {
    let text = text.trim();
    let placeholder = ["to be filled by o.e.m.", "default string", "system serial number", "not specified", "none", "n/a", "0", "0123456789", "123456789"];
    if text.is_empty() || placeholder.contains(&text.to_lowercase().as_str()) {
        return None;
    }
    let kept: String = text.chars().filter(char::is_ascii_alphanumeric).take(16).collect();
    (!kept.is_empty()).then_some(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_computer_gets_one_seed_another_its_own_and_none_keeps_one_made() {
        let id = "8C2F4A10-6E3B-4F1D-9A77-0B5E2C9D1F33";
        assert_eq!(seed_of(id), seed_of(id));
        assert_eq!(seed_of(id), seed_of(&format!(" {id}\n")));
        assert_ne!(seed_of(id), seed_of("another computer"));
        // Without an id: made once, then kept.
        let kept = std::env::temp_dir().join(format!("flow-seed-{}", std::process::id())).join("machine-seed");
        let _ = fs::remove_file(&kept);
        let made = seed(None, &kept);
        assert_eq!(seed(Some("  "), &kept), made);
        assert_eq!(seed(Some(id), &kept), seed_of(id));
        let _ = fs::remove_dir_all(kept.parent().unwrap());
    }

    #[test]
    fn linuxs_id_address_and_serial_come_from_its_files() {
        let root = std::env::temp_dir().join(format!("flow-identity-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = |path: &str, text: &str| {
            let at = root.join(path);
            fs::create_dir_all(at.parent().unwrap()).unwrap();
            fs::write(at, text).unwrap();
        };
        assert_eq!(linux_machine_id(&root), None);
        file("var/lib/dbus/machine-id", "  dbus0123\n");
        assert_eq!(linux_machine_id(&root).as_deref(), Some("dbus0123"));
        file("etc/machine-id", "systemd4567\n");
        assert_eq!(linux_machine_id(&root).as_deref(), Some("systemd4567"));
        // A bridge and the loopback have no device; the card has.
        file("sys/class/net/br0/address", "02:42:ac:11:00:01\n");
        file("sys/class/net/lo/address", "00:00:00:00:00:00\n");
        file("sys/class/net/enp5s0/address", "a4:83:e7:0f:02:c1\n");
        fs::create_dir_all(root.join("sys/class/net/enp5s0/device")).unwrap();
        let real = linux_real(&root);
        assert_eq!((real.ether, real.serial), (Some([0xa4, 0x83, 0xe7, 0x0f, 0x02, 0xc1]), None));
        file("sys/class/dmi/id/product_serial", "To Be Filled By O.E.M.\n");
        assert_eq!(linux_real(&root).serial, None);
        file("sys/class/dmi/id/product_serial", "PF2X-9QK7\n");
        assert_eq!(linux_real(&root).serial.as_deref(), Some("PF2X9QK7"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn smbios_gives_the_systems_serial_from_its_strings() {
        // A BIOS structure (type 0), then the system's (type 1): maker,
        // product, version and serial as strings 1 to 4, and the end.
        let raw = |serial: &str| {
            let mut table = vec![0u8, 4, 0, 0, b'B', 0, 0];
            table.extend([1u8, 8, 1, 0, 1, 2, 3, 4]);
            table.extend(format!("Acme\0Box 9\0v1\0{serial}\0\0").bytes());
            table.extend([127u8, 4, 2, 0, 0, 0]);
            let mut raw = vec![0u8, 3, 4, 0];
            raw.extend((table.len() as u32).to_le_bytes());
            raw.extend(&table);
            raw
        };
        assert_eq!(smbios_serial(&raw("SN-0042Z")).as_deref(), Some("SN0042Z"));
        // A placeholder, or no table at all.
        assert_eq!(smbios_serial(&raw("Default string")), None);
        assert_eq!(smbios_serial(&raw("x")[..8]), None);
        assert_eq!(mac_address("00:00:00:00:00:00"), None);
        assert_eq!(mac_address("A4-83-E7-0F-02-C1"), Some([0xa4, 0x83, 0xe7, 0x0f, 0x02, 0xc1]));
    }
}
