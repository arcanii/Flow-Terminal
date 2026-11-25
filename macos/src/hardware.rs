//! This Mac's memory banks and PCI slots, for the Workstation's PROM, from
//! IOKit's device tree (the `IODeviceTree` plane), which any user may read
//! and which asks for no permission. core's `hardware::apple` reads the
//! bytes; this fetches them. Only the keys named here are read: the tree
//! also holds serial numbers, MAC addresses, UUIDs and the like, which
//! never are.

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

use flow_core::hardware::{self, apple, Bank, Hardware, PciFunction, Probe, MOST_FUNCTIONS};

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IORegistryEntryFromPath(main_port: u32, path: *const c_char) -> u32;
    fn IORegistryEntryGetProperty(entry: u32, key: *const c_char, buffer: *mut u8, size: *mut u32) -> i32;
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingServices(main_port: u32, matching: *mut c_void, iterator: *mut u32) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32) -> i32;
    fn IORegistryEntryGetNameInPlane(entry: u32, plane: *const c_char, name: *mut c_char) -> i32;
    fn IORegistryEntryGetLocationInPlane(entry: u32, plane: *const c_char, location: *mut c_char) -> i32;
    fn IORegistryEntryGetPath(entry: u32, plane: *const c_char, path: *mut c_char) -> i32;
    fn IOObjectConformsTo(object: u32, class: *const c_char) -> i32;
    fn IOObjectRelease(object: u32) -> i32;
}

/// The default main port (`kIOMainPortDefault`).
const MAIN_PORT: u32 = 0;
const PLANE: &CStr = c"IODeviceTree";
const PCI_DEVICE: &CStr = c"IOPCIDevice";
/// A DART's mapper for a device, a child of the DART in the tree.
const DART_MAPPER: &CStr = c"IODARTMapperNub";
const HANDLE: &CStr = c"AAPL,phandle";
/// The most bridges a PCI function is looked for under.
const DEEPEST: usize = 8;

/// This Mac's memory, read once a run, and its buses, read afresh, so a
/// tab opened after a device is plugged in shows it.
pub fn read() -> Hardware {
    static BANKS: OnceLock<Vec<Bank>> = OnceLock::new();
    Hardware { banks: BANKS.get_or_init(banks).clone(), probes: probes() }
}

/// An object in IOKit's registry (an entry or an iterator), released when
/// dropped.
struct Entry(u32);

impl Drop for Entry {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: an object this holds the one reference to.
            unsafe { IOObjectRelease(self.0) };
        }
    }
}

impl Entry {
    /// The entry at a registry path ("IODeviceTree:/chosen").
    fn at(path: &CStr) -> Option<Entry> {
        // SAFETY: the path ends in a NUL.
        let entry = unsafe { IORegistryEntryFromPath(MAIN_PORT, path.as_ptr()) };
        (entry != 0).then_some(Entry(entry))
    }

    /// A property's bytes; empty where there's none, or it's too long for
    /// the call. An empty property succeeds without setting the size, so
    /// only a size under the buffer's counts.
    fn property(&self, key: &CStr) -> Vec<u8> {
        const SIZE: u32 = 4096;
        let mut buffer = vec![0u8; SIZE as usize];
        let mut size = SIZE;
        // SAFETY: the key ends in a NUL, and the buffer holds `size` bytes,
        // which the call writes at most.
        let result = unsafe { IORegistryEntryGetProperty(self.0, key.as_ptr(), buffer.as_mut_ptr(), &mut size) };
        if result != 0 || size >= SIZE {
            return Vec::new();
        }
        buffer.truncate(size as usize);
        buffer
    }

    /// The next object an iterator gives.
    fn next(&self) -> Option<Entry> {
        // SAFETY: takes the iterator this holds.
        let next = unsafe { IOIteratorNext(self.0) };
        (next != 0).then_some(Entry(next))
    }

    /// Its parent in the device tree.
    fn parent(&self) -> Option<Entry> {
        let mut parent = 0;
        // SAFETY: the plane's name ends in a NUL; writes one object, ours to release.
        let result = unsafe { IORegistryEntryGetParentEntry(self.0, PLANE.as_ptr(), &mut parent) };
        (result == 0 && parent != 0).then_some(Entry(parent))
    }

    fn conforms_to(&self, class: &CStr) -> bool {
        // SAFETY: the class's name ends in a NUL.
        unsafe { IOObjectConformsTo(self.0, class.as_ptr()) != 0 }
    }

    /// Its name in the device tree ("wlan").
    fn name(&self) -> Option<String> {
        // SAFETY: the plane's name ends in a NUL, and the buffer is an
        // io_name_t's 128 bytes, the most the call writes.
        text::<128>(|out| unsafe { IORegistryEntryGetNameInPlane(self.0, PLANE.as_ptr(), out) })
    }

    /// Where it is on its parent's bus ("0,1").
    fn location(&self) -> Option<String> {
        // SAFETY: as for `name`.
        text::<128>(|out| unsafe { IORegistryEntryGetLocationInPlane(self.0, PLANE.as_ptr(), out) })
    }

