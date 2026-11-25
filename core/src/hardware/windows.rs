//! What Windows hands over about the memory and buses, read: the firmware's
//! SMBIOS table (`GetSystemFirmwareTable`'s RSMB), its ACPI tables that say
//! which IOMMU is over which slot (Intel's DMAR, AMD's IVRS), and the
//! configuration manager's strings for each PCI function (its location
//! path, hardware and compatible IDs, and service). SMBIOS's strings, where
//! serial numbers, asset tags and part numbers sit, are passed over and
//! never decoded.

use super::{shown_kind, speed, Bank, Chain};

/// The memory devices in SMBIOS's table, from `RawSMBIOSData`: eight bytes
/// (the calling method, the version, and the table's length), then the
/// table. Each memory device (type 17) of an array of system memory (type
/// 16 with Use 3) gives a bank: its size (0 for an empty slot; one of
/// unknown size is left out), kind and speed, configured first.
pub fn smbios_banks(raw: &[u8]) -> Vec<Bank> {
    let Some(length) = raw.get(4..8).map(super::le_u32) else { return Vec::new() };
    let t = &raw[8..raw.len().min(8usize.saturating_add(length as usize))];
    let u16_at = |at: usize| t.get(at..at + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |at: usize| t.get(at..at + 4).map_or(0, super::le_u32);
    // Arrays by handle, with their Use; devices with their array.
    let mut arrays: Vec<(u16, u8)> = Vec::new();
    let mut devices: Vec<(u16, Option<Bank>)> = Vec::new();
    let mut p = 0;
    while p + 4 <= t.len() {
        let (kind, len) = (t[p], t[p + 1] as usize);
        if len < 4 || p + len > t.len() {
            break;
        }
        // The strings after the structure end at the first double NUL.
        let Some(strings) = t[p + len..].windows(2).position(|w| w == [0, 0]) else { break };
        let next = p + len + strings + 2;
        match kind {
            127 => break,
            16 if len >= 0x0F => arrays.push((u16_at(p + 2), t[p + 5])),
            17 if len >= 0x15 => {
                let at = |offset: usize, need: usize| if len >= need { Some(p + offset) } else { None };
                let size = u16_at(p + 0x0C);
                let megabytes = match size {
                    0 => Some(0),
                    0xFFFF => None,
                    0x7FFF => at(0x1C, 0x20).map(|o| (u32_at(o) & 0x7FFF_FFFF) as u64),
                    s if s & 0x8000 != 0 => Some((s & 0x7FFF) as u64 / 1024),
                    s => Some(s as u64),
                };
                let configured = at(0x20, 0x22).map_or(0, u16_at);
                let configured = match configured {
                    0xFFFF => at(0x58, 0x5C).map_or(0, |o| u32_at(o) & 0x7FFF_FFFF),
                    c => c as u32,
                };
                let rated = at(0x15, 0x17).map_or(0, u16_at);
                let rated = match rated {
                    0xFFFF => at(0x54, 0x5C).map_or(0, |o| u32_at(o) & 0x7FFF_FFFF),
                    r => r as u32,
                };
                let mts = if configured != 0 { configured } else { rated };
                let kind = memory_kind(t[p + 0x12]);
                devices.push((u16_at(p + 4), megabytes.map(|megabytes| Bank { megabytes, kind: kind.into(), speed: speed(mts as u64) })));
            }
            _ => {}
        }
        p = next;
    }
    devices
        .into_iter()
        .filter(|(array, _)| arrays.iter().find(|(handle, _)| handle == array).is_none_or(|(_, used)| *used == 0x03))
        .filter_map(|(_, bank)| bank)
        .collect()
}

/// SMBIOS's memory type byte as the kind's name; empty for the others.
fn memory_kind(byte: u8) -> &'static str {
    let kind = match byte {
        0x0F => "SDRAM",
        0x12 => "DDR",
        0x13 => "DDR2",
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x20 => "HBM",
        0x21 => "HBM2",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        0x24 => "HBM3",
        _ => "",
    };
    debug_assert!(kind.is_empty() || shown_kind(kind));
    kind
}

