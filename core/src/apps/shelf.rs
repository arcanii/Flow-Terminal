//! What Settings' Apps section shows of flowapp's apps: a line for each app
//! and package of themes installed or in the catalog, saying where it
//! stands (its version, a newer one, one waiting for a newer Flow, not
//! installed) and the buttons that change it (Install, Upgrade, Remove),
//! as `flowapp` would, and who signed it, as `flowapp info` says. Worked
//! out from what's on disk when Settings opens and after each change,
//! never as each frame is drawn.

use std::collections::BTreeMap;
use std::path::Path;

use super::table::{Shown, Signature, Signing};
use super::{signing, standing, Catalog, Installed, Standing};
use crate::i18n::{t, tf};

/// What a button does to an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doing {
    Install,
    Upgrade,
    Remove,
}

impl Doing {
    /// Its button's words.
    pub fn label(self) -> &'static str {
        t(match self {
            Doing::Install => "settings.apps.install",
            Doing::Upgrade => "settings.apps.upgrade",
            Doing::Remove => "settings.apps.remove",
        })
    }
}

/// An app's line in Settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub name: String,
    /// Where it stands, in words.
    pub text: String,
    /// Its buttons, in their order.
    pub acts: Vec<Doing>,
    /// Shown while it's chosen: what the catalog says it is, and who
    /// signed it.
    pub hint: String,
    /// It's a package of themes rather than an app.
    pub themes: bool,
    /// It's a package of skills rather than an app.
    pub skills: bool,
}

impl Line {
    /// Where it's listed: apps, then packages of themes, then of skills.
    fn rank(&self) -> u8 {
        if self.skills {
            2
        } else {
            self.themes as u8
        }
    }
}

/// Every app and package of themes `installed` or in `catalog`, apps first,
/// each by name, `busy` with what's being done to it meanwhile. `bundled`
/// is the folder of the programs Flow carries, as `flowapp` reads it.
pub fn lines(installed: &Installed, catalog: Option<&Catalog>, bundled: Option<&Path>, busy: &BTreeMap<String, Doing>) -> Vec<Line> {
    let mut names: Vec<&str> = installed.apps.keys().map(String::as_str).collect();
    if let Some(catalog) = catalog {
        names.extend(catalog.names().filter(|n| !installed.apps.contains_key(*n)));
    }
    let mut lines: Vec<Line> = names
        .into_iter()
        .map(|name| {
            let app = catalog.and_then(|c| c.get(name));
            let kept = installed.apps.get(name);
            let shown = Shown::new(name, app, standing(installed, name, app, bundled)).noting(kept, app);
            let themes = catalog.is_some_and(|c| c.is_themes(name)) || kept.is_some_and(|kept| kept.themes());
            let skills = catalog.is_some_and(|c| c.is_skills(name)) || kept.is_some_and(|kept| kept.skills());
            line(&shown, themes, skills, busy.get(name).copied())
        })
        .collect();
    lines.sort_by(|a, b| a.rank().cmp(&b.rank()).then(a.name.cmp(&b.name)));
    lines
}

/// `shown`'s line, with what's being done to it.
fn line(shown: &Shown, themes: bool, skills: bool, busy: Option<Doing>) -> Line {
    let latest = shown.latest.as_deref().unwrap_or_default();
    let needs = shown.needs_flow.as_deref().unwrap_or_default();
    let (mut text, acts) = match (busy, &shown.standing) {
        (Some(Doing::Remove), _) => (t("settings.apps.removing").to_string(), Vec::new()),
        (Some(_), _) => (t("settings.apps.installing").to_string(), Vec::new()),
        (None, Standing::Installed(have)) if shown.newer() && shown.held => {
            (tf("settings.apps.held", &[("version", have), ("latest", &latest), ("flow", &needs)]), vec![Doing::Remove])
        }
        (None, Standing::Installed(have)) if shown.newer() => (tf("settings.apps.newer", &[("version", have), ("latest", &latest)]), vec![Doing::Upgrade, Doing::Remove]),
        (None, Standing::Installed(have)) => (tf("settings.apps.installed", &[("version", have)]), vec![Doing::Remove]),
        (None, Standing::Bundled(have)) => (tf("settings.apps.bundled", &[("version", have)]), Vec::new()),
        (None, Standing::Absent) if shown.latest.is_none() => (t("settings.apps.not_here").to_string(), Vec::new()),
        (None, Standing::Absent) if shown.held => (tf("settings.apps.needs_flow", &[("latest", &latest), ("flow", &needs)]), Vec::new()),
        (None, Standing::Absent) => (tf("settings.apps.absent", &[("latest", &latest)]), vec![Doing::Install]),
    };
    let summary = if shown.summary.is_empty() { shown.name.clone() } else { shown.summary.clone() };
    let key = if themes {
        "settings.apps.hint_themes"
    } else if skills {
        "settings.apps.hint_skills"
    } else {
        "settings.apps.hint"
    };
    let hint = tf(key, &[("summary", &summary), ("name", &shown.name), ("signed", &signed_hint(shown))]).trim_end().to_string();
    if busy.is_none() {
        if let Some(mark) = off_mark(shown) {
            text = tf("settings.apps.marked", &[("text", &text), ("mark", &mark)]);
        }
    }
    Line { name: shown.name.clone(), text, acts, hint, themes, skills }
}

