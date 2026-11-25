//! The computer, for the startup logo's lines and the old machines' start
//! screens: Windows' name, the processor and the memory installed, read once
//! from the registry and the firmware's tables; and how it's powered, for
//! efficient mode.

use std::sync::OnceLock;

use flow_core::platform::Machine;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::{GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx, MEMORYSTATUSEX};

pub fn machine() -> Machine {
    static MACHINE: OnceLock<Machine> = OnceLock::new();
    MACHINE.get_or_init(read_machine).clone()
}

fn read_machine() -> Machine {
    const VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let product = registry_text(VERSION, "ProductName").unwrap_or_else(|| "Windows".into());
    let build = registry_text(VERSION, "CurrentBuild").and_then(|b| b.trim().parse().ok()).unwrap_or(0);
    let chip = registry_text(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString").map(|c| tidy(&c)).unwrap_or_default();
    Machine { os: os_name(&product, build), chip, memory_gb: memory_gb().unwrap_or(0) }
}

/// The product's name, which Windows 11 still gives as Windows 10's ("Windows
/// 10 Pro"); its builds begin at 22000.
fn os_name(product: &str, build: u32) -> String {
    match product.strip_prefix("Windows 10") {
        Some(edition) if build >= 22000 => format!("Windows 11{edition}"),
        _ => product.to_string(),
    }
}

/// The memory installed, in whole gigabytes, as the firmware's tables give
/// it; else what Windows has to use, rounded up, since it keeps some back.
fn memory_gb() -> Option<u64> {
    const GB: f64 = (1u64 << 30) as f64;
    let mut kb = 0u64;
    // SAFETY: writes one u64.
    if unsafe { GetPhysicallyInstalledSystemMemory(&mut kb) } != 0 && kb >= 1 << 20 {
        return Some((kb as f64 * 1024.0 / GB).round() as u64);
    }
    // SAFETY: a zeroed MEMORYSTATUSEX is valid; its length is set before the call.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: fills the struct it's given, whose length says its size.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0 && status.ullTotalPhys > 0).then(|| (status.ullTotalPhys as f64 / GB).ceil() as u64)
}

/// A text value under HKEY_LOCAL_MACHINE.
pub(crate) fn registry_text(key: &str, value: &str) -> Option<String> {
    let key: Vec<u16> = key.encode_utf16().chain([0]).collect();
    let value: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let mut bytes = 0u32;
    // SAFETY: both names end in a NUL; with no buffer it only says how big.
    let found = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut bytes) };
    if found != ERROR_SUCCESS || bytes == 0 {
        return None;
    }
    let mut text = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: the buffer holds `bytes` bytes, which the call writes at most.
    let read = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), text.as_mut_ptr().cast(), &mut bytes) };
    if read != ERROR_SUCCESS {
        return None;
    }
    let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..len])).filter(|t| !t.trim().is_empty())
}

/// How the computer is powered: on battery, energy saver on, or plugged
/// in, as it is where Windows can't say.
pub fn power() -> flow_core::power::Power {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: writes one SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return flow_core::power::Power::Plugged;
    }
    flow_core::power::windows(status.ACLineStatus, status.SystemStatusFlag)
}

/// Single spaces only: Intel's names pad with runs of them.
fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_goes_by_its_own_name() {
        assert_eq!(os_name("Windows 10 Pro", 26100), "Windows 11 Pro");
        assert_eq!(os_name("Windows 10 Home", 19045), "Windows 10 Home");
        assert_eq!(os_name("Windows Server 2025 Datacenter", 26100), "Windows Server 2025 Datacenter");
        assert_eq!(tidy("  Intel(R) Core(TM)   i7-8700 CPU @ 3.20GHz "), "Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz");
    }

    #[test]
    fn this_computer_says_what_it_is() {
        let m = machine();
        eprintln!("{m:?}");
        assert!(m.os.starts_with("Windows"), "{m:?}");
        assert!(!m.chip.is_empty(), "{m:?}");
        assert!(m.memory_gb > 0, "{m:?}");
    }

    #[test]
    fn a_computer_without_a_battery_is_never_on_one() {
        let power = power();
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: writes one SYSTEM_POWER_STATUS.
        assert_ne!(unsafe { GetSystemPowerStatus(&mut status) }, 0);
        eprintln!("{power:?} (AC line {}, battery flag {}, saver {})", status.ACLineStatus, status.BatteryFlag, status.SystemStatusFlag);
        // 128: no system battery. Energy saver can still be on.
        if status.BatteryFlag & 128 != 0 {
            assert_ne!(power, flow_core::power::Power::Battery);
        }
    }
}
