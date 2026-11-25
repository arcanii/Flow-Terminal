//! What the Workstation's PROM probes: the computer's memory banks and the
//! slots on its buses, as plain data, with pure parsers that turn what each
//! OS reads into it (`apple`, `linux`, `windows`). The parsers live in core,
//! not the OS crates, so every one is built and tested on every OS; the OS
//! crates only fetch the bytes (and Linux's files are read here too, from a
//! root folder a test can make up).
//!
//! Only what names no one is kept: sizes, kinds and speeds of memory, bus
//! addresses, and driver or device-tree names. No parser reads a table's
//! strings, or any key that could hold a serial number, an address or a
//! name someone gave, and what is kept still passes [`shown_name`] and the
//! like before it reaches the screen.

pub mod apple;
pub mod linux;
pub mod windows;

use std::collections::BTreeMap;

/// The memory banks and the buses' slots, as the PROM probes them. Empty
/// lists are what the OS didn't say, and the PROM probes made-up ones.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hardware {
    /// Each bank the firmware lists, by its slot number.
    pub banks: Vec<Bank>,
    /// Each slot on the buses, in order.
    pub probes: Vec<Probe>,
}

/// One memory bank.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bank {
    /// Its size; 0 for an empty slot.
    pub megabytes: u64,
    /// Its kind ("DDR4", "LPDDR5"); empty when unknown.
    pub kind: String,
    /// Its speed in MT/s (MHz on old tables); 0 when unknown.
    pub speed: u32,
}

/// One slot on a bus, as one of the PROM's lines probes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    /// The bus it's on: "/iommu@fed91000/pci@0,0", or a Mac's
    /// "/arm-io@10F00000/apcie0@80000000".
    pub path: String,
    /// Where on the bus: device and function in hex ("1c,4").
    pub at: String,
    /// What's in it, a name each (empty where there's none to give); none at
    /// all is "Nothing there".
    pub names: Vec<String>,
}

/// A PCI function's slots, (device, function) each, from its root bus
/// down: the first is its slot on the root bus, the last its own.
pub type Chain = Vec<(u8, u8)>;

/// One PCI function, as an OS crate finds it, for [`pci_probes`] to put in
/// its slot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PciFunction {
    /// The root bus it hangs from ([`pci_root`], or a Mac's device-tree path).
    pub root: String,
    /// The IOMMU that translates it ("fed91000", or AMD's "0,2"); empty for
    /// none.
    pub iommu: String,
    /// (device, function) from the root bus down: the first is its slot on
    /// the root bus, the last its own.
    pub chain: Chain,
    /// Its class code, 0xCCSSPP.
    pub class: u32,
    /// Its driver's or device-tree name; empty for none.
    pub name: String,
    pub vendor: u16,
    pub device: u16,
}

/// The most PCI functions an OS reader takes, which bounds its time on big
/// servers.
pub const MOST_FUNCTIONS: usize = 512;

/// The largest bank a table can sensibly hold, 16 TB: past it the table has
/// gone wrong.
const LARGEST_BANK: u64 = 1 << 24;

/// How many times the memory the system sees the banks may hold: an APU
/// given 96 GB of 128 for its graphics has its system see a quarter, and
/// rounding may put that a little under, so twice that.
const MORE_THAN_SEEN: u64 = 8;

/// The fastest memory a table can sensibly list, in MT/s: past it the speed
/// is a table gone wrong, and none is shown.
const FASTEST: u32 = 20000;

impl Hardware {
    /// The banks' memory in all.
    pub fn megabytes(&self) -> u64 {
        self.banks.iter().fold(0, |sum, b| sum.saturating_add(b.megabytes))
    }

    /// Whether the banks add up to the memory the system reports
    /// (`memory_gb`, rounded; 0 if unknown): some bank filled, none absurd,
    /// short of the total by no more than a gigabyte and an eighth, and no
    /// more than `MORE_THAN_SEEN` times it. Banks are what's installed, and
    /// the system may see less: firmware keeps some back for itself, and an
    /// APU's firmware some for its graphics (an APU with 64 GB, 48 of them
    /// given to its graphics, has Linux see 16). Banks that don't agree
    /// come from a table gone wrong (or decoded wrong, the Intel Mac's
    /// guess included, which gives nothing or too much), and the PROM probes
    /// made-up ones instead.
    pub fn banks_agree(&self, memory_gb: u64) -> bool {
        if !self.banks.iter().any(|b| b.megabytes > 0) || self.banks.iter().any(|b| b.megabytes > LARGEST_BANK) {
            return false;
        }
        let (banks, seen) = (self.megabytes(), memory_gb.saturating_mul(1024));
        memory_gb == 0 || (banks >= seen.saturating_sub(1024 + seen / 8) && banks <= seen.saturating_mul(MORE_THAN_SEEN))
    }
}