/// Who signed `shown`, in a sentence or two for its hint, as `flowapp
/// info` says it: what's installed as the install noted it (else the
/// catalog's word for that version), or the catalog's download where it
/// isn't installed. Empty where there's nothing to say.
fn signed_hint(shown: &Shown) -> String {
    let words = |key: &str, s: &Signature| {
        let who = s.by.clone().unwrap_or_else(|| t("flowapp.signer_unknown").to_string());
        let key = match s.on.is_empty() {
            true => format!("{key}_undated"),
            false => key.to_string(),
        };
        tf(&key, &[("who", &who), ("key", &signing::short(&s.key)), ("date", &s.on)])
    };
    match &shown.signing {
        Some(Signing::OwnFile) => t("settings.apps.signed_own").to_string(),
        Some(Signing::WithFlow) => t("settings.apps.signed_with_flow").to_string(),
        Some(Signing::Signed(s)) if s.unnoted => words("settings.apps.signed_unnoted", s),
        Some(Signing::Signed(s)) if s.checked.is_empty() => words("settings.apps.signed_installed", s),
        Some(Signing::Signed(s)) => tf("settings.apps.signed_and_checked", &[("signed", &words("settings.apps.signed_installed", s)), ("date", &s.checked)]),
        None if shown.standing.version().is_some() => t("settings.apps.signed_unknown").to_string(),
        None => shown.offered.as_ref().map(|s| words("settings.apps.signed_offered", s)).unwrap_or_default(),
    }
}

/// What a row adds where something's off about who signed what's
/// installed: a file of the user's own, signed by no one, or a key Flow
/// doesn't know. None where Flow signed it.
fn off_mark(shown: &Shown) -> Option<&'static str> {
    match &shown.signing {
        Some(Signing::OwnFile) => Some(t("settings.apps.off_not_signed")),
        Some(Signing::Signed(s)) if s.by.is_none() => Some(t("settings.apps.off_unknown_key")),
        _ => None,
    }
}

/// A shelf for snapshots, made up rather than read from this computer: one
/// app newer in the catalog, one up to date, one waiting for a newer Flow,
/// one not installed, Flow's machines as a package of themes, a package of
/// themes of the user's own, signed by no one, and Flow's own skills as a
/// package of skills, the catalog's signed by Flow.
pub fn made_up() -> Vec<Line> {
    let flows = signing::fingerprint(crate::UPDATE_PUBLIC_KEY).unwrap_or_default();
    let signature = |checked: &str| Signature { by: Some("Flow".into()), key: flows.clone(), on: "2026-11-18".into(), checked: checked.into(), ..Signature::default() };
    let shown = |name: &str, have: Option<&str>, latest: &str, needs: &str, summary: &str| Shown {
        name: name.into(),
        standing: have.map_or(Standing::Absent, |v| Standing::Installed(v.into())),
        latest: Some(latest.into()),
        size: Some(4_000_000),
        summary: summary.into(),
        programs: vec![name.into()],
        page: None,
        themes: false,
        skills: false,
        needs_flow: (!needs.is_empty()).then(|| needs.into()),
        held: !needs.is_empty(),
        offered: Some(signature("")),
        signing: have.map(|_| Signing::Signed(signature("2026-11-20"))),
    };
    let own = Shown { standing: Standing::Installed("1.0".into()), latest: None, signing: Some(Signing::OwnFile), offered: None, ..shown("my-themes", None, "", "", "My own themes") };
    let mut lines = vec![
        line(&shown("flow-invaders", Some("0.1.26"), "0.1.27", "", "Flow Invaders: invaders in waves, fought from a truck with a laser"), false, false, None),
        line(&shown("flow-mines", Some("0.1.9"), "0.1.9", "", "Flow Mines: the mine-finding puzzle"), false, false, None),
        line(&shown("flow-rogue", Some("0.1.45"), "0.1.46", "0.2.0", "Flow Rogue: a roguelike in Flow's tabs"), false, false, None),
        line(&shown("flow-sdk", None, "0.1.4", "", "Flow's SDK: its C library, header and Python package, with flow-demo"), false, false, None),
        line(&shown("flow-utils", None, "0.2.0", "", "Flow-Utils: Unix commands that draw in Flow's tabs"), false, false, None),
        line(&shown("flow-core-themes", None, "0.1.0", "", "Flow's ten machines as themes"), true, false, None),
        line(&own, true, false, None),
        line(&shown("flow-skills", Some("0.1.0"), "0.1.0", "", "Flow's own skills for its assistant: theme packages, the SDK, flowapp and flow-settings"), false, true, None),
    ];
    lines.sort_by(|a, b| a.rank().cmp(&b.rank()).then(a.name.cmp(&b.name)));
    lines
}