    /// Its path in the device tree, "IODeviceTree:/arm-io@10F00000/...".
    fn path(&self) -> Option<String> {
        // SAFETY: the plane's name ends in a NUL, and the buffer is an
        // io_string_t's 512 bytes, the most the call writes.
        text::<512>(|out| unsafe { IORegistryEntryGetPath(self.0, PLANE.as_ptr(), out) })
    }
}

/// The text a call writes into a buffer of `N` bytes, up to its NUL; none if
/// the call fails.
fn text<const N: usize>(call: impl FnOnce(*mut c_char) -> i32) -> Option<String> {
    let mut buffer = [0u8; N];
    if call(buffer.as_mut_ptr().cast()) != 0 {
        return None;
    }
    let len = buffer.iter().position(|&b| b == 0).unwrap_or(N);
    Some(String::from_utf8_lossy(&buffer[..len]).into_owned())
}

/// The memory: Apple silicon's in `/chosen`, an Intel Mac's DIMMs in
/// `/memory`, else Apple silicon's total, which is honestly its one bank
/// (one pool in the chip's package); none where the tree doesn't say. Each
/// is taken only where it adds up to the memory macOS reports, so one that
/// lists nothing filled, or the wrong amount, gives way to the next.
fn banks() -> Vec<Bank> {
    let physical = objc2_foundation::NSProcessInfo::processInfo().physicalMemory();
    let total = || pooled(physical);
    let readings: [&dyn Fn() -> Vec<Bank>; 3] = [&chosen, &dimms, &total];
    first_agreeing(rounded_gb(physical), readings.iter().map(|read| read()))
}

/// Bytes as the whole gigabytes the system reports.
fn rounded_gb(bytes: u64) -> u64 {
    (bytes as f64 / (1u64 << 30) as f64).round() as u64
}

/// The first of the readings whose banks add up to the memory the system
/// reports (`memory_gb`); none if none do. Readings are made only until one
/// does.
fn first_agreeing(memory_gb: u64, readings: impl IntoIterator<Item = Vec<Bank>>) -> Vec<Bank> {
    readings.into_iter().map(|banks| Hardware { banks, probes: Vec::new() }).find(|h| h.banks_agree(memory_gb)).map(|h| h.banks).unwrap_or_default()
}

/// Apple silicon's memory as `/chosen` gives it.
fn chosen() -> Vec<Bank> {
    let Some(chosen) = Entry::at(c"IODeviceTree:/chosen") else { return Vec::new() };
    apple::chosen_bank(&chosen.property(c"dram-size"), &chosen.property(c"dram-type")).into_iter().collect()
}

/// An Intel Mac's DIMMs. Apple silicon has a `memory@0` too, which the path
/// finds by its name, but its `reg` is where the memory lies, not its slots.
fn dimms() -> Vec<Bank> {
    if cfg!(target_arch = "x86_64") {
        memory_node()
    } else {
        Vec::new()
    }
}

/// `/memory`'s banks, read as an Intel Mac's.
fn memory_node() -> Vec<Bank> {
    let Some(memory) = Entry::at(c"IODeviceTree:/memory") else { return Vec::new() };
    apple::memory_banks(&memory.property(c"reg"), &memory.property(c"dimm-types"), &memory.property(c"dimm-speeds"))
}

/// Apple silicon's memory in all as its one bank; none on an Intel Mac,
/// whose memory is in banks of its own.
fn pooled(physical: u64) -> Vec<Bank> {
    let megabytes = physical >> 20;
    if cfg!(target_arch = "aarch64") && megabytes > 0 {
        vec![Bank { megabytes, ..Default::default() }]
    } else {
        Vec::new()
    }
}

/// The PCI slots: every PCI device in the device tree, under the bus it
/// hangs from, by its real path, under the DART (an IOMMU) its port's
/// devices go through: Apple silicon's DARTs sit beside its PCIe
/// controllers rather than above them, so it's found by the handles each
/// names (`darts`, `apple::iommu`).
/// The empty ports `#ports` suggests aren't probed: they can't be told from
/// lanes not wired.
fn probes() -> Vec<Probe> {
    // SAFETY: the class's name ends in a NUL; the dictionary it returns
    // goes to IOServiceGetMatchingServices, which releases it.
    let matching = unsafe { IOServiceMatching(PCI_DEVICE.as_ptr()) };
    if matching.is_null() {
        return Vec::new();
    }
    let mut iterator = 0;
    // SAFETY: takes the dictionary (whatever it returns) and writes one
    // iterator, ours to release.
    if unsafe { IOServiceGetMatchingServices(MAIN_PORT, matching, &mut iterator) } != 0 {
        return Vec::new();
    }
    let iterator = Entry(iterator);
    let darts = darts();
    let functions: Vec<PciFunction> = std::iter::from_fn(|| iterator.next()).take(MOST_FUNCTIONS).filter_map(|device| function(&device, &darts)).collect();
    hardware::pci_probes(functions)
}

