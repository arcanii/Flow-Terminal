//! This PC's memory banks and PCI slots, for the Workstation's PROM: the
//! firmware's SMBIOS table and its ACPI tables of IOMMUs
//! (`GetSystemFirmwareTable`), and the PCI functions the configuration
//! manager lists (cfgmgr32), none of which needs an administrator. core's
//! `hardware::windows` reads the bytes and strings; this only fetches them.
//! Device instance IDs (whose USB suffixes can carry serials), friendly
//! names and descriptions (Bluetooth's are given by people) are never kept.

use std::sync::OnceLock;

use flow_core::hardware::{self, Hardware, PciFunction, Probe, MOST_FUNCTIONS};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW, CM_Locate_DevNodeW, CM_DRP_COMPATIBLEIDS, CM_DRP_HARDWAREID, CM_DRP_LOCATION_PATHS,
    CM_DRP_SERVICE, CM_GETIDLIST_FILTER_ENUMERATOR, CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_NORMAL, CR_SUCCESS,
};
use windows_sys::Win32::System::SystemInformation::{EnumSystemFirmwareTables, GetSystemFirmwareTable, ACPI, RSMB};

/// This PC's memory, read once a run, and its buses, read afresh, so a
/// tab opened after a device is plugged in shows it.
pub fn read() -> Hardware {
    static BANKS: OnceLock<Vec<hardware::Bank>> = OnceLock::new();
    let banks = BANKS.get_or_init(|| hardware::windows::smbios_banks(&firmware_table(RSMB, 0)));
    Hardware { banks: banks.clone(), probes: probes() }
}

/// One of the firmware's tables, as Windows hands it over; empty where it
/// won't. Never kept, logged or printed: SMBIOS's holds serial numbers.
pub(crate) fn firmware_table(provider: u32, id: u32) -> Vec<u8> {
    // SAFETY: with no buffer it only says how big the table is.
    let size = unsafe { GetSystemFirmwareTable(provider, id, std::ptr::null_mut(), 0) };
    if size == 0 {
        return Vec::new();
    }
    let mut table = vec![0u8; size as usize];
    // SAFETY: the buffer holds `size` bytes, which the call writes at most.
    let got = unsafe { GetSystemFirmwareTable(provider, id, table.as_mut_ptr(), size) };
    if got == 0 || got > size {
        return Vec::new();
    }
    table.truncate(got as usize);
    table
}

/// The IOMMUs the ACPI tables list: Intel's DMAR and AMD's IVRS, each
/// asked for by the ID Windows listed it under, whichever way round its
/// bytes go. None where Windows won't give them, and the slots' paths then
/// start at their bus.
fn iommu_units() -> Vec<hardware::windows::Iommu> {
    // SAFETY: with no buffer it only says how long the list of IDs is.
    let size = unsafe { EnumSystemFirmwareTables(ACPI, std::ptr::null_mut(), 0) };
    if size == 0 {
        return Vec::new();
    }
    let mut ids = vec![0u8; size as usize];
    // SAFETY: the buffer holds `size` bytes, which the call writes at most.
    let got = unsafe { EnumSystemFirmwareTables(ACPI, ids.as_mut_ptr(), size) };
    if got == 0 || got > size {
        return Vec::new();
    }
    ids.truncate(got as usize);
    let mut units = Vec::new();
    for id in ids.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)) {
        if [id.to_le_bytes(), id.to_be_bytes()].iter().any(|s| s == b"DMAR" || s == b"IVRS") {
            units.extend(hardware::windows::iommu_units(&firmware_table(ACPI, id)));
        }
    }
    units
}

/// A string property of a device node, as its list of strings; empty where
/// it has none.
fn property(node: u32, which: u32) -> Vec<String> {
    let mut bytes = 0u32;
    // SAFETY: with no buffer it only says how many bytes the property takes.
    unsafe { CM_Get_DevNode_Registry_PropertyW(node, which, std::ptr::null_mut(), std::ptr::null_mut(), &mut bytes, 0) };
    if bytes == 0 {
        return Vec::new();
    }
    let mut wide = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: the buffer holds at least `bytes` bytes, which the call writes at most.
    let read = unsafe { CM_Get_DevNode_Registry_PropertyW(node, which, std::ptr::null_mut(), wide.as_mut_ptr().cast(), &mut bytes, 0) };
    if read != CR_SUCCESS {
        return Vec::new();
    }
    hardware::windows::multi_sz(&wide)
}