/// The apps and packages of themes `installed` that `catalog` has a newer
/// version of for this system, which this Flow can take: what a check says
/// can be upgraded. A package of the user's own of a catalog's name isn't
/// one, as `Shown::newer` says.
pub fn upgradable(installed: &Installed, catalog: &Catalog, bundled: Option<&Path>) -> Vec<String> {
    installed
        .apps
        .keys()
        .filter(|name| {
            let app = catalog.get(name);
            let shown = Shown::new(name, app, standing(installed, name, app, bundled)).noting(installed.apps.get(*name), app);
            shown.newer() && !shown.held
        })
        .cloned()
        .collect()
}

/// How long Flow waits between reading the catalog by itself, for
/// `apps.check` ("daily", "weekly"); none for "off" or anything else.
pub fn check_every(check: &str) -> Option<std::time::Duration> {
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    match check {
        "daily" => Some(day),
        "weekly" => Some(day * 7),
        _ => None,
    }
}

/// Whether Flow reads the catalog now: at a start where `at_start` says
/// so, or once the saved catalog is older than `apps.check` asks (`age`,
/// none where it was never read).
pub fn check_due(starting: bool, at_start: bool, check: &str, age: Option<std::time::Duration>) -> bool {
    (starting && at_start) || check_every(check).is_some_and(|every| age.is_none_or(|age| age >= every))
}

