//! A Linux computer's memory and buses from its files, which any user may
//! read: the memory devices systemd's `dmi_memory_id` puts in udev's
//! database (`/run/udev/data/+dmi:id`, mode 0644) or, on boards booted from
//! a device tree, its memory nodes; and the PCI functions in sysfs, each
//! read from its link's path, its class, vendor and device files and the
//! names of its driver and IOMMU. The file reading lives in core so that it
//! can be tested on any OS against a made-up tree.
//!
//! Nothing else is read: not udev's other files (disks' serials are there),
//! not a memory device's serial, asset tag or part number in that file, and
//! not a function's config space, VPD or label, the network's devices or
//! USB.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use super::{hex, pci_probes, pci_root, shown_kind, speed, Bank, Chain, Hardware, PciFunction, MOST_FUNCTIONS};

/// The memory and buses of the Linux whose root folder is `root` ("/").
pub fn read(root: &Path) -> Hardware {
    Hardware { banks: banks(root), probes: probes(root) }
}

/// The memory banks alone, which don't change while it runs.
pub fn banks(root: &Path) -> Vec<Bank> {
    let udev = fs::read_to_string(root.join("run/udev/data/+dmi:id")).map(|text| udev_banks(&text)).unwrap_or_default();
    if !udev.is_empty() {
        return udev;
    }
    // A board booted from a device tree: a bank for each memory node.
    let base = root.join("sys/firmware/devicetree/base");
    let read = |path: &Path| fs::read(path).unwrap_or_default();
    let (address_cells, size_cells) = (read(&base.join("#address-cells")), read(&base.join("#size-cells")));
    let Ok(entries) = fs::read_dir(&base) else { return Vec::new() };
    let mut nodes: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name == "memory" || name.starts_with("memory@"))
        .collect();
    nodes.sort();
    let mut banks = Vec::new();
    for node in nodes {
        let megabytes = device_tree_megabytes(&read(&base.join(&node).join("reg")), &address_cells, &size_cells);
        if megabytes > 0 {
            banks.push(Bank { megabytes, ..Default::default() });
        }
    }
    banks
}

/// The memory devices in udev's database of the firmware's tables, slot by
/// slot: each `E:MEMORY_DEVICE_<n>_<FIELD>=` line's size (bytes), presence,
/// kind and speeds. Every other field, the serial number, asset tag and
/// part number among them, is passed over and never kept.
pub fn udev_banks(text: &str) -> Vec<Bank> {
    #[derive(Default)]
    struct Slot {
        size: Option<u64>,
        absent: bool,
        kind: String,
        configured: u64,
        speed: u64,
    }
    let mut slots: BTreeMap<u32, Slot> = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.strip_prefix("E:MEMORY_DEVICE_").and_then(|rest| rest.split_once('=')) else { continue };
        let Some((n, field)) = key.split_once('_') else { continue };
        let Some(n) = n.parse::<u32>().ok().filter(|v| *v < 256 && n.chars().all(|c| c.is_ascii_digit())) else { continue };
        let number = || value.trim().parse::<u64>().ok();
        let entry = slots.entry(n);
        match field {
            "SIZE" => entry.or_default().size = number(),
            "PRESENT" => entry.or_default().absent = value.trim() == "0",
            "TYPE" => entry.or_default().kind = if shown_kind(value.trim()) { value.trim().to_string() } else { String::new() },
            "CONFIGURED_SPEED_MTS" => entry.or_default().configured = number().unwrap_or(0),
            "SPEED_MTS" => entry.or_default().speed = number().unwrap_or(0),
            _ => {}
        }
    }
    slots
        .into_values()
        .filter_map(|s| {
            let megabytes = if s.absent { 0 } else { s.size? >> 20 };
            let mts = if s.configured > 0 { s.configured } else { s.speed };
            Some(Bank { megabytes, kind: s.kind, speed: speed(mts) })
        })
        .collect()
}

/// The memory a device-tree memory node's `reg` holds, in megabytes: its
/// ranges of `#address-cells` and `#size-cells` big-endian cells (2 and 1
/// where the files are missing), the sizes added up. 0 for cells it can't
/// hold.
pub fn device_tree_megabytes(reg: &[u8], address_cells: &[u8], size_cells: &[u8]) -> u64 {
    let cells = |bytes: &[u8], missing: u32| bytes.first_chunk::<4>().map_or(missing, |b| u32::from_be_bytes(*b)) as usize;
    let (ac, sc) = (cells(address_cells, 2), cells(size_cells, 1));
    if ac > 3 || sc == 0 || sc > 2 {
        return 0;
    }
    let bytes = reg
        .chunks_exact((ac + sc) * 4)
        .map(|range| range[ac * 4..].as_chunks::<4>().0.iter().fold(0u64, |size, cell| (size << 32) | u32::from_be_bytes(*cell) as u64))
        .fold(0u64, u64::saturating_add);
    bytes >> 20
}

