//! How the computer is powered, for efficient mode's setting that follows
//! it (`ui.efficient_on_battery`): on battery, or saving power while
//! plugged in (the Mac's Low Power Mode, Windows' energy saver). Each OS
//! crate reads its own (`Platform::power`) on a thread of its own; what
//! they read is turned into a [`Power`] here, Linux's folder of power
//! supplies read here too, so all of it is tested on any OS.

use std::fs;
use std::path::Path;

/// How the computer is powered, as far as efficient mode cares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Power {
    /// Plugged in, or nothing says otherwise (a desktop, a system that
    /// doesn't tell).
    #[default]
    Plugged,
    /// Running on its battery.
    Battery,
    /// Plugged in, or on a battery the system doesn't name, but saving
    /// power: the Mac's Low Power Mode, Windows' energy saver.
    LowPower,
}

impl Power {
    /// On battery or saving power: what efficient mode follows.
    pub fn saving(self) -> bool {
        self != Power::Plugged
    }
}

/// The Mac's: the source IOKit says is providing power ("AC Power",
/// "Battery Power", "UPS Power") and whether Low Power Mode is on.
pub fn mac(source: Option<&str>, low_power: bool) -> Power {
    match source {
        Some("Battery Power") => Power::Battery,
        _ if low_power => Power::LowPower,
        _ => Power::Plugged,
    }
}

/// Windows': `GetSystemPowerStatus`'s line status (0 offline, 1 online,
/// 255 unknown) and its system status flag (1 while energy saver is on).
pub fn windows(ac_line_status: u8, system_status_flag: u8) -> Power {
    match (ac_line_status, system_status_flag) {
        (0, _) => Power::Battery,
        (_, 1) => Power::LowPower,
        _ => Power::Plugged,
    }
}

/// Linux's, from the power supplies under `<root>/sys/class/power_supply`:
/// plugged in while a charger the system draws on (mains, USB) is online;
/// else on battery while one of the system's batteries is discharging.
/// A battery of a device's own (a wireless mouse's, `scope` "Device")
/// says nothing of the computer's, and a folder that can't be read is
/// plugged in.
pub fn linux(root: &Path) -> Power {
    let Ok(entries) = fs::read_dir(root.join("sys/class/power_supply")) else { return Power::Plugged };
    let mut discharging = false;
    for entry in entries.flatten() {
        let dir = entry.path();
        let read = |name: &str| fs::read_to_string(dir.join(name)).map(|s| s.trim().to_string()).unwrap_or_default();
        if read("scope") == "Device" {
            continue;
        }
        match read("type").as_str() {
            "Battery" => discharging |= read("status") == "Discharging",
            // A UPS keeps a desktop going on a battery that isn't the
            // computer's, which it says nothing of here.
            "UPS" => {}
            _ if read("online") == "1" => return Power::Plugged,
            _ => {}
        }
    }
    if discharging {
        Power::Battery
    } else {
        Power::Plugged
    }
}

/// Efficient mode's part that follows the power: how the computer was
/// last read to be powered, and whether the user turned efficient mode
/// off while that kept it on, which holds until the computer stops saving
/// power (plugged in, Low Power Mode or energy saver off) or Flow quits:
/// it isn't saved.
#[derive(Debug, Default)]
pub struct Follow {
    power: Power,
    waived: bool,
}

impl Follow {
    /// How the computer was last read to be powered.
    pub fn power(&self) -> Power {
        self.power
    }

    /// What keeps efficient mode on beside the user's own switch: the
    /// power, while the setting follows it (`follow`), the computer saves
    /// power, and the user hasn't turned efficient mode off since.
    pub fn keeps_on(&self, follow: bool) -> Option<Power> {
        (follow && self.power.saving() && !self.waived).then_some(self.power)
    }

    /// The power read anew; true if it changed. Plugged in, the user's
    /// turning efficient mode off is forgotten.
    pub fn read(&mut self, power: Power) -> bool {
        if !power.saving() {
            self.waived = false;
        }
        std::mem::replace(&mut self.power, power) != power
    }

    /// The user turned efficient mode `on`, or off; off while the power
    /// alone kept it on, it stays off until the computer stops saving power
    /// or Flow quits (it isn't saved). Off while their own switch (`own`)
    /// had it on, only that goes, and the power keeps it on if it would:
    /// off again, it's waived.
    pub fn switched(&mut self, on: bool, own: bool, follow: bool) {
        self.waived = !on && !own && self.keeps_on(follow).is_some();
    }

