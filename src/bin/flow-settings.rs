//! `flow-settings`: Flow's settings seen and changed from the shell, on every
//! tab's PATH as the games and `flow-help` are. It asks Flow with OSC 7732
//! (`flow_sdk::settings`): `flow-settings` lists every setting a program may
//! see, `get <key>` reads one, `set <key> <value>` changes one, `reset <key>`
//! (or `reset all`) puts it back to its default, `reset terminal` starts the
//! tab afresh, and `restart` restarts Flow as Settings' Restart does. Flow
//! applies and saves each change, and says so; what a program may change
//! is Flow's to decide (`app.rs`'s `open_to_programs`), and so is what each
//! setting takes, which `--help` asks Flow for, so it's never out of date. It asks the tab's
//! terminal itself, not its standard input and output, so what it prints
//! can be piped, redirected or read by the shell's `$(…)`. Each request
//! carries the pane's own secret (`FLOW_TAB_KEY`) as its `pass`, without
//! which Flow doesn't answer at all, since anything printed in a tab can
//! ask as this does and its answer is typed into the shell's input. So
//! this says why itself: at once where there's no secret to send, and
//! where nothing answers, that the one it sent may be another pane's.

use std::process::ExitCode;
use std::time::Duration;

use flow_core::flow_command::TAB_KEY;
use flow_core::i18n::{self, t};
use serde_json::{json, Value};

/// How long Flow is given to answer.
const WAIT: Duration = Duration::from_secs(3);

fn main() -> ExitCode {
    // Flow's own language, which it gives its tabs, then the system's.
    let preferred: Vec<String> = ["FLOW_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).collect();
    i18n::set_language(i18n::AUTO, &preferred);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let (request, shows) = match words.as_slice() {
        [] | ["list"] => (json!({ "do": "list" }), Shows::List),
        ["get", key] => (json!({ "do": "get", "key": key }), Shows::Value),
        ["set", key, value @ ..] if !value.is_empty() => (json!({ "do": "set", "key": key, "value": value_of(&value.join(" ")) }), Shows::KeyValue),
        ["reset", "terminal"] => (json!({ "do": "reset-terminal" }), Shows::Nothing),
        ["reset", "all"] => (json!({ "do": "reset", "key": "all" }), Shows::AllReset),
        ["reset", key] => (json!({ "do": "reset", "key": key }), Shows::KeyValue),
        ["restart"] => (json!({ "do": "restart" }), Shows::Nothing),
        ["--help" | "-h" | "help"] => {
            println!("{}", t("flow_settings.usage"));
            print_takes();
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{}", t("flow_settings.usage"));
            return ExitCode::from(2);
        }
    };
    // Without the pane's secret Flow wouldn't answer, so the whole wait
    // would pass before saying why.
    let Some(pass) = pane_key() else {
        eprintln!("{}", t("flow_settings.no_key"));
        return ExitCode::from(2);
    };
    let answer = match flow_sdk::settings::ask(&with_pass(request, &pass).to_string(), WAIT) {
        Ok(Some(answer)) => answer,
        Ok(None) => {
            eprintln!("{}", t("flow_settings.no_answer"));
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("flow-settings: {error}");
            return ExitCode::from(2);
        }
    };
    let answer: Value = serde_json::from_str(&answer).unwrap_or_default();
    if answer["ok"] != Value::Bool(true) {
        eprintln!("flow-settings: {}", answer["error"].as_str().unwrap_or_default());
        return ExitCode::from(1);
    }
    match shows {
        Shows::List => {
            let settings = answer["settings"].as_object().cloned().unwrap_or_default();
            let mut keys: Vec<&String> = settings.keys().collect();
            keys.sort();
            for key in keys {
                println!("{key} = {}", settings[key]);
            }
        }
        Shows::Value => println!("{}", plain(&answer["value"])),
        Shows::KeyValue => println!("{} = {}", answer["key"].as_str().unwrap_or_default(), answer["value"]),
        Shows::AllReset => println!("{}", t("flow_settings.reset_all")),
        Shows::Nothing => {}
    }
    ExitCode::SUCCESS
}

/// What each setting takes, where Settings offers choices or a range, as
/// Flow says: asked only with the pane's secret, which only Flow's tabs
/// give, so help elsewhere doesn't wait for an answer that won't come.
fn print_takes() {
    let Some(pass) = pane_key() else { return };
    let asked = with_pass(json!({ "do": "describe" }), &pass);
    let Ok(Some(answer)) = flow_sdk::settings::ask(&asked.to_string(), Duration::from_secs(1)) else { return };
    let answer: Value = serde_json::from_str(&answer).unwrap_or_default();
    let Some(words) = answer["words"].as_object().filter(|w| !w.is_empty()) else { return };
    println!("\n{}", t("flow_settings.takes"));
    for line in takes_lines(words) {
        println!("{line}");
    }
}

/// Each setting and what it takes, its key padded so they line up.
fn takes_lines(words: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut keys: Vec<&String> = words.keys().collect();
    keys.sort();
    let width = keys.iter().map(|k| k.chars().count()).max().unwrap_or(0);
    keys.iter().map(|k| format!("  {k:width$}  {}", plain(&words[k.as_str()]))).collect()
}

/// The secret of the pane this runs in, which its shell was given; none
/// outside Flow's tabs, or where something on the way left it out (ssh,
/// sudo).
fn pane_key() -> Option<String> {
    std::env::var(TAB_KEY).ok().filter(|k| !k.is_empty())
}

/// `request` with the pane's secret `pass`, as Flow needs of every one.
fn with_pass(mut request: Value, pass: &str) -> Value {
    request["pass"] = Value::String(pass.to_string());
    request
}

/// What's printed of an answer.
enum Shows {
    List,
    Value,
    KeyValue,
    AllReset,
    Nothing,
}

/// A value as typed: JSON where it reads as JSON (16, true, ["a", "b"],
/// "quoted"), else the words as text (rain, #00ff00).
fn value_of(typed: &str) -> Value {
    serde_json::from_str(typed).unwrap_or_else(|_| Value::String(typed.to_string()))
}

/// A value as a script reads it: text bare, anything else as JSON.
fn plain(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_as_json_where_they_can_be_and_as_text_elsewhere() {
        assert_eq!(value_of("16"), json!(16));
        assert_eq!(value_of("true"), json!(true));
        assert_eq!(value_of(r#"["VT323", "Menlo"]"#), json!(["VT323", "Menlo"]));
        assert_eq!(value_of("rain"), json!("rain"));
        assert_eq!(value_of("#00ff00"), json!("#00ff00"));
        assert_eq!((plain(&json!("rain")), plain(&json!(0.5))), ("rain".to_string(), "0.5".to_string()));
    }

    #[test]
    fn every_request_carries_the_panes_key() {
        let asked = with_pass(json!({ "do": "set", "key": "fx.style", "value": "rain" }), &"k".repeat(32));
        assert_eq!(asked, json!({ "do": "set", "key": "fx.style", "value": "rain", "pass": "k".repeat(32) }));
        assert_eq!(with_pass(json!({ "do": "describe" }), "k"), json!({ "do": "describe", "pass": "k" }));
    }

    #[test]
    fn what_each_setting_takes_lines_up_under_its_key() {
        let words = json!({ "fx.style": "clouds or none", "font.size": "a number from 8 to 40" });
        assert_eq!(takes_lines(words.as_object().unwrap()), ["  font.size  a number from 8 to 40", "  fx.style   clouds or none"]);
    }
}
