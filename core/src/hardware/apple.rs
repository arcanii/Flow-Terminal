//! A Mac's memory and buses from its device tree (IOKit's `IODeviceTree`
//! plane), whose properties the macOS crate fetches as bytes. Apple silicon
//! names its memory in `/chosen` (`dram-size`, `dram-type`); an Intel Mac
//! lists its DIMMs in `/memory`. Neither key a serial number sits under is
//! read (`dimm-serial-number`, `slot-names`), and no vendor either.

use super::{hex_byte, le_u32, shown_kind, speed, Bank};

/// Apple silicon's memory, one pool in the chip's package and so honestly
/// its one bank: `dram-size` (a little-endian u64 in bytes, or a u32) and
/// `dram-type` ("LPDDR5"). None when there's no size.
pub fn chosen_bank(dram_size: &[u8], dram_type: &[u8]) -> Option<Bank> {
    let bytes = match dram_size.first_chunk::<8>() {
        Some(b) => u64::from_le_bytes(*b),
        None => le_u32(dram_size) as u64,
    };
    let megabytes = bytes >> 20;
    (megabytes > 0).then(|| Bank { megabytes, kind: kind(dram_type), speed: 0 })
}

/// An Intel Mac's DIMMs from `/memory`: `reg`, eight bytes a slot, and the
/// NUL-separated lists `dimm-types` ("DDR4") and `dimm-speeds` ("2667
/// MHz"). Each slot's size is its high word then its low one, each
/// little-endian, as AppleSMBIOS writes it, it seems: no Intel Mac has
/// checked it, and `Hardware::banks_agree` catches a wrong guess.
pub fn memory_banks(reg: &[u8], dimm_types: &[u8], dimm_speeds: &[u8]) -> Vec<Bank> {
    let slots: Vec<u64> = reg.as_chunks::<8>().0.iter().map(|b| ((le_u32(&b[..4]) as u64) << 32) | le_u32(&b[4..]) as u64).collect();
    let types = list(dimm_types);
    let speeds = list(dimm_speeds);
    // A list that doesn't go slot by slot: one kind for all if it names one.
    let one_kind = types.first().filter(|first| types.iter().all(|t| t == *first)).map(|t| kind(t.as_bytes())).unwrap_or_default();
    slots
        .iter()
        .enumerate()
        .map(|(i, bytes)| {
            let kind = if types.len() == slots.len() { kind(types[i].as_bytes()) } else { one_kind.clone() };
            let speed = if speeds.len() == slots.len() { leading_number(&speeds[i]) } else { 0 };
            Bank { megabytes: bytes >> 20, kind, speed }
        })
        .collect()
}

/// The IOMMU over a PCI function on Apple silicon, as its DART's unit
/// address ("94000000"): the DART whose mapper the function's
/// `iommu-parent` names, or, for a port's bridge, the one its
/// `function-dart_self` names (each a handle, a little-endian u32 first).
/// `darts` takes each mapper's handle, and each DART's own, to its DART's
/// address. Empty where neither names one it knows. Apple silicon's DARTs
/// sit beside its PCIe controllers in the tree rather than above them, so
/// only these handles tie the two together.
pub fn iommu(parent: &[u8], dart_self: &[u8], darts: &std::collections::HashMap<u32, String>) -> String {
    [parent, dart_self].into_iter().map(le_u32).filter(|&handle| handle != 0).find_map(|handle| darts.get(&handle)).cloned().unwrap_or_default()
}

/// A DART's unit address as the tree gives it, kept only where it's hex
/// digits alone and no longer than an address: "94000000".
pub fn dart_address(location: &str) -> Option<String> {
    (!location.is_empty() && location.len() <= 16 && location.chars().all(|c| c.is_ascii_hexdigit())).then(|| location.to_string())
}

/// A device-tree location as device and function: "0", "0,1", "1C,4".
pub fn dev_fn(location: &str) -> Option<(u8, u8)> {
    let mut parts = location.split(',');
    let dev = hex_byte(parts.next()?).filter(|d| *d <= 0x1f)?;
    let func = match parts.next() {
        Some(f) => hex_byte(f).filter(|f| *f <= 7)?,
        None => 0,
    };
    parts.next().is_none().then_some((dev, func))
}

/// A memory kind from a device-tree string, where it says something.
fn kind(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches('\0');
    if shown_kind(text) {
        text.to_string()
    } else {
        String::new()
    }
}

/// A NUL-separated list of strings.
fn list(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches('\0');
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\0').map(str::to_string).collect()
}

