//! Running total of the tokens the assistant has used, kept on disk so it
//! survives restarts, plus the formatting the UI shows it with.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub requests: u64,
}

impl Usage {
    /// Every token the API reported, cached ones included.
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }

    pub fn add(&mut self, other: &Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_write += other.cache_write;
        self.cache_read += other.cache_read;
        self.requests += other.requests;
    }

    /// Overwrites the fields present in an API `usage` object. Stream events
    /// report cumulative counts for the message, so later values win.
    pub fn update_from_api(&mut self, v: &Value) {
        let set = |field: &mut u64, key: &str| {
            if let Some(n) = v.get(key).and_then(Value::as_u64) {
                *field = n;
            }
        };
        set(&mut self.input, "input_tokens");
        set(&mut self.output, "output_tokens");
        set(&mut self.cache_write, "cache_creation_input_tokens");
        set(&mut self.cache_read, "cache_read_input_tokens");
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    /// Unix seconds when counting (re)started.
    since: u64,
    usage: Usage,
}

/// Shared between the UI and the agent thread.
pub struct Meter {
    state: Mutex<Stored>,
    path: Option<PathBuf>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Meter {
    /// Loads `usage.json` from the config folder (or starts at zero).
    pub fn load() -> Arc<Self> {
        let path = crate::config::config_dir().join("usage.json");
        let state = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Stored>(&t).ok())
            .unwrap_or(Stored { since: now_secs(), usage: Usage::default() });
        Arc::new(Self { state: Mutex::new(state), path: Some(path) })
    }

    /// The meter of a snapshot or a recording: a few thousand tokens, made
    /// up and the same each time, neither read from the disk nor written to
    /// it, so the pictures never show what whoever makes them has used.
    pub fn made_up() -> Arc<Self> {
        let usage = Usage { input: 3_140, output: 1_065, cache_write: 0, cache_read: 0, requests: 6 };
        Arc::new(Self { state: Mutex::new(Stored { since: now_secs(), usage }), path: None })
    }

    /// A meter that never touches the disk (tests).
    #[cfg(test)]
    pub fn in_memory() -> Arc<Self> {
        Arc::new(Self { state: Mutex::new(Stored { since: now_secs(), usage: Usage::default() }), path: None })
    }

    pub fn add(&self, usage: &Usage) {
        let snapshot = {
            let mut st = self.state.lock();
            st.usage.add(usage);
            *st
        };
        self.save(&snapshot);
    }

    pub fn usage(&self) -> Usage {
        self.state.lock().usage
    }

    pub fn since(&self) -> u64 {
        self.state.lock().since
    }

    pub fn clear(&self) {
        let snapshot = {
            let mut st = self.state.lock();
            *st = Stored { since: now_secs(), usage: Usage::default() };
            *st
        };
        self.save(&snapshot);
    }

    fn save(&self, state: &Stored) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(state) {
            if let Err(err) = std::fs::write(path, text) {
                log::warn!("could not save token usage: {err}");
            }
        }
    }
}

/// 950, 12.3k, 250k, 1.25M
pub fn fmt_tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=99_999 => trim(format!("{:.1}", n as f64 / 1_000.0)) + "k",
        100_000..=999_999 => format!("{}k", n / 1_000),
        _ => trim(format!("{:.2}", n as f64 / 1_000_000.0)) + "M",
    }
}

fn trim(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// "Oct 21" for a Unix timestamp (UTC).
pub fn fmt_date(secs: u64) -> String {
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let mon = crate::i18n::t(&format!("date.mon.{month}"));
    crate::i18n::tf("date.short", &[("mon", &mon), ("day", &day)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_snapshots_meter_shows_a_few_thousand_made_up_tokens_and_keeps_none() {
        let meter = Meter::made_up();
        assert_eq!(fmt_tokens(meter.usage().total()), "4.2k");
        assert_eq!(meter.usage(), Meter::made_up().usage(), "the same each time");
        // No file to read or write: what the assistant would add stays in it.
        assert!(meter.path.is_none());
        meter.add(&Usage { input: 800, ..Usage::default() });
        assert_eq!(fmt_tokens(meter.usage().total()), "5k");
    }

    #[test]
    fn token_formatting() {
        assert_eq!(fmt_tokens(950), "950");
        assert_eq!(fmt_tokens(12_345), "12.3k");
        assert_eq!(fmt_tokens(20_000), "20k");
        assert_eq!(fmt_tokens(250_400), "250k");
        assert_eq!(fmt_tokens(1_250_000), "1.25M");
        assert_eq!(fmt_tokens(2_000_000), "2M");
    }

    #[test]
    fn dates() {
        assert_eq!(fmt_date(0), "Jan 1");
        // 2026-10-21 09:00 UTC
        assert_eq!(fmt_date(1_792_573_200), "Oct 21");
    }

    #[test]
    fn later_stream_counts_win() {
        let mut u = Usage::default();
        u.update_from_api(&json!({"input_tokens": 120, "cache_read_input_tokens": 900, "output_tokens": 1}));
        u.update_from_api(&json!({"output_tokens": 57}));
        assert_eq!(u, Usage { input: 120, output: 57, cache_write: 0, cache_read: 900, requests: 0 });
        assert_eq!(u.total(), 1077);
    }

    #[test]
    fn meter_adds_and_clears() {
        let m = Meter::in_memory();
        m.add(&Usage { input: 10, output: 5, requests: 1, ..Default::default() });
        m.add(&Usage { input: 1, output: 1, requests: 1, ..Default::default() });
        assert_eq!(m.usage().total(), 17);
        assert_eq!(m.usage().requests, 2);
        m.clear();
        assert_eq!(m.usage().total(), 0);
    }
}
