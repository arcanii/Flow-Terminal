//! This Mac's own id (IOKit's `IOPlatformUUID`), which only seeds the
//! workstations' made-up numbers after core's `identity` hashes it, and,
//! read only while Theme details' Its real numbers is on, its serial
//! number (`IOPlatformSerialNumber`) and its first Ethernet or Wi-Fi
//! address (`getifaddrs`). `hardware.rs` never reads any of them.

use std::ffi::{c_char, c_void, CStr};

use flow_core::identity::{usable_serial, RealNumbers};
use objc2_foundation::NSString;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> u32;
    fn IORegistryEntryCreateCFProperty(entry: u32, key: *const c_void, allocator: *const c_void, options: u32) -> *const c_void;
    fn IOObjectRelease(object: u32) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFGetTypeID(object: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFRelease(object: *const c_void);
}

/// The default main port (`kIOMainPortDefault`).
const MAIN_PORT: u32 = 0;

/// A text property of the platform's own entry in IOKit's registry.
fn platform_text(key: &str) -> Option<String> {
    // SAFETY: the class name ends in a NUL; the dictionary made goes to
    // IOServiceGetMatchingService, which releases it.
    let entry = unsafe { IOServiceGetMatchingService(MAIN_PORT, IOServiceMatching(c"IOPlatformExpertDevice".as_ptr())) };
    if entry == 0 {
        return None;
    }
    let key = NSString::from_str(key);
    // SAFETY: NSString is toll-free bridged with CFString; the copy made is
    // ours to release, and the entry's one reference too.
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, (&*key as *const NSString).cast(), std::ptr::null(), 0) };
    unsafe { IOObjectRelease(entry) };
    if value.is_null() {
        return None;
    }
    // SAFETY: read as a string only if it is one, then released.
    let text = unsafe { (CFGetTypeID(value) == CFStringGetTypeID()).then(|| (*value.cast::<NSString>()).to_string()) };
    unsafe { CFRelease(value) };
    text.filter(|t| !t.trim().is_empty())
}

/// This Mac's id for itself, which stays as long as its logic board does.
pub fn machine_id() -> Option<String> {
    platform_text("IOPlatformUUID")
}

/// Its serial number and the address of its first Ethernet or Wi-Fi
/// interface (`en0` where it has one).
pub fn real_numbers() -> RealNumbers {
    RealNumbers { ether: ether(), serial: platform_text("IOPlatformSerialNumber").and_then(|s| usable_serial(&s)) }
}

fn ether() -> Option<[u8; 6]> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: fills a list freed below.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return None;
    }
    let mut found: Vec<(String, [u8; 6])> = Vec::new();
    let mut at = list;
    while !at.is_null() {
        // SAFETY: an entry of the list getifaddrs made, its name a C string
        // and its address, where it's a link's, a sockaddr_dl whose data
        // holds the name and then the address.
        unsafe {
            let entry = &*at;
            let addr = entry.ifa_addr;
            if !addr.is_null() && (*addr).sa_family as i32 == libc::AF_LINK {
                let dl = &*addr.cast::<libc::sockaddr_dl>();
                if dl.sdl_alen == 6 {
                    let data = std::ptr::addr_of!(dl.sdl_data).cast::<u8>().add(dl.sdl_nlen as usize);
                    let mut address = [0u8; 6];
                    std::ptr::copy_nonoverlapping(data, address.as_mut_ptr(), 6);
                    let name = CStr::from_ptr(entry.ifa_name).to_string_lossy().into_owned();
                    if name.starts_with("en") && address != [0; 6] {
                        found.push((name, address));
                    }
                }
            }
            at = entry.ifa_next;
        }
    }
    // SAFETY: the list getifaddrs made.
    unsafe { libc::freeifaddrs(list) };
    found.sort_by_key(|(name, _)| (name.len(), name.clone()));
    found.first().map(|(_, address)| *address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_mac_has_an_id_a_serial_and_an_address() {
        // Only their shape: what they are is this Mac's own.
        let id = machine_id().expect("an id");
        assert_eq!(id.len(), 36, "a UUID");
        assert_eq!(machine_id().as_deref(), Some(id.as_str()), "the same each time");
        let real = real_numbers();
        assert!(real.serial.is_some_and(|s| (6..=16).contains(&s.len())));
        // A Mac in a virtual machine may have no en0; a real one has.
        if let Some(address) = real.ether {
            assert_ne!(address, [0; 6]);
        }
    }
}