/// A PCI root bus as Open Firmware names it: "/pci@0,0". A segment may pass
/// 16 bits (Linux numbers Intel VMD's from 0x10000).
pub fn pci_root(segment: u32, bus: u8) -> String {
    format!("/pci@{segment:x},{bus:x}")
}

/// A PCI-to-PCI bridge's class (a root port, or a switch's port).
const BRIDGE: u32 = 0x0604;
/// A host bridge's class: the bus itself.
const HOST_BRIDGE: u32 = 0x0600;

/// PCI functions put in their slots, a probe each, as the PROM probed them:
/// each root port with what's below it, and each device on a root bus with
/// all its functions on one line. Host bridges are the bus itself, as Sun
/// didn't list its iommu or sbus nodes, so they're left out; a root port
/// with nothing below it is the real "Nothing there".
pub fn pci_probes(mut functions: Vec<PciFunction>) -> Vec<Probe> {
    functions.retain(|f| !f.chain.is_empty() && f.class >> 8 != HOST_BRIDGE);
    functions.sort_by(|a, b| (&a.root, &a.chain).cmp(&(&b.root, &b.chain)));
    // Each slot by (root, device, function): a root port's function, or
    // none for a device on the root bus, whose functions share its line.
    let mut slots: BTreeMap<(&str, u8, Option<u8>), Vec<&PciFunction>> = BTreeMap::new();
    for f in &functions {
        let (dev, func) = f.chain[0];
        let bridge = f.class >> 8 == BRIDGE;
        let slot = if f.chain.len() > 1 || bridge { (f.root.as_str(), dev, Some(func)) } else { (f.root.as_str(), dev, None) };
        slots.entry(slot).or_default().push(f);
    }
    slots
        .into_iter()
        .map(|((root, dev, func), members)| {
            // A device on the root bus is at its lowest function, which
            // sorts first.
            let func = func.unwrap_or(members[0].chain[0].1);
            let names = members.iter().filter(|f| f.class >> 8 != BRIDGE).map(|f| function_name(f)).collect();
            let path = match members[0].iommu.as_str() {
                "" => root.to_string(),
                iommu => format!("/iommu@{iommu}{root}"),
            };
            Probe { path, at: format!("{dev:x},{func:x}"), names }
        })
        .collect()
}

/// A function's name on the PROM's line: its own where it could name no
/// one, else Open Firmware's for a device without one ("pci8086,3e30"), else
/// empty, which still counts as something in the slot.
fn function_name(f: &PciFunction) -> String {
    if shown_name(&f.name) {
        f.name.clone()
    } else if f.vendor != 0 && f.vendor != 0xffff {
        format!("pci{:x},{:x}", f.vendor, f.device)
    } else {
        String::new()
    }
}

/// Whether a driver's or device's name may be shown: 1 to 31 letters,
/// digits and `,._+-`, and not what looks like a serial number (8 or more
/// letters and digits with a digit among them, all hex digits or none in
/// lower case). So no name can be a start screen's `{token}`, a color, an
/// escape, a MAC address, a UUID or "Alex's iPhone".
pub fn shown_name(name: &str) -> bool {
    let allowed = |c: char| c.is_ascii_alphanumeric() || ",._+-".contains(c);
    if name.is_empty() || name.len() > 31 || !name.chars().all(allowed) {
        return false;
    }
    let serial = name.len() >= 8
        && name.chars().all(|c| c.is_ascii_alphanumeric())
        && name.chars().any(|c| c.is_ascii_digit())
        && (name.chars().all(|c| c.is_ascii_hexdigit()) || !name.chars().any(|c| c.is_ascii_lowercase()));
    !serial
}

/// Whether a memory kind may be shown: 1 to 10 letters, digits and `-`,
/// and not a word that says nothing (RAM, Other, Unknown and the like).
pub fn shown_kind(kind: &str) -> bool {
    const SAY_NOTHING: [&str; 6] = ["RAM", "DRAM", "Other", "Unknown", "Undefined", "Reserved"];
    !kind.is_empty()
        && kind.len() <= 10
        && kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !SAY_NOTHING.iter().any(|w| w.eq_ignore_ascii_case(kind))
}