/// The number "2667 MHz" starts with.
fn leading_number(text: &str) -> u32 {
    let digits: String = text.trim_start().chars().take_while(char::is_ascii_digit).take(9).collect();
    speed(digits.parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pci_function_names_its_dart_through_a_mapper_and_a_bridge_through_its_own() {
        // An Apple silicon Mac's: wlan and Bluetooth under port 0 through
        // dart-apcie0's mappers (0x6c, 0x6d), the SD reader under port 1
        // through dart-apcie1's (0x72); the ports' bridges name the DARTs
        // themselves (0x6b, 0x71).
        let darts: std::collections::HashMap<u32, String> =
            [(0x6b, "94000000"), (0x6c, "94000000"), (0x6d, "94000000"), (0x71, "95000000"), (0x72, "95000000")].into_iter().map(|(h, a)| (h, a.to_string())).collect();
        let handle = |h: u32| h.to_le_bytes().to_vec();
        assert_eq!(iommu(&handle(0x6c), &[], &darts), "94000000");
        assert_eq!(iommu(&handle(0x72), &[], &darts), "95000000");
        // A bridge's `function-dart_self` holds the handle, then a word.
        assert_eq!(iommu(&[], &[0x71, 0, 0, 0, b'f', b'l', b'e', b'S'], &darts), "95000000");
        // Neither, or one it doesn't know: none.
        assert_eq!(iommu(&[], &[], &darts), "");
        assert_eq!(iommu(&handle(0x99), &[], &darts), "");
        assert_eq!((dart_address("94000000"), dart_address(""), dart_address("94,0"), dart_address(&"9".repeat(17))), (Some("94000000".into()), None, None, None));
        // As the probe shows it, over the controller's own path.
        let f = |iommu: &str| super::super::PciFunction { root: "/arm-io@10F00000/apcie0@80000000".into(), iommu: iommu.into(), chain: vec![(0, 0)], class: 0x060400, name: "pci-bridge0".into(), vendor: 0x106b, device: 0x1002 };
        let probes = super::super::pci_probes(vec![f("94000000")]);
        assert_eq!(probes[0].path, "/iommu@94000000/arm-io@10F00000/apcie0@80000000");
        assert!(super::super::shown_path(&probes[0].path));
    }

    #[test]
    fn apple_silicons_memory_is_one_bank() {
        // An Apple silicon Mac's /chosen: 32 GB of LPDDR5.
        let size = 0x8_0000_0000u64.to_le_bytes();
        assert_eq!(chosen_bank(&size, b"LPDDR5\0"), Some(Bank { megabytes: 32768, kind: "LPDDR5".into(), speed: 0 }));
        assert_eq!(chosen_bank(&(1u32 << 31).to_le_bytes(), b""), Some(Bank { megabytes: 2048, kind: String::new(), speed: 0 }));
        // A kind that says nothing, or isn't a kind, is left out.
        assert_eq!(chosen_bank(&size, b"Unknown").map(|b| b.kind), Some(String::new()));
        assert_eq!(chosen_bank(&size, b"{red}").map(|b| b.kind), Some(String::new()));
        assert_eq!((chosen_bank(&[0; 8], b"LPDDR5"), chosen_bank(&[], b"LPDDR5"), chosen_bank(&[1, 2], b"")), (None, None, None));
    }

    #[test]
    fn an_intel_macs_memory_comes_slot_by_slot() {
        // Two 16 GB DIMMs of four slots, high word first.
        let slot = |gb: u64| {
            let bytes = gb << 30;
            [((bytes >> 32) as u32).to_le_bytes(), (bytes as u32).to_le_bytes()].concat()
        };
        let reg = [slot(16), slot(0), slot(16), slot(0)].concat();
        let banks = memory_banks(&reg, b"DDR4\0empty\0DDR4\0empty\0", b"2667 MHz\0empty\x002667 MHz\0empty\0");
        assert_eq!(banks.iter().map(|b| (b.megabytes, b.kind.as_str(), b.speed)).collect::<Vec<_>>(), [(16384, "DDR4", 2667), (0, "empty", 0), (16384, "DDR4", 2667), (0, "empty", 0)]);
        // Speeds that don't go slot by slot are dropped; one kind for all
        // goes on each.
        let banks = memory_banks(&reg, b"DDR4\0", b"2667 MHz\0");
        assert!(banks.iter().all(|b| b.speed == 0 && b.kind == "DDR4"), "{banks:?}");
        // Kinds that differ, not slot by slot: none.
        let banks = memory_banks(&reg, b"DDR4\0DDR3\0", b"");
        assert!(banks.iter().all(|b| b.kind.is_empty()), "{banks:?}");
        assert!(memory_banks(&[1, 2, 3], b"DDR4", b"1 MHz").is_empty());
        assert_eq!(leading_number(" 99999999999 MHz"), 0, "no such speed");
    }

    #[test]
    fn a_device_tree_location_reads_as_device_and_function() {
        assert_eq!((dev_fn("0"), dev_fn("0,1"), dev_fn("1C,4"), dev_fn("1f,7")), (Some((0, 0)), Some((0, 1)), Some((0x1c, 4)), Some((0x1f, 7))));
        for bad in ["", ",", "20", "0,8", "0,1,2", "x", "-1", "+1", "0, 1"] {
            assert_eq!(dev_fn(bad), None, "{bad:?}");
        }
    }
}
