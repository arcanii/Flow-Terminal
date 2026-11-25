//! How this Mac is powered, for efficient mode: the source IOKit's power
//! sources say is providing power ("Battery Power" on battery), and
//! whether Low Power Mode is on, which NSProcessInfo says from macOS 12 on
//! (before that there was none). core's `power::mac` makes a `Power` of
//! them.

use std::ffi::c_void;

use flow_core::power::Power;
use objc2::runtime::NSObjectProtocol;
use objc2::sel;
use objc2_foundation::{NSProcessInfo, NSString};

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> *const c_void;
    fn IOPSGetProvidingPowerSourceType(snapshot: *const c_void) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(object: *const c_void);
}

pub fn power() -> Power {
    flow_core::power::mac(source().as_deref(), low_power())
}

/// The source providing power: "AC Power", "Battery Power" or "UPS Power".
fn source() -> Option<String> {
    // SAFETY: takes nothing; the snapshot it copies is ours to release.
    let snapshot = unsafe { IOPSCopyPowerSourcesInfo() };
    if snapshot.is_null() {
        return None;
    }
    // SAFETY: the snapshot was just copied. The name is a CFString the
    // snapshot holds (not ours to release), read before the snapshot is
    // released, and CFString is toll-free bridged with NSString.
    let source = unsafe {
        let name = IOPSGetProvidingPowerSourceType(snapshot);
        (!name.is_null()).then(|| (*name.cast::<NSString>()).to_string())
    };
    // SAFETY: the one reference the copy gave us.
    unsafe { CFRelease(snapshot) };
    source
}

/// Low Power Mode is on (macOS 12 and later).
fn low_power() -> bool {
    let info = NSProcessInfo::processInfo();
    info.respondsToSelector(sel!(isLowPowerModeEnabled)) && info.isLowPowerModeEnabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_mac_says_how_its_powered() {
        // A desktop Mac or a laptop, plugged in or not: any answer (a
        // virtual Mac may have no source at all), but it must come, and
        // quickly enough to ask now and then.
        let started = std::time::Instant::now();
        let (power, source) = (power(), source());
        eprintln!("{power:?}, {source:?}, in {:?}", started.elapsed());
        assert!(source.as_deref().is_none_or(|s| s.ends_with(" Power")), "{source:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