/// An IOMMU unit from an ACPI table: its PCI segment, and either the slots
/// on root buses it translates (`slots`, as (bus, device, function)) or,
/// with `all`, every slot of the segment (on its own bus, where it says).
#[derive(Debug, Clone, PartialEq)]
pub struct Iommu {
    pub segment: u16,
    pub bus: Option<u8>,
    /// How the PROM names it: Intel's by its registers' address
    /// ("fed91000"), AMD's by its own PCI function ("0,2").
    pub unit: String,
    pub all: bool,
    pub slots: Vec<(u8, u8, u8)>,
}

/// The IOMMU units an ACPI table lists, by its signature: DMAR's remapping
/// units (DRHD) or IVRS's hardware definitions (IVHD); nothing from any
/// other table.
pub fn iommu_units(table: &[u8]) -> Vec<Iommu> {
    let Some(length) = table.get(4..8).map(super::le_u32) else { return Vec::new() };
    let t = &table[..table.len().min(length as usize)];
    let u16_at = |at: usize| t.get(at..at + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    let mut units = Vec::new();
    let mut p = 48;
    match table.get(..4) {
        Some(b"DMAR") => {
            while p + 4 <= t.len() {
                let (kind, len) = (u16_at(p), u16_at(p + 2) as usize);
                if len < 4 || p + len > t.len() {
                    break;
                }
                if kind == 0 && len >= 16 {
                    let base = t.get(p + 8..p + 16).map_or(0, |b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
                    let mut slots = Vec::new();
                    // Its device scopes: a PCI endpoint's or a bridge's first
                    // step from its start bus is its slot on that bus.
                    let mut q = p + 16;
                    while q + 2 <= p + len {
                        let (scope, scope_len) = (t[q], t[q + 1] as usize);
                        if scope_len < 6 || q + scope_len > p + len {
                            break;
                        }
                        if (scope == 1 || scope == 2) && scope_len >= 8 {
                            slots.push((t[q + 5], t[q + 6], t[q + 7]));
                        }
                        q += scope_len;
                    }
                    units.push(Iommu { segment: u16_at(p + 6), bus: None, unit: format!("{base:x}"), all: t[p + 4] & 1 != 0, slots });
                }
                p += len;
            }
        }
        Some(b"IVRS") => {
            let mut seen: Vec<(u16, u16)> = Vec::new();
            while p + 4 <= t.len() {
                let (kind, len) = (t[p], u16_at(p + 2) as usize);
                if len < 4 || p + len > t.len() {
                    break;
                }
                if matches!(kind, 0x10 | 0x11 | 0x40) && len >= 24 {
                    let (id, segment) = (u16_at(p + 4), u16_at(p + 16));
                    // 0x10 and 0x11 describe the same unit.
                    if !seen.contains(&(segment, id)) {
                        seen.push((segment, id));
                        units.push(Iommu { segment, bus: Some((id >> 8) as u8), unit: format!("{:x},{:x}", (id >> 3) & 0x1f, id & 7), all: true, slots: Vec::new() });
                    }
                }
                p += len;
            }
        }
        _ => {}
    }
    units
}

/// The IOMMU over a PCI function on `segment`'s root `bus` with slots
/// `chain`, as [`Iommu::unit`] names it: the unit that lists its slot, else
/// one over every slot of that bus, else one over every slot that names no
/// bus, else any over every slot; empty for none.
pub fn iommu_over(units: &[Iommu], segment: u16, bus: u8, chain: &[(u8, u8)]) -> String {
    let Some(&(dev, func)) = chain.first() else { return String::new() };
    let on_segment = || units.iter().filter(|u| u.segment == segment);
    let found = on_segment()
        .find(|u| u.slots.contains(&(bus, dev, func)))
        .or_else(|| on_segment().find(|u| u.all && u.bus == Some(bus)))
        .or_else(|| on_segment().find(|u| u.all && u.bus.is_none()))
        .or_else(|| on_segment().find(|u| u.all));
    found.map(|u| u.unit.clone()).unwrap_or_default()
}

/// A PCI function's root bus and slots from its location path,
/// "PCIROOT(0)#PCI(1C04)#PCI(0000)": the root's number, then each slot's
/// device and function. None for any other kind of path ("ACPI(_SB_)#...").
pub fn location_chain(path: &str) -> Option<(u8, Chain)> {
    let mut parts = path.split('#');
    let root = parts.next()?.strip_prefix("PCIROOT(")?.strip_suffix(')')?;
    let root = hex_digits(root, 1..=2)? as u8;
    let mut chain = Vec::new();
    for part in parts {
        let slot = hex_digits(part.strip_prefix("PCI(")?.strip_suffix(')')?, 4..=4)?;
        let (dev, func) = ((slot >> 8) as u8, slot as u8);
        if dev > 0x1f || func > 7 {
            return None;
        }
        chain.push((dev, func));
    }
    (!chain.is_empty()).then_some((root, chain))
}

/// Hex digits alone, so many of them.
fn hex_digits(text: &str, count: std::ops::RangeInclusive<usize>) -> Option<u32> {
    (count.contains(&text.len()) && text.chars().all(|c| c.is_ascii_hexdigit())).then(|| u32::from_str_radix(text, 16).ok()).flatten()
}

/// A PCI function's vendor, device and class from its IDs: VEN_ and DEV_
/// in the first hardware ID with both ("PCI\VEN_8086&DEV_A706&..."), and
/// CC_ in a compatible ID ("PCI\CC_060400", or "PCI\CC_0604" without its
/// interface).
pub fn pci_ids(hardware_ids: &[String], compatible_ids: &[String]) -> (u16, u16, u32) {
    let after = |id: &str, key: &str| -> Option<String> {
        let at = id.to_ascii_uppercase().find(key)? + key.len();
        Some(id[at..].chars().take_while(char::is_ascii_hexdigit).collect())
    };
    let four = |digits: String| (digits.len() >= 4).then(|| u16::from_str_radix(&digits[..4], 16).ok()).flatten();
    let (vendor, device) = hardware_ids.iter().find_map(|id| Some((four(after(id, "VEN_")?)?, four(after(id, "DEV_")?)?))).unwrap_or((0, 0));
    let classes: Vec<String> = compatible_ids.iter().filter_map(|id| after(id, "CC_")).collect();
    let class = classes
        .iter()
        .find(|c| c.len() == 6)
        .and_then(|c| u32::from_str_radix(c, 16).ok())
        .or_else(|| classes.iter().find(|c| c.len() == 4).and_then(|c| u32::from_str_radix(c, 16).ok()).map(|c| c << 8))
        .unwrap_or(0);
    (vendor, device, class)
}

/// Windows' list of strings: each ended by a NUL, the list by an empty one.
pub fn multi_sz(wide: &[u16]) -> Vec<String> {
    wide.split(|&c| c == 0).take_while(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One SMBIOS structure: its type, its formatted bytes after the
    /// header (handle first), and its strings.
    fn structure(kind: u8, handle: u16, body: &[u8], strings: &[&str]) -> Vec<u8> {
        let mut s = vec![kind, (4 + body.len()) as u8];
        s.extend(handle.to_le_bytes());
        s.extend(body);
        for text in strings {
            s.extend(text.as_bytes());
            s.push(0);
        }
        if strings.is_empty() {
            s.push(0);
        }
        s.push(0);
        s
    }

    /// A memory device of array `array`, 0x5C bytes long.
    fn device(array: u16, size: u16, kind: u8, rated: u16, extended_size: u32, configured: u16, extended: (u32, u32)) -> Vec<u8> {
        let mut b = vec![0u8; 0x5C - 4];
        let mut put = |at: usize, bytes: &[u8]| b[at - 4..at - 4 + bytes.len()].copy_from_slice(bytes);
        put(0x04, &array.to_le_bytes());
        put(0x0C, &size.to_le_bytes());
        put(0x12, &[kind]);
        put(0x15, &rated.to_le_bytes());
        put(0x1C, &extended_size.to_le_bytes());
        put(0x20, &configured.to_le_bytes());
        put(0x54, &extended.0.to_le_bytes());
        put(0x58, &extended.1.to_le_bytes());
        // Strings the device names (its locator, maker, serial, asset tag,
        // part number) are numbered in its bytes; their numbers don't matter.
        put(0x17, &[1, 2, 3, 4, 5]);
        b
    }

    fn raw(structures: &[Vec<u8>]) -> Vec<u8> {
        let table = structures.concat();
        let mut raw = vec![0, 3, 4, 0];
        raw.extend((table.len() as u32).to_le_bytes());
        raw.extend(table);
        raw
    }

    #[test]
    fn smbios_gives_the_memory_devices_and_never_reads_their_strings() {
        // Arrays: system memory (Use 3) and video memory (Use 4).
        let array = |used: u8| [vec![0x03, used, 0x06], vec![0; 0x0F - 7]].concat();
        let serial = ["DIMM A1", "Maker", "SERIAL-XYZ", "ASSET-TAG", "PART-9"];
        let table = raw(&[
            structure(0, 0, &[1, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], &["BIOS maker"]),
            structure(16, 0x1000, &array(3), &[]),
            structure(16, 0x2000, &array(4), &[]),
            structure(17, 0x1100, &device(0x1000, 16384, 0x22, 4800, 0, 5600, (0, 0)), &serial),
            structure(17, 0x1101, &device(0x1000, 0, 0x02, 0, 0, 0, (0, 0)), &["DIMM A2"]),
            structure(17, 0x1102, &device(0x1000, 0x7FFF, 0x1A, 3200, 65536, 0, (0, 0)), &serial),
            structure(17, 0x1103, &device(0x1000, 0x8000 | 2048, 0x18, 0, 0, 0, (0, 0)), &serial),
            structure(17, 0x2100, &device(0x2000, 8192, 0x22, 0, 0, 0, (0, 0)), &serial),
            structure(17, 0x1104, &device(0x1000, 4096, 0x23, 0xFFFF, 0, 0xFFFF, (8533, 7500)), &serial),
            structure(17, 0x1105, &device(0x1000, 4096, 0x01, 0xFFFF, 0, 0, (6400, 0)), &serial),
            structure(17, 0x1106, &device(0x1000, 0xFFFF, 0x1A, 0, 0, 0, (0, 0)), &serial),
            structure(17, 0x1107, &device(0x9999, 2048, 0x1A, 65000, 0, 0, (0, 0)), &serial),
            structure(127, 0x7F00, &[], &[]),
            structure(17, 0x1108, &device(0x1000, 1024, 0x1A, 0, 0, 0, (0, 0)), &serial),
        ]);
        let banks = smbios_banks(&table);
        let bank = |megabytes, kind: &str, speed| Bank { megabytes, kind: kind.into(), speed };
        assert_eq!(
            banks,
            [
                bank(16384, "DDR5", 5600),
                bank(0, "", 0),
                bank(65536, "DDR4", 3200),
                bank(2, "DDR3", 0),
                bank(4096, "LPDDR5", 7500),
                bank(4096, "", 6400),
                bank(2048, "DDR4", 0),
            ],
            "video memory, a size unknown and what's past the end left out"
        );
        assert!(!format!("{banks:?}").contains("SERIAL-XYZ"));
        // Cut short anywhere, it gives what it read before the cut.
        for cut in 0..table.len() {
            let mut short = table[..cut].to_vec();
            if short.len() >= 8 {
                short[4..8].copy_from_slice(&((cut - 8) as u32).to_le_bytes());
            }
            assert!(smbios_banks(&short).len() <= banks.len());
        }
        assert!(smbios_banks(&[]).is_empty());
    }

    /// An ACPI table's header for `signature`, its length filled in later.
    fn acpi(signature: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut t = signature.to_vec();
        t.extend(((48 + body.len()) as u32).to_le_bytes());
        t.resize(48, 0);
        t.extend(body);
        t
    }

    #[test]
    fn dmar_and_ivrs_say_which_iommu_is_over_a_slot() {
        // Intel's: one unit for the graphics at 0:2.0, one for the rest.
        let drhd = |flags: u8, base: u64, scopes: &[u8]| {
            let mut s = vec![0, 0];
            s.extend(((16 + scopes.len()) as u16).to_le_bytes());
            s.extend([flags, 0, 0, 0]);
            s.extend(base.to_le_bytes());
            s.extend(scopes);
            s
        };
        let endpoint = [1u8, 8, 0, 0, 0, 0, 2, 0];
        let ioapic = [3u8, 8, 0, 0, 2, 0xf0, 0x1f, 0];
        let dmar = acpi(b"DMAR", &[drhd(0, 0xfed9_0000, &endpoint), drhd(1, 0xfed9_1000, &ioapic)].concat());
        let units = iommu_units(&dmar);
        assert_eq!(
            units,
            [
                Iommu { segment: 0, bus: None, unit: "fed90000".into(), all: false, slots: vec![(0, 2, 0)] },
                Iommu { segment: 0, bus: None, unit: "fed91000".into(), all: true, slots: Vec::new() },
            ]
        );
        assert_eq!(iommu_over(&units, 0, 0, &[(2, 0)]), "fed90000");
        assert_eq!(iommu_over(&units, 0, 0, &[(0x1c, 4), (0, 0)]), "fed91000");
        assert_eq!(iommu_over(&units, 1, 0, &[(0x1c, 4)]), "", "another segment");
        assert_eq!(iommu_over(&units, 0, 0, &[]), "");
        // AMD's: the same unit twice (types 0x10 and 0x11), at 0:0.2.
        let ivhd = |kind: u8, len: u16, id: u16| {
            let mut b = vec![kind, 0];
            b.extend(len.to_le_bytes());
            b.extend(id.to_le_bytes());
            b.resize(len as usize, 0);
            b
        };
        let ivrs = acpi(b"IVRS", &[ivhd(0x10, 24, 0x0002), ivhd(0x11, 40, 0x0002), ivhd(0x20, 32, 0x0040)].concat());
        let units = iommu_units(&ivrs);
        assert_eq!(units, [Iommu { segment: 0, bus: Some(0), unit: "0,2".into(), all: true, slots: Vec::new() }]);
        assert_eq!(iommu_over(&units, 0, 0, &[(8, 1)]), "0,2");
        // A unit on another bus is over that bus's slots first.
        let both = [units[0].clone(), Iommu { bus: Some(0x40), unit: "0,3".into(), ..units[0].clone() }];
        assert_eq!((iommu_over(&both, 0, 0x40, &[(1, 0)]), iommu_over(&both, 0, 0, &[(1, 0)])), ("0,3".into(), "0,2".into()));
        // A table of another kind gives nothing.
        assert!(iommu_units(&acpi(b"APIC", &drhd(1, 1, &[]))).is_empty());
        assert!(iommu_units(&dmar[..30]).is_empty());
    }

    #[test]
    fn a_location_path_gives_the_root_and_the_slots_down_to_a_function() {
        assert_eq!(location_chain("PCIROOT(0)#PCI(1C04)#PCI(0000)"), Some((0, vec![(0x1c, 4), (0, 0)])));
        assert_eq!(location_chain("PCIROOT(80)#PCI(0200)"), Some((0x80, vec![(2, 0)])));
        for bad in ["ACPI(_SB_)#ACPI(PCI0)#ACPI(RP05)#PCI(0000)", "PCIROOT(0)", "PCIROOT(100)#PCI(0000)", "PCIROOT(0)#PCI(2000)", "PCIROOT(0)#PCI(0008)", "PCIROOT(0)#PCI(1C0)", "PCIROOT(0)#USBROOT(0)", "", "PCIROOT(0)#PCI(+1C0)"] {
            assert_eq!(location_chain(bad), None, "{bad}");
        }
    }

    #[test]
    fn windows_ids_give_the_vendor_the_device_and_the_class() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let hardware = ids(&["PCI\\VEN_8086&DEV_A706&SUBSYS_00000000&REV_00", "PCI\\VEN_8086&DEV_A706"]);
        let compatible = ids(&["PCI\\VEN_8086&DEV_A706&REV_00", "PCI\\VEN_8086&CC_060400", "PCI\\VEN_8086&CC_0604", "PCI\\CC_0604"]);
        assert_eq!(pci_ids(&hardware, &compatible), (0x8086, 0xa706, 0x060400));
        assert_eq!(pci_ids(&ids(&["PCI\\VEN_10EC"]), &ids(&["PCI\\CC_0200"])), (0, 0, 0x020000));
        assert_eq!(pci_ids(&[], &ids(&["PCI\\CC_02"])), (0, 0, 0));
    }

    #[test]
    fn multi_strings_split_at_their_nuls() {
        let wide: Vec<u16> = "PCI\\VEN_8086\0PCI\\CC_0604\0\0junk\0".encode_utf16().collect();
        assert_eq!(multi_sz(&wide), ["PCI\\VEN_8086", "PCI\\CC_0604"]);
        assert_eq!(multi_sz(&"stornvme\0".encode_utf16().collect::<Vec<_>>()), ["stornvme"]);
        assert!(multi_sz(&[]).is_empty() && multi_sz(&[0, 65]).is_empty());
    }
}