/// The device nodes of the PCI functions present (at most
/// `MOST_FUNCTIONS`); `None` where cfgmgr32 won't list them. The instance
/// IDs only find the nodes, and are dropped.
fn pci_nodes() -> Option<Vec<u32>> {
    let filter: Vec<u16> = "PCI".encode_utf16().chain([0]).collect();
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;
    let mut len = 0u32;
    // SAFETY: the filter ends in a NUL; writes one u32.
    if unsafe { CM_Get_Device_ID_List_SizeW(&mut len, filter.as_ptr(), flags) } != CR_SUCCESS {
        return None;
    }
    if len == 0 {
        return Some(Vec::new());
    }
    let mut list = vec![0u16; len as usize];
    // SAFETY: the filter ends in a NUL, and the buffer holds `len`
    // characters, which the call writes at most.
    if unsafe { CM_Get_Device_ID_ListW(filter.as_ptr(), list.as_mut_ptr(), len, flags) } != CR_SUCCESS {
        return None;
    }
    let mut nodes = Vec::new();
    for id in hardware::windows::multi_sz(&list).into_iter().take(MOST_FUNCTIONS) {
        let id: Vec<u16> = id.encode_utf16().chain([0]).collect();
        let mut node = 0u32;
        // SAFETY: the ID ends in a NUL; writes one u32.
        if unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) } == CR_SUCCESS {
            nodes.push(node);
        }
    }
    Some(nodes)
}

/// The PCI slots: each PCI function present, by its location path, IDs and
/// service, under the IOMMU over it. PCIROOT's number is its root's ACPI
/// _UID, its bus on every PC with one root; cfgmgr32 gives no segment, so
/// it's 0.
fn probes() -> Vec<Probe> {
    let Some(nodes) = pci_nodes().filter(|nodes| !nodes.is_empty()) else { return Vec::new() };
    let units = iommu_units();
    let mut functions = Vec::new();
    for node in nodes {
        let Some((root, chain)) = property(node, CM_DRP_LOCATION_PATHS).iter().find_map(|path| hardware::windows::location_chain(path)) else { continue };
        let (vendor, device, class) = hardware::windows::pci_ids(&property(node, CM_DRP_HARDWAREID), &property(node, CM_DRP_COMPATIBLEIDS));
        let name = property(node, CM_DRP_SERVICE).first().map(|s| s.to_lowercase()).unwrap_or_default();
        let iommu = hardware::windows::iommu_over(&units, 0, root, &chain);
        functions.push(PciFunction { root: hardware::pci_root(0, root), iommu, chain, class, name, vendor, device });
    }
    hardware::pci_probes(functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_probes_its_memory_and_buses() {
        let started = std::time::Instant::now();
        let h = read();
        let took = started.elapsed();
        // Only clean data: sizes, kinds, bus addresses and services' names.
        eprintln!("{h:?} in {took:?}");
        assert!(took < std::time::Duration::from_millis(250), "{took:?}");
        // SMBIOS requires its memory devices (type 17), and Hyper-V, VMware,
        // Parallels and QEMU all give them.
        let memory_gb = crate::system::machine().memory_gb;
        assert!(!h.banks.is_empty(), "no memory devices in SMBIOS");
        assert!(h.banks_agree(memory_gb), "{h:?} {memory_gb}");
        // cfgmgr32 lists the PCI functions present, and reads their
        // properties: every PCI function's hardware IDs name its vendor.
        let nodes = pci_nodes().expect("cfgmgr32 lists the PCI functions present");
        let vendors = nodes.iter().filter(|&&node| hardware::windows::pci_ids(&property(node, CM_DRP_HARDWAREID), &[]).0 != 0).count();
        assert_eq!(vendors, nodes.len(), "every PCI function's hardware IDs name its vendor");
        // So where one under a PCI root isn't a host bridge, a slot is
        // probed (a Hyper-V Gen 2 VM rightly has none).
        let probed = nodes
            .iter()
            .filter(|&&node| {
                let (_, _, class) = hardware::windows::pci_ids(&[], &property(node, CM_DRP_COMPATIBLEIDS));
                class >> 8 != 0x0600 && property(node, CM_DRP_LOCATION_PATHS).iter().any(|path| hardware::windows::location_chain(path).is_some())
            })
            .count();
        eprintln!("{} PCI functions present, {probed} of them to probe", nodes.len());
        assert!(probed == 0 || !h.probes.is_empty(), "{h:?}");
        assert_eq!(read(), h, "the same, its buses read afresh");
    }
}