/// Each DART's mappers' handles, and the DART's own, to the DART's unit
/// address: what a PCI function's `iommu-parent` (a mapper) and a port's
/// bridge's `function-dart_self` (the DART) name.
fn darts() -> HashMap<u32, String> {
    let mut darts = HashMap::new();
    // SAFETY: as in `probes`.
    let matching = unsafe { IOServiceMatching(DART_MAPPER.as_ptr()) };
    if matching.is_null() {
        return darts;
    }
    let mut iterator = 0;
    // SAFETY: as in `probes`.
    if unsafe { IOServiceGetMatchingServices(MAIN_PORT, matching, &mut iterator) } != 0 {
        return darts;
    }
    let iterator = Entry(iterator);
    for mapper in std::iter::from_fn(|| iterator.next()).take(MOST_FUNCTIONS) {
        let Some(dart) = mapper.parent() else { continue };
        let Some(address) = dart.location().as_deref().and_then(apple::dart_address) else { continue };
        for handle in [hardware::le_u32(&mapper.property(HANDLE)), hardware::le_u32(&dart.property(HANDLE))] {
            if handle != 0 {
                darts.insert(handle, address.clone());
            }
        }
    }
    darts
}

/// A PCI device as a function on its root bus, by the slots from that bus
/// down, under the DART it goes through; none if it isn't in the device
/// tree, or hangs too deep.
fn function(device: &Entry, darts: &HashMap<u32, String>) -> Option<PciFunction> {
    let mut chain = vec![apple::dev_fn(&device.location()?)?];
    let mut parent = device.parent()?;
    while parent.conforms_to(PCI_DEVICE) {
        if chain.len() > DEEPEST {
            return None;
        }
        chain.push(apple::dev_fn(&parent.location()?)?);
        parent = parent.parent()?;
    }
    chain.reverse();
    let root = parent.path()?.strip_prefix("IODeviceTree:")?.to_string();
    Some(PciFunction {
        root,
        iommu: apple::iommu(&device.property(c"iommu-parent"), &device.property(c"function-dart_self"), darts),
        chain,
        class: hardware::le_u32(&device.property(c"class-code")),
        name: device.name().unwrap_or_default(),
        vendor: hardware::le_u32(&device.property(c"vendor-id")) as u16,
        device: hardware::le_u32(&device.property(c"device-id")) as u16,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_mac_probes_its_memory_and_buses() {
        let started = std::time::Instant::now();
        let h = read();
        let took = started.elapsed();
        // Only clean data: sizes, kinds, bus paths and device-tree names.
        eprintln!("{h:?} in {took:?}");
        assert!(took < std::time::Duration::from_millis(100), "{took:?}");
        if cfg!(target_arch = "aarch64") {
            let physical = objc2_foundation::NSProcessInfo::processInfo().physicalMemory();
            assert_eq!(h.banks.len(), 1, "{h:?}");
            assert!(h.banks_agree(rounded_gb(physical)), "{h:?}");
            // Were `/chosen` to say nothing, this Mac's own `/memory`, read
            // as an Intel Mac's, gives way to the chip's total.
            assert_eq!(first_agreeing(rounded_gb(physical), [Vec::new(), memory_node(), pooled(physical)]), pooled(physical));
            assert!(dimms().is_empty());
        }
        // Read again, it's the same, the buses read afresh in a few
        // milliseconds.
        let started = std::time::Instant::now();
        assert_eq!(read(), h);
        assert!(started.elapsed() < std::time::Duration::from_millis(50), "{:?}", started.elapsed());
    }

    #[test]
    fn a_reading_that_doesnt_add_up_gives_way_to_the_next() {
        let bank = |megabytes| Bank { megabytes, ..Default::default() };
        // No `/chosen`, then Apple silicon's `memory@0`, whose `reg` read
        // as slots is one of nothing: the chip's total.
        assert_eq!(first_agreeing(64, [Vec::new(), vec![bank(0)], vec![bank(65536)]]), [bank(65536)]);
        // A `/chosen` that says too little, or an Intel Mac's words read the
        // wrong way round.
        assert_eq!(first_agreeing(64, [vec![bank(8)], vec![bank(16384), bank(1 << 40)], vec![bank(65536)]]), [bank(65536)]);
        // The first that adds up wins, and readings after it aren't made.
        let never = || -> Vec<Bank> { panic!("read after one that added up") };
        let readings: [&dyn Fn() -> Vec<Bank>; 2] = [&|| vec![bank(32768), bank(32768)], &never];
        assert_eq!(first_agreeing(64, readings.iter().map(|read| read())), [bank(32768), bank(32768)]);
        // None that do: none, and the PROM probes made-up banks.
        assert!(first_agreeing(64, [vec![bank(0)], Vec::new()]).is_empty());
        assert!(pooled(0).is_empty());
    }
}