/// Whether a start is the first of a newer Flow than the one that `ran`
/// before (`updates.json`'s, empty where no Flow wrote it), with apps
/// installed that may have caught up with it: what the reminder to check
/// them is for. `older_flow`: a config was there before this start, so a
/// Flow from before `ran` was kept ran here.
pub fn first_start_of_newer_flow(ran: &str, this: &str, older_flow: bool, any_installed: bool) -> bool {
    any_installed && if ran.trim().is_empty() { older_flow } else { super::newer(this, ran) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Kept;

    fn catalog(flow_needed: &str) -> Catalog {
        let download = |version: &str, flow: &str| {
            format!(r#"{{"version":"{version}","url":"https://example.com/x.tar.gz","size":10,"sha256":"00","signature":"00","flow":"{flow}"}}"#)
        };
        let here = crate::apps::platform();
        Catalog::parse(&format!(
            r#"{{"format":1,"apps":{{
                "flow-rogue":{{"version":"0.1.46","summary":"A roguelike","programs":["flow-rogue"],"downloads":{{"{here}":{rogue}}}}},
                "flow-mines":{{"version":"0.1.9","summary":"Mines","programs":["flow-mines"],"downloads":{{"{here}":{mines}}}}},
                "flow-sdk":{{"version":"0.1.4","summary":"The SDK","programs":["flow-demo"],"downloads":{{"{here}":{sdk}}}}},
                "flow-far":{{"version":"1.0.0","summary":"Elsewhere only","programs":["far"],"downloads":{{"nowhere-cpu":{far}}}}}
            }},"themes":{{
                "flow-core-themes":{{"version":"0.1.0","summary":"Flow's machines","programs":[],"downloads":{{"any":{themes}}}}}
            }}}}"#,
            rogue = download("0.1.46", ""),
            mines = download("0.1.9", flow_needed),
            sdk = download("0.1.4", ""),
            far = download("1.0.0", ""),
            themes = download("0.1.0", ""),
        ))
        .unwrap()
    }

    fn installed(apps: &[(&str, &str, &str)]) -> Installed {
        let mut installed = Installed::default();
        for (name, version, kind) in apps {
            installed.apps.insert(name.to_string(), Kept { version: version.to_string(), programs: vec![name.to_string()], kind: kind.to_string(), ..Kept::default() });
        }
        installed
    }

    #[test]
    fn each_app_says_where_it_stands_and_what_can_be_done() {
        crate::i18n::set_language("en", &[]);
        let catalog = catalog("9.9.9");
        let installed = installed(&[("flow-rogue", "0.1.45", ""), ("flow-mines", "0.1.8", ""), ("flow-old", "0.0.1", "")]);
        let lines = lines(&installed, Some(&catalog), None, &BTreeMap::new());
        let of = |name: &str| lines.iter().find(|l| l.name == name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(of("flow-rogue").acts, [Doing::Upgrade, Doing::Remove], "{}", of("flow-rogue").text);
        assert!(of("flow-rogue").text.contains("0.1.45") && of("flow-rogue").text.contains("0.1.46"));
        assert_eq!(of("flow-mines").acts, [Doing::Remove], "a newer one waits for a newer Flow");
        assert!(of("flow-mines").text.contains("9.9.9"), "{}", of("flow-mines").text);
        assert_eq!(of("flow-sdk").acts, [Doing::Install]);
        assert!(of("flow-far").acts.is_empty(), "none for this system");
        assert_eq!(of("flow-old").acts, [Doing::Remove], "installed, though the catalog has forgotten it");
        assert!(of("flow-core-themes").themes && of("flow-core-themes").hint.contains("Flow's machines"));
        // Apps first, then packages of themes, each by name.
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["flow-far", "flow-mines", "flow-old", "flow-rogue", "flow-sdk", "flow-core-themes"]);
    }

    #[test]
    fn each_line_says_who_signed_it_and_marks_only_what_flow_didnt() {
        crate::i18n::set_language("en", &[]);
        let catalog = catalog("");
        let mut installed = installed(&[("flow-rogue", "0.1.46", ""), ("flow-mines", "0.1.9", ""), ("flow-gone", "0.0.1", ""), ("flow-odd", "1.0", ""), ("my-themes", "1.0", crate::apps::THEMES)]);
        let flows = signing::fingerprint(crate::UPDATE_PUBLIC_KEY).unwrap();
        let rogue = crate::apps::Download { signed: "2026-11-18".into(), ..catalog.get("flow-rogue").and_then(crate::apps::App::here).unwrap().clone() };
        let note = |kept: &mut Kept, key: &str| {
            kept.download = Some(rogue.clone());
            kept.key = key.into();
            kept.checked = "2026-11-20".into();
        };
        note(installed.apps.get_mut("flow-rogue").unwrap(), &flows);
        note(installed.apps.get_mut("flow-odd").unwrap(), &"ab".repeat(32));
        installed.apps.get_mut("my-themes").unwrap().own_file = true;
        let lines = lines(&installed, Some(&catalog), None, &BTreeMap::new());
        let of = |name: &str| lines.iter().find(|l| l.name == name).unwrap_or_else(|| panic!("{name}"));
        // As the install noted it, and no mark: Flow signed it.
        assert!(of("flow-rogue").hint.ends_with("Signed by Flow (key f186 6b49) on 2026-11-18. Checked with Flow's key as it was installed, 2026-11-20."), "{}", of("flow-rogue").hint);
        assert_eq!(of("flow-rogue").text, "0.1.46, up to date");
        // Nothing noted, so the catalog's word for the version installed.
        assert!(of("flow-mines").hint.ends_with("The catalog says Flow (key f186 6b49) signed this version. Nothing noted it as it was installed."), "{}", of("flow-mines").hint);
        assert_eq!(of("flow-mines").text, "0.1.9, up to date");
        assert!(of("flow-gone").hint.ends_with("Nothing says who signed it."), "{}", of("flow-gone").hint);
        // Not installed: who signed its download.
        assert!(of("flow-sdk").hint.ends_with("Its download is signed by Flow (key f186 6b49)."), "{}", of("flow-sdk").hint);
        // Off: a file of the user's own, and a key Flow doesn't know.
        assert_eq!(of("my-themes").text, "1.0, up to date · not signed");
        assert!(of("my-themes").hint.ends_with("Not signed: installed from a file of your own."), "{}", of("my-themes").hint);
        assert_eq!(of("flow-odd").text, "1.0, up to date · unknown key");
        assert!(of("flow-odd").hint.contains("Signed by an unknown key (key abab abab)"), "{}", of("flow-odd").hint);
        // No catalog download and nothing installed says nothing more.
        assert!(of("flow-far").hint.ends_with("flowapp info flow-far."), "{}", of("flow-far").hint);
    }

    #[test]
    fn a_package_of_skills_comes_last_and_says_where_its_skills_are_turned_on() {
        crate::i18n::set_language("en", &[]);
        let mut catalog = catalog("");
        let mut skills = crate::apps::App { summary: "Flow's own skills".into(), ..Default::default() };
        skills.downloads.insert(crate::apps::ANY.into(), crate::apps::Download { version: "0.1.0".into(), ..Default::default() });
        catalog.skills.insert("flow-skills".into(), skills);
        let installed = installed(&[("my-skills", "1.0", crate::apps::SKILLS)]);
        let lines = lines(&installed, Some(&catalog), None, &BTreeMap::new());
        let of = |name: &str| lines.iter().find(|l| l.name == name).unwrap_or_else(|| panic!("{name}"));
        assert!(of("flow-skills").skills && !of("flow-skills").themes && of("flow-skills").acts == [Doing::Install]);
        assert!(of("flow-skills").hint.contains("Skills from flow-skills"), "{}", of("flow-skills").hint);
        assert!(of("my-skills").skills, "one of the user's own, which the catalog doesn't know");
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(&names[names.len() - 3..], ["flow-core-themes", "flow-skills", "my-skills"], "apps, then themes, then skills");
    }

    #[test]
    fn an_app_being_changed_has_no_buttons_meanwhile() {
        crate::i18n::set_language("en", &[]);
        let catalog = catalog("");
        let installed = installed(&[("flow-rogue", "0.1.46", "")]);
        let busy = BTreeMap::from([("flow-rogue".to_string(), Doing::Remove), ("flow-sdk".to_string(), Doing::Install)]);
        let lines = lines(&installed, Some(&catalog), None, &busy);
        for name in ["flow-rogue", "flow-sdk"] {
            assert!(lines.iter().find(|l| l.name == name).unwrap().acts.is_empty(), "{name}");
        }
        // Without a catalog, what's installed is still listed.
        let alone = super::lines(&installed, None, None, &BTreeMap::new());
        assert_eq!((alone.len(), alone[0].acts.clone()), (1, vec![Doing::Remove]));
    }

    #[test]
    fn a_check_finds_what_can_be_upgraded_now() {
        let installed = installed(&[("flow-rogue", "0.1.45", ""), ("flow-mines", "0.1.8", ""), ("flow-core-themes", "0.1.0", "themes")]);
        assert_eq!(upgradable(&installed, &catalog("9.9.9"), None), ["flow-rogue"], "not the one waiting for a newer Flow");
        assert_eq!(upgradable(&installed, &catalog(""), None), ["flow-mines", "flow-rogue"]);
    }

    #[test]
    fn the_catalog_is_read_at_start_or_once_its_copy_is_old_enough() {
        let hours = |h: u64| Some(std::time::Duration::from_secs(h * 3600));
        assert!(check_due(true, true, "off", hours(1)), "at start, asked");
        assert!(!check_due(false, true, "off", hours(1)), "on focus, not for at-start");
        assert!(!check_due(true, false, "off", None), "never asked");
        assert!(check_due(false, false, "daily", hours(25)) && !check_due(false, false, "daily", hours(23)));
        assert!(check_due(false, false, "weekly", hours(24 * 7)) && !check_due(false, false, "weekly", hours(24 * 6)));
        assert!(check_due(false, false, "daily", None), "never read");
        assert!(!check_due(false, false, "hourly", None), "a word it doesn't know is off");
    }

    #[test]
    fn the_reminder_comes_at_the_first_start_of_a_newer_flow_with_apps_installed() {
        assert!(first_start_of_newer_flow("0.1.34", "0.1.35", false, true));
        assert!(!first_start_of_newer_flow("0.1.35", "0.1.35", true, true), "the same Flow again");
        assert!(!first_start_of_newer_flow("0.1.36", "0.1.35", true, true), "an older one");
        assert!(!first_start_of_newer_flow("0.1.34", "0.1.35", true, false), "no apps to check");
        assert!(first_start_of_newer_flow("", "0.1.35", true, true), "from before Flow kept what ran, with a config");
        assert!(!first_start_of_newer_flow("", "0.1.35", false, true), "a first start ever");
    }
}