/// Whether a bus's path may be shown: a `/`, then up to 120 letters, digits
/// and `,._+@/-` in all.
pub fn shown_path(path: &str) -> bool {
    path.starts_with('/') && path.len() <= 120 && path.chars().all(|c| c.is_ascii_alphanumeric() || ",._+@/-".contains(c))
}

/// Whether where on a bus may be shown: 1 to 9 hex digits (lower case) and
/// commas.
pub fn shown_at(at: &str) -> bool {
    !at.is_empty() && at.len() <= 9 && at.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c) || c == ',')
}

/// A hex number as files and tables write it: "0x060400\n", "fed91000".
pub fn hex(text: &str) -> Option<u64> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).unwrap_or(text);
    u64::from_str_radix(digits, 16).ok().filter(|_| !digits.starts_with('+'))
}

/// A little-endian u32 from the first four bytes; 0 when there are fewer.
pub fn le_u32(bytes: &[u8]) -> u32 {
    bytes.first_chunk::<4>().map_or(0, |b| u32::from_le_bytes(*b))
}

/// A byte written in hex digits alone ("1C", "f"), no sign or space.
fn hex_byte(text: &str) -> Option<u8> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(text, 16).ok()
}

/// A speed as the PROM shows it: none past what memory runs at.
fn speed(mts: u64) -> u32 {
    if mts > FASTEST as u64 {
        0
    } else {
        mts as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(root: &str, chain: &[(u8, u8)], class: u32, name: &str) -> PciFunction {
        PciFunction { root: root.into(), chain: chain.to_vec(), class, name: name.into(), vendor: 0x106b, device: 0x100c, ..Default::default() }
    }

    fn probe(path: &str, at: &str, names: &[&str]) -> Probe {
        Probe { path: path.into(), at: at.into(), names: names.iter().map(|n| n.to_string()).collect() }
    }

    /// The PCI functions of an Apple silicon Mac as its device tree has them.
    fn mac_functions() -> Vec<PciFunction> {
        let apcie = "/arm-io@10F00000/apcie0@80000000";
        let apciec = |i: u8| format!("/arm-io@10F00000/apciec{i}@30000000");
        let mut functions = vec![
            function(apcie, &[(0, 0)], 0x060400, "pci-bridge0"),
            function(apcie, &[(0, 0), (0, 0)], 0x028000, "wlan"),
            function(apcie, &[(0, 0), (0, 1)], 0x028000, "bluetooth-pcie"),
            function(apcie, &[(1, 0)], 0x060400, "pci-bridge1"),
            function(apcie, &[(1, 0), (0, 0)], 0x010802, "pcie-sdreader"),
        ];
        // Thunderbolt's bridges, with nothing plugged in.
        for i in [2, 0, 1] {
            functions.push(function(&apciec(i), &[(0, 0)], 0x060400, &format!("pcic{i}-bridge")));
        }
        functions
    }

    #[test]
    fn pci_functions_fall_into_slots_as_the_prom_probed_them() {
        let apcie = "/arm-io@10F00000/apcie0@80000000";
        assert_eq!(
            pci_probes(mac_functions()),
            [
                probe(apcie, "0,0", &["wlan", "bluetooth-pcie"]),
                probe(apcie, "1,0", &["pcie-sdreader"]),
                probe("/arm-io@10F00000/apciec0@30000000", "0,0", &[]),
                probe("/arm-io@10F00000/apciec1@30000000", "0,0", &[]),
                probe("/arm-io@10F00000/apciec2@30000000", "0,0", &[]),
            ]
        );
        // An Intel PC: its host bridge left out, the graphics on the root
        // bus under an IOMMU of its own, the chipset's functions on one
        // line, an empty root port, and an NVMe drive behind a switch.
        let root = pci_root(0, 0);
        let intel = |chain: &[(u8, u8)], class: u32, name: &str, iommu: &str| PciFunction { iommu: iommu.into(), vendor: 0x8086, device: 0x3e30, ..function(&root, chain, class, name) };
        let functions = vec![
            intel(&[(0x1f, 5)], 0x0c8000, "intel-spi", "fed91000"),
            intel(&[(0, 0)], 0x060000, "skl_uncore", "fed91000"),
            intel(&[(2, 0)], 0x030000, "i915", "fed90000"),
            intel(&[(0x1f, 0)], 0x060100, "", "fed91000"),
            intel(&[(0x1f, 3)], 0x040300, "snd_hda_intel", "fed91000"),
            intel(&[(0x1f, 4)], 0x0c0500, "i801_smbus", "fed91000"),
            intel(&[(0x1c, 4)], 0x060400, "pcieport", "fed91000"),
            intel(&[(0x1d, 0)], 0x060400, "pcieport", "fed91000"),
            intel(&[(0x1d, 0), (0, 0)], 0x060400, "pcieport", "fed91000"),
            intel(&[(0x1d, 0), (0, 0), (1, 0)], 0x060400, "pcieport", "fed91000"),
            intel(&[(0x1d, 0), (0, 0), (1, 0), (0, 0)], 0x010802, "nvme", "fed91000"),
        ];
        let under = |iommu: &str| format!("/iommu@{iommu}/pci@0,0");
        assert_eq!(
            pci_probes(functions),
            [
                probe(&under("fed90000"), "2,0", &["i915"]),
                probe(&under("fed91000"), "1c,4", &[]),
                probe(&under("fed91000"), "1d,0", &["nvme"]),
                probe(&under("fed91000"), "1f,0", &["pci8086,3e30", "snd_hda_intel", "i801_smbus", "intel-spi"]),
            ]
        );
        // Nothing to go by: no slot, and a nameless device is still there.
        assert!(pci_probes(vec![function("/pci@0,0", &[], 0x020000, "e1000e")]).is_empty());
        let nameless = PciFunction { vendor: 0xffff, ..function("/pci@0,0", &[(3, 0)], 0x020000, "") };
        assert_eq!(pci_probes(vec![nameless]), [probe("/pci@0,0", "3,0", &[""])]);
        assert_eq!(pci_root(0x10, 0xe1), "/pci@10,e1");
    }

    #[test]
    fn a_name_is_shown_only_when_it_could_name_no_one() {
        for name in ["wlan", "snd_hda_intel", "SUNW,tcx", "pci8086,a706", "XHC1", "nvlddmkm", "pcie-sdreader", "e1i68x64", "netwtw10"] {
            assert!(shown_name(name), "{name}");
        }
        let uuid = "123e4567-e89b-12d3-a456-426614174000";
        for name in ["Alex's iPhone", "My Mac", "00:11:22:33:44:55", uuid, "C02XL0GHJGH5", "0123abcd", "\x1b[2J", "{red}", "", &"a".repeat(32)] {
            assert!(!shown_name(name), "{name:?}");
        }
        for kind in ["DDR4", "LPDDR5", "LPDDR5X", "DDR3"] {
            assert!(shown_kind(kind), "{kind}");
        }
        for kind in ["<OUT OF SPEC>", "RAM", "ram", "Other", "Unknown", "", "DDR4 SDRAM", "DDR4-{red}"] {
            assert!(!shown_kind(kind), "{kind:?}");
        }
        assert!(shown_path("/arm-io@10F00000/apcie0@80000000") && shown_path("/iommu@0,2/pci@0,0"));
        for path in ["/pci@0,0 x", "/pci\x1b[2J", "pci@0,0", "/pci@{red}", "/a:b", &format!("/{}", "a".repeat(120))] {
            assert!(!shown_path(path), "{path:?}");
        }
        assert!(shown_at("1c,4") && shown_at("0,0"));
        assert!(!shown_at("") && !shown_at("1C,4") && !shown_at("1234567890") && !shown_at("0 0"));
        assert_eq!((hex("0x060400\n"), hex("fed91000"), hex(" 0X10 "), hex(""), hex("+1"), hex("0xg")), (Some(0x060400), Some(0xfed91000), Some(16), None, None, None));
        assert_eq!((le_u32(&[0, 4, 6, 0]), le_u32(&[1, 2, 3]), le_u32(&[0x6b, 0x10, 0, 0, 9])), (0x060400, 0, 0x106b));
    }

    #[test]
    fn banks_agree_with_the_memory_the_system_reports() {
        let banks = |mb: &[u64]| Hardware { banks: mb.iter().map(|&megabytes| Bank { megabytes, ..Default::default() }).collect(), probes: Vec::new() };
        // Firmware keeps some back, and the system rounds.
        assert!(banks(&[8192, 8192, 0, 0]).banks_agree(16));
        assert!(banks(&[16384]).banks_agree(17) && banks(&[16384]).banks_agree(15));
        assert_eq!(banks(&[8192, 8192, 0]).megabytes(), 16384);
        // An APU's graphics given most of it: 64 GB with 48 GB for its
        // graphics, which Linux sees as 16 GB (or a little under), and a 16
        // GB handheld with 4 GB for its graphics.
        let apu = banks(&[16384; 4]);
        assert!(apu.banks_agree(16) && apu.banks_agree(15) && apu.banks_agree(64));
        assert!(banks(&[8192, 8192]).banks_agree(12));
        // Way off: a table gone wrong, or decoded wrong (the Intel Mac's
        // words the wrong way round give nothing or too much).
        assert!(!banks(&[8192]).banks_agree(64) && !banks(&[65536 * 9]).banks_agree(64) && !banks(&[0, 2]).banks_agree(16));
        assert!(banks(&[65536 * 8]).banks_agree(64) && !banks(&[16384 * 9]).banks_agree(16));
        // Nothing filled, or nothing at all.
        assert!(!banks(&[0, 0]).banks_agree(16) && !Hardware::default().banks_agree(16));
        // The system's total unknown: any sane banks do.
        assert!(banks(&[4096]).banks_agree(0));
        // An absurd bank, whatever the total.
        assert!(!banks(&[1 << 25]).banks_agree(0) && !banks(&[u64::MAX, 1]).banks_agree(16));
    }

    #[test]
    fn broken_tables_give_nothing_and_never_panic() {
        // Truncated, empty and random bytes and text, into every parser.
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut buffers: Vec<Vec<u8>> = vec![Vec::new(), vec![0], vec![0xff; 3], vec![0; 9]];
        for len in [4, 8, 16, 47, 48, 49, 64, 100, 300, 1000] {
            for _ in 0..40 {
                buffers.push((0..len).map(|_| next() as u8).collect());
            }
        }
        // Tables with real signatures and headers over random bodies.
        for sig in [b"DMAR", b"IVRS"] {
            for _ in 0..200 {
                let len = 48 + (next() % 200) as usize;
                let mut t: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                t[..4].copy_from_slice(sig);
                t[4..8].copy_from_slice(&((next() % 400) as u32).to_le_bytes());
                buffers.push(t);
            }
        }
        for _ in 0..200 {
            let len = 8 + (next() % 300) as usize;
            let mut raw: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            raw[4..8].copy_from_slice(&((next() % 400) as u32).to_le_bytes());
            buffers.push(raw);
        }
        for b in &buffers {
            let text = String::from_utf8_lossy(b);
            let _ = windows::smbios_banks(b);
            let units = windows::iommu_units(b);
            let _ = windows::iommu_over(&units, 0, 0, &[(0, 0)]);
            let _ = windows::location_chain(&text);
            let wide: Vec<u16> = b.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
            let _ = windows::multi_sz(&wide);
            let _ = windows::pci_ids(&[text.to_string()], &[text.to_string()]);
            let _ = apple::chosen_bank(b, b);
            let _ = apple::memory_banks(b, b, b);
            let _ = apple::dev_fn(&text);
            let _ = (apple::iommu(b, b, &Default::default()), apple::dart_address(&text));
            let _ = linux::udev_banks(&text);
            let _ = linux::device_tree_megabytes(b, b, b);
            let _ = linux::device_tree_megabytes(b, &[0, 0, 0, 2], &[0, 0, 0, 2]);
            let _ = linux::sysfs_chain(&text);
            let _ = linux::iommu_unit(&text, Some(&text), Some(&text));
            let _ = hex(&text);
            let _ = le_u32(b);
        }
        // Text shaped like what's read, with its numbers broken.
        for text in ["E:MEMORY_DEVICE_999999999999_SIZE=1", "E:MEMORY_DEVICE__SIZE=", "E:MEMORY_DEVICE_0_SIZE=-5", "pci0000:00/0000:00:zz.9", "pciffffff:00/x", "PCIROOT(100)#PCI(0000)", "PCIROOT(0)#PCI(FFFF)", "PCIROOT(0)#PCI(1C0)", "1F,8", "20,0", ",", "smmu3.0x", "/iommu/"] {
            let _ = (linux::udev_banks(text), linux::sysfs_chain(text), windows::location_chain(text), apple::dev_fn(text), linux::iommu_unit(text, None, Some(text)));
        }
        assert!(linux::udev_banks("E:MEMORY_DEVICE_999_SIZE=17179869184").is_empty(), "past 255 devices");
    }
}