/// A PCI function's root bus and slots from its sysfs link
/// ("../../../devices/pci0000:00/0000:00:1c.4/0000:05:00.0"): the domain and
/// bus of the last `pci<domain>:<bus>` in it, and each `<domain>:<bus>:
/// <dev>.<fn>` after it. Only those are read, so Hyper-V's GUIDs before
/// them never are; its root's domain, which pci-hyperv numbers from one,
/// is kept as the bus address it is. A domain may pass 16 bits: Intel VMD's
/// are numbered from 0x10000, clear of the firmware's. None without a
/// function.
pub fn sysfs_chain(link: &str) -> Option<(u32, u8, Chain)> {
    let parts: Vec<&str> = link.split('/').collect();
    let (at, (domain, bus)) = parts.iter().enumerate().rev().find_map(|(i, part)| Some((i, root_bus(part)?)))?;
    let chain: Option<Vec<(u8, u8)>> = parts[at + 1..].iter().filter(|p| !p.is_empty()).map(|p| function(p)).collect();
    chain.filter(|c| !c.is_empty()).map(|c| (domain, bus, c))
}

/// "pci0000:00" (or VMD's "pci10000:e0") as its domain and bus.
fn root_bus(part: &str) -> Option<(u32, u8)> {
    let (domain, bus) = part.strip_prefix("pci")?.split_once(':')?;
    Some((hex_number(domain)?.try_into().ok()?, hex_number(bus)?.try_into().ok()?))
}

/// "0000:05:00.0" as its device and function.
fn function(part: &str) -> Option<(u8, u8)> {
    let mut fields = part.split(':');
    let (_domain, _bus, slot) = (hex_number(fields.next()?)?, hex_number(fields.next()?)?, fields.next()?);
    let (dev, func) = slot.split_once('.')?;
    let (dev, func) = (hex_number(dev)?, hex_number(func)?);
    (fields.next().is_none() && dev <= 0x1f && func <= 7).then_some((dev as u8, func as u8))
}

/// Hex digits alone, as sysfs writes its names' numbers.
fn hex_number(text: &str) -> Option<u64> {
    (!text.is_empty() && text.len() <= 8 && text.chars().all(|c| c.is_ascii_hexdigit())).then(|| u64::from_str_radix(text, 16).ok()).flatten()
}

/// The IOMMU unit named `name` in `/sys/class/iommu`, as the PROM names it:
/// an Intel unit by its registers' address (its `intel-iommu/address`
/// file), an Arm SMMU by the address its name holds ("smmu3.0x...09050000"),
/// an AMD unit by its own PCI function ("0,2", before `/iommu/` in its
/// link); else empty.
pub fn iommu_unit(name: &str, intel_address: Option<&str>, class_link: Option<&str>) -> String {
    if let Some(address) = intel_address.and_then(hex) {
        return format!("{address:x}");
    }
    if let Some(address) = name.split_once(".0x").and_then(|(_, digits)| hex(digits)) {
        return format!("{address:x}");
    }
    let amd = class_link.and_then(|link| link.split_once("/iommu/")).and_then(|(before, _)| function(before.rsplit('/').next()?));
    amd.map(|(dev, func)| format!("{dev:x},{func:x}")).unwrap_or_default()
}

