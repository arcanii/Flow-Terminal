//! The system's notification when a long command finishes while Flow isn't
//! in front (`ui.notify_after`): whether one is due, and what it says. The
//! OS crates show it (`Platform::notify`): the Mac's Notification Center,
//! whose click brings Flow and the tab forward; Windows' taskbar button
//! flashing; a desktop notification on Linux.

use crate::i18n::tf;
use crate::session::Finished;

/// What Settings offers, in seconds: off (0), then 10 s, 30 s, 1 min and
/// 5 min.
pub const AFTER: [u32; 5] = [0, 10, 30, 60, 300];

/// Whether `finished` is worth a notification: it ran at least `after`
/// seconds (0 is never), Flow wasn't in front as it ended (`focused`), and
/// it wasn't stopped with Ctrl+C (exit code 130), which whoever stopped it
/// knows of.
pub fn wanted(finished: &Finished, focused: bool, after: u32) -> bool {
    after > 0 && !focused && finished.exit != Some(130) && finished.secs >= after as f32
}

/// How much of a command line a notification shows, in characters, cut
/// with … past it.
const COMMAND_SHOWN: usize = 100;

/// The notification's words: the command run, on a line of its own where
/// the shell's marks said what it was, and then that it finished, or
/// failed and with which exit code, and how long it took. A shell that
/// reports no exit code (cmd) finished. A command that looks as if it
/// holds a secret (`history::looks_secret`, which keeps it out of the
/// history too) isn't shown: notifications stay in Notification Center
/// and may show on a locked screen.
pub fn body(finished: &Finished) -> String {
    let took = took(finished.secs);
    let how = match finished.exit {
        Some(code) if code != 0 => tf("notify.failed", &[("code", &code), ("took", &took)]),
        _ => tf("notify.finished", &[("took", &took)]),
    };
    let command = finished.command.as_deref().map(str::trim).filter(|c| !c.is_empty() && !crate::history::looks_secret(c));
    match command {
        Some(command) => {
            let mut shown: String = command.chars().take(COMMAND_SHOWN).collect();
            if command.chars().count() > COMMAND_SHOWN {
                shown.push('…');
            }
            format!("{shown}\n{how}")
        }
        None => how,
    }
}

/// How long `secs` is, in words, to the second under an hour and to the
/// minute past it: "42 s", "3 min 5 s", "5 min", "1 h 12 min".
pub fn took(secs: f32) -> String {
    let secs = secs.max(0.0).round() as u64;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    match (h, m, s) {
        (0, 0, s) => tf("notify.seconds", &[("s", &s)]),
        (0, m, 0) => tf("notify.minutes", &[("m", &m)]),
        (0, m, s) => tf("notify.minutes_seconds", &[("m", &m), ("s", &s)]),
        (h, 0, _) => tf("notify.hours", &[("h", &h)]),
        (h, m, _) => tf("notify.hours_minutes", &[("h", &h), ("m", &m)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ran(secs: f32, exit: Option<i32>) -> Finished {
        Finished { exit, secs, command: None }
    }

    #[test]
    fn a_long_command_done_while_flow_is_behind_notifies() {
        assert!(wanted(&ran(30.0, Some(0)), false, 30));
        assert!(wanted(&ran(400.0, Some(1)), false, 300));
        // cmd says nothing of how it ended.
        assert!(wanted(&ran(12.0, None), false, 10));
    }

    #[test]
    fn short_seen_stopped_or_switched_off_commands_dont() {
        assert!(!wanted(&ran(29.9, Some(0)), false, 30), "under the threshold");
        assert!(!wanted(&ran(600.0, Some(0)), true, 30), "Flow in front");
        assert!(!wanted(&ran(600.0, Some(130)), false, 30), "stopped with Ctrl+C");
        assert!(!wanted(&ran(600.0, Some(0)), false, 0), "off");
    }

    #[test]
    fn the_notification_says_how_it_ended_and_how_long_it_took() {
        crate::i18n::set_language("en", &[]);
        assert_eq!(body(&ran(42.0, Some(0))), "Finished after 42 s");
        assert_eq!(body(&ran(185.0, None)), "Finished after 3 min 5 s");
        assert_eq!(body(&ran(65.0, Some(2))), "Failed with exit code 2 after 1 min 5 s");
        assert_eq!(took(300.2), "5 min");
        assert_eq!(took(3600.0 + 12.0 * 60.0 + 30.0), "1 h 12 min");
        assert_eq!(took(7200.0), "2 h");
    }

    #[test]
    fn the_command_comes_first_cut_short_and_never_with_a_secret() {
        crate::i18n::set_language("en", &[]);
        let with = |command: &str| Finished { command: Some(command.into()), ..ran(42.0, Some(0)) };
        assert_eq!(body(&with("cargo build --release ")), "cargo build --release\nFinished after 42 s");
        let long = "x".repeat(150);
        assert_eq!(body(&with(&long)), format!("{}…\nFinished after 42 s", "x".repeat(100)));
        assert_eq!(body(&with("API_KEY=sk-0123456789abcdefghij ./deploy")), "Finished after 42 s");
        assert_eq!(body(&with("  ")), "Finished after 42 s");
    }

    #[test]
    fn every_choice_settings_offers_but_off_notifies() {
        for &after in &AFTER[1..] {
            assert!(wanted(&ran(after as f32, Some(0)), false, after));
        }
    }
}
