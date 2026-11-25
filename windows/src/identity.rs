//! This PC's own id (the registry's `MachineGuid`), which only seeds the
//! workstations' made-up numbers after core's `identity` hashes it, and,
//! read only while Theme details' Its real numbers is on, its serial number
//! (SMBIOS's system serial, through the firmware table `hardware.rs` reads)
//! and the address of its first wired or Wi-Fi adapter
//! (`GetAdaptersAddresses`).

use flow_core::identity::{smbios_serial, RealNumbers};
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GAA_FLAG_SKIP_UNICAST, IF_TYPE_ETHERNET_CSMACD,
    IF_TYPE_IEEE80211, IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::System::SystemInformation::RSMB;

/// This PC's id for itself, set when Windows was installed.
pub fn machine_id() -> Option<String> {
    crate::system::registry_text(r"SOFTWARE\Microsoft\Cryptography", "MachineGuid")
}

/// Its serial number and the address of its first wired or Wi-Fi adapter.
pub fn real_numbers() -> RealNumbers {
    RealNumbers { ether: ether(), serial: smbios_serial(&crate::hardware::firmware_table(RSMB, 0)) }
}

fn ether() -> Option<[u8; 6]> {
    let flags = GAA_FLAG_SKIP_UNICAST | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size: u32 = 16 * 1024;
    // The list may grow between asking its size and reading it.
    for _ in 0..3 {
        // Eight-byte words, for the adapters' alignment.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: writes at most `size` bytes into the buffer, which holds
        // them, and says how many it needs when it doesn't.
        let got = unsafe { GetAdaptersAddresses(0, flags, std::ptr::null(), first, &mut size) };
        if got == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if got != ERROR_SUCCESS {
            return None;
        }
        let mut at = first.cast_const();
        while !at.is_null() {
            // SAFETY: an adapter of the list just written into the buffer,
            // which outlives this walk.
            let adapter = unsafe { &*at };
            let kind = adapter.IfType;
            if (kind == IF_TYPE_ETHERNET_CSMACD || kind == IF_TYPE_IEEE80211) && adapter.PhysicalAddressLength == 6 {
                let address: [u8; 6] = adapter.PhysicalAddress[..6].try_into().expect("six bytes");
                if address != [0; 6] {
                    return Some(address);
                }
            }
            at = adapter.Next;
        }
        return None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_pc_has_an_id_that_stays_the_same() {
        // Only its shape: what it is is this PC's own.
        let id = machine_id().expect("an id");
        assert_eq!(id.len(), 36, "a GUID");
        assert_eq!(machine_id().as_deref(), Some(id.as_str()));
        let _ = real_numbers();
    }
}