/// The slots on the buses, read afresh each time.
pub fn probes(root: &Path) -> Vec<super::Probe> {
    let Ok(entries) = fs::read_dir(root.join("sys/bus/pci/devices")) else { return Vec::new() };
    let link_name = |path: &Path| fs::read_link(path).ok().and_then(|target| Some(target.file_name()?.to_string_lossy().into_owned())).unwrap_or_default();
    let mut units: HashMap<String, String> = HashMap::new();
    let mut functions = Vec::new();
    for entry in entries.flatten().take(MOST_FUNCTIONS) {
        let p = entry.path();
        let Ok(link) = fs::read_link(&p) else { continue };
        let Some((domain, bus, chain)) = sysfs_chain(&link.to_string_lossy()) else { continue };
        let number = |file: &str| fs::read_to_string(p.join(file)).ok().and_then(|text| hex(&text)).unwrap_or(0);
        let iommu = match link_name(&p.join("iommu")) {
            name if name.is_empty() => String::new(),
            name => units
                .entry(name.clone())
                .or_insert_with(|| {
                    let class = root.join("sys/class/iommu").join(&name);
                    let address = fs::read_to_string(class.join("intel-iommu/address")).ok();
                    let link = fs::read_link(&class).ok().map(|l| l.to_string_lossy().into_owned());
                    iommu_unit(&name, address.as_deref(), link.as_deref())
                })
                .clone(),
        };
        functions.push(PciFunction {
            root: pci_root(domain, bus),
            iommu,
            chain,
            class: number("class") as u32,
            name: link_name(&p.join("driver")),
            vendor: number("vendor") as u16,
            device: number("device") as u16,
        });
    }
    pci_probes(functions)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::super::Probe;
    use super::*;

    #[test]
    fn udev_gives_the_memory_slots_and_nothing_else() {
        let text = "\
I:123456
E:MEMORY_ARRAY_LOCATION=System Board Or Motherboard
E:MEMORY_ARRAY_NUM_DEVICES=4
E:MEMORY_DEVICE_0_SIZE=17179869184
E:MEMORY_DEVICE_0_TYPE=DDR4
E:MEMORY_DEVICE_0_SPEED_MTS=3200
E:MEMORY_DEVICE_0_CONFIGURED_SPEED_MTS=2933
E:MEMORY_DEVICE_0_SERIAL_NUMBER=SN-0001234
E:MEMORY_DEVICE_0_ASSET_TAG=ASSET-99
E:MEMORY_DEVICE_0_PART_NUMBER=PART-42
E:MEMORY_DEVICE_1_PRESENT=0
E:MEMORY_DEVICE_1_TYPE=Unknown
E:MEMORY_DEVICE_10_SIZE=8589934592
E:MEMORY_DEVICE_10_TYPE=RAM
E:MEMORY_DEVICE_10_SPEED_MTS=2400
E:MEMORY_DEVICE_2_LOCATOR=DIMM C
E:ID_SERIAL=SERIAL-OF-A-DISK
E:MEMORY_DEVICE_3_SIZE=4294967296
E:MEMORY_DEVICE_3_TYPE={red}
";
        let banks = udev_banks(text);
        assert_eq!(
            banks,
            [
                Bank { megabytes: 16384, kind: "DDR4".into(), speed: 2933 },
                Bank { megabytes: 0, kind: String::new(), speed: 0 },
                Bank { megabytes: 4096, kind: String::new(), speed: 0 },
                Bank { megabytes: 8192, kind: String::new(), speed: 2400 },
            ],
            "slots in order; a slot with neither size nor presence left out"
        );
        let kept = format!("{banks:?}");
        assert!(["SN-", "ASSET", "PART", "SERIAL", "DIMM", "red"].iter().all(|s| !kept.contains(s)), "{kept}");
        assert!(udev_banks("").is_empty() && udev_banks("E:ID_SERIAL=X\n").is_empty());
    }

    #[test]
    fn the_device_tree_gives_each_memory_node_as_a_bank() {
        // Two cells of address and two of size: 4 GB at 0x40000000 and 2 GB
        // above 4 GB.
        let two = 2u32.to_be_bytes();
        let reg = [0u32, 0x4000_0000, 1, 0, 0, 1, 0, 0x8000_0000].iter().flat_map(|c| c.to_be_bytes()).collect::<Vec<u8>>();
        assert_eq!(device_tree_megabytes(&reg, &two, &two), 4096 + 2048);
        // The defaults, two cells of address and one of size.
        let reg = [0u32, 0x8000_0000, 0x4000_0000].iter().flat_map(|c| c.to_be_bytes()).collect::<Vec<u8>>();
        assert_eq!(device_tree_megabytes(&reg, &[], &[]), 1024);
        // Cells it can't hold.
        assert_eq!(device_tree_megabytes(&reg, &4u32.to_be_bytes(), &two), 0);
        assert_eq!(device_tree_megabytes(&reg, &two, &0u32.to_be_bytes()), 0);
        assert_eq!(device_tree_megabytes(&reg, &two, &3u32.to_be_bytes()), 0);
        assert_eq!(device_tree_megabytes(&[1, 2, 3], &[], &[]), 0);
    }

    #[test]
    fn a_sysfs_link_gives_the_root_and_the_slots_down_to_a_function() {
        assert_eq!(sysfs_chain("../../../devices/pci0000:00/0000:00:1c.4/0000:05:00.0"), Some((0, 0, vec![(0x1c, 4), (0, 0)])));
        assert_eq!(sysfs_chain("../../../devices/pci0000:00/0000:00:1f.3"), Some((0, 0, vec![(0x1f, 3)])));
        // Hyper-V's: under its VMBus, the device's instance GUID, then a root
        // of its own. pci-hyperv numbers that root's domain from the GUID's
        // second group (bytes 4 and 5), and that domain, the bus address
        // lspci shows, is all that's kept of it: the GUID's part of the path
        // is never read.
        let hyper_v = "../../../devices/LNXSYSTM:00/LNXSYBUS:00/ACPI0004:00/VMBUS:00/1b1a7a2e-3f4c-4d5e-8f90-123456789abc/pci3f4c:00/3f4c:00:00.0";
        let (domain, bus, chain) = sysfs_chain(hyper_v).unwrap();
        assert_eq!((domain, bus, chain), (0x3f4c, 0, vec![(0, 0)]));
        assert_eq!(pci_root(domain, bus), "/pci@3f4c,0");
        // A part shaped like a GUID's isn't a function, so a path that ends
        // in one gives nothing rather than a piece of it.
        assert_eq!(sysfs_chain("../../../devices/pci0000:00/0000:00:01.0/1b1a7a2e-3f4c-4d5e-8f90-123456789abc"), None);
        // A root bus under another (a VMD's) starts afresh, and Linux numbers
        // a VMD's domain from 0x10000, past 16 bits.
        assert_eq!(sysfs_chain("../../../devices/pci0000:00/0000:00:0e.0/pci0001:e0/0001:e0:06.0/0001:e1:00.0"), Some((1, 0xe0, vec![(6, 0), (0, 0)])));
        let vmd = "../../../devices/pci0000:00/0000:00:0e.0/pci10000:e0/10000:e0:06.0/10000:e1:00.0";
        assert_eq!(sysfs_chain(vmd), Some((0x10000, 0xe0, vec![(6, 0), (0, 0)])));
        assert_eq!(pci_root(0x10000, 0xe0), "/pci@10000,e0");
        for bad in ["", "garbage", "../../../devices/pci0000:00", "../../devices/pci0000:00/0000:00:20.0", "../pci0000:00/0000:00:1f.8", "../pci0000:00/usb1", "../pci123456789:00/0000:00:00.0", "0000:00:1f.3"] {
            assert_eq!(sysfs_chain(bad), None, "{bad}");
        }
    }

    #[test]
    fn an_iommu_is_named_by_its_address_or_its_pci_function() {
        assert_eq!(iommu_unit("dmar1", Some("fed91000\n"), Some("../../devices/virtual/iommu/dmar1")), "fed91000");
        assert_eq!(iommu_unit("dmar0", Some("0x00000000FED90000"), None), "fed90000");
        assert_eq!(iommu_unit("smmu3.0x0000000009050000", None, Some("../../devices/platform/arm-smmu-v3.0.auto/iommu/smmu3.0x0000000009050000")), "9050000");
        assert_eq!(iommu_unit("ivhd0", None, Some("../../devices/pci0000:00/0000:00:00.2/iommu/ivhd0")), "0,2");
        assert_eq!(iommu_unit("virtio", None, Some("../../devices/pci0000:00/0000:00:01.0/virtio0/iommu/virtio")), "");
        assert_eq!(iommu_unit("dmar1", None, Some("../../devices/virtual/iommu/dmar1")), "");
        assert_eq!(iommu_unit("x", Some("not hex"), None), "");
    }

    /// A made-up root folder of a Linux, with real links.
    #[cfg(unix)]
    struct Tree(std::path::PathBuf);

    #[cfg(unix)]
    impl Tree {
        fn new(name: &str) -> Tree {
            let dir = std::env::temp_dir().join(format!("flow-hardware-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Tree(dir)
        }

        fn file(&self, path: &str, bytes: impl AsRef<[u8]>) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }

        fn link(&self, path: &str, target: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, path).unwrap();
        }

        /// A PCI function under sys/devices, linked from sys/bus/pci/devices.
        fn function(&self, path: &str, class: &str, vendor: &str, device: &str, driver: Option<&str>, iommu: &str) {
            let dir = format!("sys/devices/{path}");
            self.file(&format!("{dir}/class"), class);
            self.file(&format!("{dir}/vendor"), vendor);
            self.file(&format!("{dir}/device"), device);
            // A file no reader may open, holding a marker to look for.
            self.file(&format!("{dir}/vpd"), "VPD-MARKER-0xC0FFEE");
            self.file(&format!("{dir}/label"), "Alex's slot");
            if let Some(driver) = driver {
                self.link(&format!("{dir}/driver"), &format!("../../../bus/pci/drivers/{driver}"));
            }
            self.link(&format!("{dir}/iommu"), &format!("../../virtual/iommu/{iommu}"));
            let name = path.rsplit('/').next().unwrap();
            self.link(&format!("sys/bus/pci/devices/{name}"), &format!("../../../devices/{path}"));
        }
    }

    #[cfg(unix)]
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn linux_is_read_from_its_files_alone() {
        let tree = Tree::new("full");
        tree.file("run/udev/data/+dmi:id", "E:MEMORY_DEVICE_0_SIZE=8589934592\nE:MEMORY_DEVICE_0_TYPE=DDR5\nE:MEMORY_DEVICE_0_CONFIGURED_SPEED_MTS=5600\nE:MEMORY_DEVICE_0_SERIAL_NUMBER=VPD-MARKER-0xC0FFEE\nE:MEMORY_DEVICE_1_PRESENT=0\n");
        tree.file("run/udev/data/b8:0", "E:ID_SERIAL=VPD-MARKER-0xC0FFEE\n");
        tree.function("pci0000:00/0000:00:1c.0", "0x060400\n", "0x8086\n", "0xa338\n", Some("pcieport"), "dmar1");
        tree.function("pci0000:00/0000:00:1c.0/0000:02:00.0", "0x020000\n", "0x10ec\n", "0x8168\n", Some("r8169"), "dmar1");
        tree.function("pci0000:00/0000:00:1f.0", "0x060100\n", "0x8086\n", "0xa30d\n", None, "dmar1");
        tree.function("pci0000:00/0000:00:1f.3", "0x040380\n", "0x8086\n", "0xa348\n", Some("snd_hda_intel"), "dmar1");
        tree.function("pci0000:00/0000:00:02.0", "0x030000\n", "0x8086\n", "0x3e92\n", Some("i915"), "dmar0");
        tree.link("sys/class/iommu/dmar1", "../../devices/virtual/iommu/dmar1");
        tree.file("sys/devices/virtual/iommu/dmar1/intel-iommu/address", "fed91000\n");
        tree.link("sys/class/iommu/dmar0", "../../devices/virtual/iommu/dmar0");
        tree.file("sys/devices/virtual/iommu/dmar0/intel-iommu/address", "fed90000\n");
        let hardware = read(&tree.0);
        let probe = |path: &str, at: &str, names: &[&str]| Probe { path: path.into(), at: at.into(), names: names.iter().map(|n| n.to_string()).collect() };
        assert_eq!(
            hardware,
            Hardware {
                banks: vec![Bank { megabytes: 8192, kind: "DDR5".into(), speed: 5600 }, Bank::default()],
                probes: vec![
                    probe("/iommu@fed90000/pci@0,0", "2,0", &["i915"]),
                    probe("/iommu@fed91000/pci@0,0", "1c,0", &["r8169"]),
                    probe("/iommu@fed91000/pci@0,0", "1f,0", &["pci8086,a30d", "snd_hda_intel"]),
                ],
            }
        );
        let kept = format!("{hardware:?}");
        assert!(!kept.contains("MARKER") && !kept.contains("Alex"), "{kept}");
        // A board with a device tree and no udev memory, and no PCI.
        let board = Tree::new("board");
        board.file("sys/firmware/devicetree/base/#address-cells", 1u32.to_be_bytes());
        board.file("sys/firmware/devicetree/base/#size-cells", 1u32.to_be_bytes());
        board.file("sys/firmware/devicetree/base/memory@40000000/reg", [0x4000_0000u32, 0x8000_0000].iter().flat_map(|c| c.to_be_bytes()).collect::<Vec<u8>>());
        board.file("sys/firmware/devicetree/base/memory-controller/reg", 0x4000_0000u32.to_be_bytes());
        assert_eq!(read(&board.0), Hardware { banks: vec![Bank { megabytes: 2048, ..Default::default() }], probes: Vec::new() });
        // Nothing at all.
        let empty = Tree::new("empty");
        assert_eq!(read(&empty.0), Hardware::default());
    }
}