    /// The setting turned on or off: what's known of the power is
    /// forgotten (it's read again while the setting is on, and not at
    /// all while it's off), and with it the user's turning it off.
    pub fn reset(&mut self) {
        *self = Follow::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_system_says_battery_low_power_or_plugged_in() {
        assert_eq!(mac(Some("Battery Power"), false), Power::Battery);
        assert_eq!(mac(Some("Battery Power"), true), Power::Battery);
        assert_eq!(mac(Some("AC Power"), true), Power::LowPower);
        assert_eq!(mac(Some("AC Power"), false), Power::Plugged);
        assert_eq!(mac(Some("UPS Power"), false), Power::Plugged);
        assert_eq!(mac(None, false), Power::Plugged);
        assert_eq!(windows(0, 0), Power::Battery);
        assert_eq!(windows(0, 1), Power::Battery);
        assert_eq!(windows(1, 1), Power::LowPower);
        assert_eq!(windows(1, 0), Power::Plugged);
        assert_eq!(windows(255, 0), Power::Plugged, "unknown is plugged in");
    }

    /// A made-up root folder with power supplies, each a folder of files.
    struct Supplies(std::path::PathBuf);

    impl Supplies {
        fn new(name: &str) -> Supplies {
            let dir = std::env::temp_dir().join(format!("flow-power-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("sys/class/power_supply")).unwrap();
            Supplies(dir)
        }

        fn supply(&self, name: &str, files: &[(&str, &str)]) {
            let dir = self.0.join("sys/class/power_supply").join(name);
            fs::create_dir_all(&dir).unwrap();
            for (file, text) in files {
                fs::write(dir.join(file), format!("{text}\n")).unwrap();
            }
        }
    }

    impl Drop for Supplies {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_linux_laptop_is_on_battery_while_its_battery_discharges_and_no_charger_is_online() {
        let laptop = Supplies::new("laptop");
        laptop.supply("AC", &[("type", "Mains"), ("online", "0")]);
        laptop.supply("BAT0", &[("type", "Battery"), ("status", "Discharging"), ("scope", "System")]);
        // A wireless mouse's battery, charging, says nothing of the laptop's.
        laptop.supply("hidpp_battery_0", &[("type", "Battery"), ("status", "Charging"), ("scope", "Device"), ("online", "1")]);
        assert_eq!(linux(&laptop.0), Power::Battery);
        // Plugged in, though a busy laptop may still draw on its battery.
        laptop.supply("AC", &[("type", "Mains"), ("online", "1")]);
        assert_eq!(linux(&laptop.0), Power::Plugged);
        // Charged over USB-C, with no mains supply at all.
        let usb = Supplies::new("usb");
        // Linux names it ucsi-source-psy-USBC000:001, which Windows can't
        // make a folder of; only its type matters.
        let charger = "ucsi-source-psy-USBC000-001";
        usb.supply("BAT1", &[("type", "Battery"), ("status", "Not charging")]);
        usb.supply(charger, &[("type", "USB"), ("online", "0")]);
        assert_eq!(linux(&usb.0), Power::Plugged, "a battery held at its charge limit isn't discharging");
        usb.supply("BAT1", &[("type", "Battery"), ("status", "Discharging")]);
        assert_eq!(linux(&usb.0), Power::Battery);
        usb.supply(charger, &[("type", "USB"), ("online", "1")]);
        assert_eq!(linux(&usb.0), Power::Plugged);
    }

    #[test]
    fn a_desktop_or_a_folder_that_isnt_there_is_plugged_in() {
        let desktop = Supplies::new("desktop");
        assert_eq!(linux(&desktop.0), Power::Plugged);
        // A UPS running on its battery isn't the computer's battery.
        desktop.supply("ups", &[("type", "UPS"), ("online", "0"), ("status", "Discharging")]);
        assert_eq!(linux(&desktop.0), Power::Plugged);
        assert_eq!(linux(Path::new("/no/such/root")), Power::Plugged);
    }

    #[test]
    fn efficient_mode_follows_the_battery_until_the_user_turns_it_off_till_plugging_in() {
        let mut follow = Follow::default();
        assert_eq!(follow.keeps_on(true), None, "plugged in, as nothing says otherwise");
        assert!(follow.read(Power::Battery));
        assert_eq!(follow.keeps_on(true), Some(Power::Battery));
        assert_eq!(follow.keeps_on(false), None, "only while the setting says so");
        assert!(!follow.read(Power::Battery), "the same again changes nothing");
        // Turned off by hand, it stays off, through Low Power Mode too.
        follow.switched(false, false, true);
        assert_eq!(follow.keeps_on(true), None);
        follow.read(Power::LowPower);
        assert_eq!(follow.keeps_on(true), None);
        // Plugged in, and on battery again later, it's on again.
        follow.read(Power::Plugged);
        follow.read(Power::Battery);
        assert_eq!(follow.keeps_on(true), Some(Power::Battery));
        // Turned on by hand, it's the user's own switch that keeps it on.
        follow.switched(true, false, true);
        assert_eq!(follow.keeps_on(true), Some(Power::Battery));
        // Their own switch off on battery leaves it on for the battery,
        // and off again it's waived.
        follow.switched(false, true, true);
        assert_eq!(follow.keeps_on(true), Some(Power::Battery));
        follow.switched(false, false, true);
        assert_eq!(follow.keeps_on(true), None);
        follow.read(Power::Plugged);
        follow.read(Power::Battery);
        // Turned off while plugged in, nothing's waived.
        follow.read(Power::Plugged);
        follow.switched(false, true, true);
        follow.read(Power::LowPower);
        assert_eq!(follow.keeps_on(true), Some(Power::LowPower));
        follow.reset();
        assert_eq!((follow.power(), follow.keeps_on(true)), (Power::Plugged, None));
    }
}
