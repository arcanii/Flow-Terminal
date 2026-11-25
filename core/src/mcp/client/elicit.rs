//! A server asking the user something (MCP's elicitation): a form of
//! fields read from its `requestedSchema` (flat, of strings, numbers,
//! switches and choices, as the spec allows), checked as the schema says
//! before anything goes back, or a link to open in the browser, whose
//! host is shown and which Flow never fetches itself.

use serde_json::{json, Map, Value};

use crate::i18n::{t, tf};

/// The error a request fails with where the user must open links first.
pub const LINKS_NEEDED: i64 = -32042;

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    Form(Form),
    Link(LinkAsk),
}

impl Ask {
    /// An `elicitation/create` request's params, or why Flow refuses it.
    pub fn read(params: &Value) -> Result<Ask, String> {
        match params.get("mode").and_then(Value::as_str).unwrap_or("form") {
            "form" => Form::read(params).map(Ask::Form),
            "url" => LinkAsk::read(params).map(Ask::Link),
            other => Err(format!("Flow doesn't take questions of mode {other}")),
        }
    }
}

/// A link the server asks the user to open.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkAsk {
    pub message: String,
    pub url: String,
    /// Its `elicitationId`, which the server says is done with.
    pub id: String,
}

impl LinkAsk {
    /// The link a server's `params` ask to open, where it's a web address
    /// written as one (`link::web_address`), which the system's opener
    /// takes for nothing more.
    pub fn read(params: &Value) -> Result<LinkAsk, String> {
        let get = |k: &str| params.get(k).and_then(Value::as_str).map(String::from);
        let url = get("url").filter(|u| (u.starts_with("https://") || u.starts_with("http://")) && crate::link::web_address(u)).ok_or("its url isn't a web address")?;
        Ok(LinkAsk { message: get("message").unwrap_or_default(), url, id: get("elicitationId").unwrap_or_default() })
    }

    /// The link around its host, so the host can be lit: what's before it,
    /// the host, and what's after.
    pub fn around_host(&self) -> (&str, &str, &str) {
        let start = self.url.find("://").map_or(0, |i| i + 3);
        let rest = &self.url[start..];
        let end = start + rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &self.url[start..end];
        let host_start = start + authority.rfind('@').map_or(0, |i| i + 1);
        let host_end = match self.url[host_start..end].strip_prefix('[') {
            Some(_) => host_start + self.url[host_start..end].find(']').map_or(end - host_start, |i| i + 1),
            None => host_start + self.url[host_start..end].find(':').unwrap_or(end - host_start),
        };
        (&self.url[..host_start], &self.url[host_start..host_end], &self.url[host_end..])
    }

    /// What to warn of before it's opened: a host that may pass for
    /// another (punycode, or letters past ASCII), or no HTTPS.
    pub fn warning(&self) -> Option<String> {
        let (_, host, _) = self.around_host();
        if host.split('.').any(|label| label.to_ascii_lowercase().starts_with("xn--")) || !host.is_ascii() {
            return Some(t("server_ask.warn_lookalike").to_string());
        }
        if !self.url.starts_with("https://") {
            return Some(t("server_ask.warn_plain").to_string());
        }
        None
    }
}

/// A form to fill in.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub message: String,
    pub fields: Vec<Field>,
    /// Each field's answer to begin with: its default where it has one.
    pub start: Vec<Answer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: String,
    pub title: String,
    pub description: String,
    pub required: bool,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Text { min: Option<usize>, max: Option<usize>, pattern: Option<String>, format: Option<Format> },
    Number { integer: bool, min: Option<f64>, max: Option<f64> },
    Switch,
    /// One of these.
    One(Vec<Choice>),
    /// Any of these, as many as it says.
    Many { options: Vec<Choice>, min: Option<usize>, max: Option<usize> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub value: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Email,
    Uri,
    Date,
    DateTime,
}

/// What the user has put in a field: a number is typed as text.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Text(String),
    Switch(bool),
    One(Option<usize>),
    Many(Vec<bool>),
}

fn choices(list: &[Value], value_key: &str) -> Vec<Choice> {
    list.iter()
        .filter_map(|v| match v {
            Value::String(s) => Some(Choice { value: s.clone(), title: s.clone() }),
            Value::Object(o) => {
                let value = o.get(value_key).and_then(Value::as_str)?.to_string();
                let title = o.get("title").and_then(Value::as_str).map(String::from).unwrap_or_else(|| value.clone());
                Some(Choice { value, title })
            }
            _ => None,
        })
        .collect()
}

impl Form {
    pub fn read(params: &Value) -> Result<Form, String> {
        let schema = params.get("requestedSchema").ok_or("it has no requestedSchema")?;
        let properties = schema.get("properties").and_then(Value::as_object).ok_or("its requestedSchema has no properties")?;
        let required: Vec<&str> = schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let mut fields = Vec::new();
        for (key, p) in properties {
            let num = |k: &str| p.get(k).and_then(Value::as_f64);
            let count = |k: &str| p.get(k).and_then(Value::as_u64).map(|n| n as usize);
            let kind = match p.get("type").and_then(Value::as_str).unwrap_or("string") {
                "string" if p.get("enum").is_some() => Kind::One(choices(p["enum"].as_array().map_or(&[], Vec::as_slice), "const")),
                "string" if p.get("oneOf").is_some() => Kind::One(choices(p["oneOf"].as_array().map_or(&[], Vec::as_slice), "const")),
                "string" => Kind::Text {
                    min: count("minLength"),
                    max: count("maxLength"),
                    pattern: p.get("pattern").and_then(Value::as_str).map(String::from),
                    format: match p.get("format").and_then(Value::as_str) {
                        Some("email") => Some(Format::Email),
                        Some("uri") => Some(Format::Uri),
                        Some("date") => Some(Format::Date),
                        Some("date-time") => Some(Format::DateTime),
                        _ => None,
                    },
                },
                kind @ ("number" | "integer") => Kind::Number { integer: kind == "integer", min: num("minimum"), max: num("maximum") },
                "boolean" => Kind::Switch,
                "array" => {
                    let items = p.get("items").cloned().unwrap_or(Value::Null);
                    let options = match (items.get("enum"), items.get("anyOf").or_else(|| items.get("oneOf"))) {
                        (Some(Value::Array(list)), _) => choices(list, "const"),
                        (_, Some(Value::Array(list))) => choices(list, "const"),
                        _ => return Err(format!("its field {key} is a list of something other than choices")),
                    };
                    Kind::Many { options, min: count("minItems"), max: count("maxItems") }
                }
                other => return Err(format!("its field {key} is of type {other}, which a form can't ask")),
            };
            if matches!(&kind, Kind::One(o) | Kind::Many { options: o, .. } if o.is_empty()) {
                return Err(format!("its field {key} has no choices"));
            }
            let text = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let title = Some(text("title")).filter(|t| !t.is_empty()).unwrap_or_else(|| key.clone());
            fields.push(Field { key: key.clone(), title, description: text("description"), required: required.contains(&key.as_str()), kind });
        }
        let start = defaults(&fields, params);
        Ok(Form { message: params.get("message").and_then(Value::as_str).unwrap_or_default().to_string(), fields, start })
    }

    /// A form of `fields` asked with `message`, each empty to begin with.
    pub fn plain(message: String, fields: Vec<Field>) -> Form {
        let start = defaults(&fields, &Value::Null);
        Form { message, fields, start }
    }
}

/// Each field's answer to begin with: its default in `params`' schema
/// where it has one.
fn defaults(fields: &[Field], params: &Value) -> Vec<Answer> {
    let props = params.pointer("/requestedSchema/properties");
    fields
            .iter()
            .map(|f| {
                let default = props.and_then(|p| p.get(&f.key)).and_then(|p| p.get("default"));
                match &f.kind {
                    Kind::Text { .. } => Answer::Text(default.and_then(Value::as_str).unwrap_or_default().to_string()),
                    Kind::Number { .. } => Answer::Text(default.filter(|d| d.is_number()).map(|d| d.to_string()).unwrap_or_default()),
                    Kind::Switch => Answer::Switch(default.and_then(Value::as_bool).unwrap_or(false)),
                    Kind::One(options) => Answer::One(default.and_then(Value::as_str).and_then(|d| options.iter().position(|o| o.value == d))),
                    Kind::Many { options, .. } => {
                        let picked: Vec<&str> = default.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                        Answer::Many(options.iter().map(|o| picked.contains(&o.value.as_str())).collect())
                    }
                }
            })
            .collect()
}

impl Form {
    /// The content to send, or each field's problem (by its place), in the
    /// user's words.
    pub fn content(&self, answers: &[Answer]) -> Result<Value, Vec<(usize, String)>> {
        let mut content = Map::new();
        let mut problems = Vec::new();
        for (i, (field, answer)) in self.fields.iter().zip(answers).enumerate() {
            match check(field, answer) {
                Ok(Some(value)) => {
                    content.insert(field.key.clone(), value);
                }
                Ok(None) => {}
                Err(why) => problems.push((i, why)),
            }
        }
        if problems.is_empty() {
            Ok(Value::Object(content))
        } else {
            Err(problems)
        }
    }
}

/// A field's answer as it goes back, none for one left empty, or what's
/// wrong with it.
fn check(field: &Field, answer: &Answer) -> Result<Option<Value>, String> {
    let empty = match answer {
        Answer::Text(text) => text.trim().is_empty(),
        Answer::One(picked) => picked.is_none(),
        Answer::Switch(_) | Answer::Many(_) => false,
    };
    if empty {
        return if field.required { Err(t("server_ask.required").to_string()) } else { Ok(None) };
    }
    match (&field.kind, answer) {
        (Kind::Text { min, max, pattern, format }, Answer::Text(text)) => {
            let n = text.chars().count();
            if min.is_some_and(|m| n < m) {
                return Err(tf("server_ask.too_short", &[("n", &min.unwrap_or(0))]));
            }
            if max.is_some_and(|m| n > m) {
                return Err(tf("server_ask.too_long", &[("n", &max.unwrap_or(0))]));
            }
            if let Some(format) = format {
                if !formatted(text, *format) {
                    return Err(t(match format {
                        Format::Email => "server_ask.not_email",
                        Format::Uri => "server_ask.not_uri",
                        Format::Date => "server_ask.not_date",
                        Format::DateTime => "server_ask.not_date_time",
                    })
                    .to_string());
                }
            }
            // A pattern Rust's regex can't read is passed over rather than
            // keeping the user from answering.
            if let Some(re) = pattern.as_deref().and_then(|p| regex::Regex::new(p).ok()) {
                if !re.is_match(text) {
                    return Err(t("server_ask.not_pattern").to_string());
                }
            }
            Ok(Some(json!(text)))
        }
        (Kind::Number { integer, min, max }, Answer::Text(text)) => {
            let Ok(n) = text.trim().parse::<f64>() else { return Err(t("server_ask.not_number").to_string()) };
            if *integer && n.fract() != 0.0 {
                return Err(t("server_ask.not_integer").to_string());
            }
            if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                let shown = |v: Option<f64>| v.map_or("…".to_string(), |v| v.to_string());
                return Err(tf("server_ask.out_of_range", &[("min", &shown(*min)), ("max", &shown(*max))]));
            }
            Ok(Some(if *integer { json!(n as i64) } else { json!(n) }))
        }
        (Kind::Switch, Answer::Switch(on)) => Ok(Some(json!(on))),
        (Kind::One(options), Answer::One(Some(i))) => Ok(options.get(*i).map(|o| json!(o.value))),
        (Kind::Many { options, min, max }, Answer::Many(picked)) => {
            let chosen: Vec<&str> = options.iter().zip(picked).filter(|(_, on)| **on).map(|(o, _)| o.value.as_str()).collect();
            if chosen.is_empty() && field.required && min.is_none() {
                return Err(t("server_ask.required").to_string());
            }
            if min.is_some_and(|m| chosen.len() < m) || max.is_some_and(|m| chosen.len() > m) {
                let shown = |v: Option<usize>| v.map_or("…".to_string(), |v| v.to_string());
                return Err(tf("server_ask.pick_between", &[("min", &shown(*min)), ("max", &shown(*max))]));
            }
            Ok(Some(json!(chosen)))
        }
        _ => Ok(None),
    }
}

/// Whether `text` is as `format` says, as far as a form needs to know.
fn formatted(text: &str, format: Format) -> bool {
    let digits = |s: &str, n: usize| s.len() == n && s.chars().all(|c| c.is_ascii_digit());
    let date = |s: &str| {
        let parts: Vec<&str> = s.split('-').collect();
        parts.len() == 3 && digits(parts[0], 4) && digits(parts[1], 2) && digits(parts[2], 2) && (1..=12).contains(&parts[1].parse::<u32>().unwrap_or(0)) && (1..=31).contains(&parts[2].parse::<u32>().unwrap_or(0))
    };
    match format {
        Format::Email => {
            let mut halves = text.split('@');
            matches!((halves.next(), halves.next(), halves.next()), (Some(a), Some(b), None) if !a.is_empty() && b.contains('.') && !b.starts_with('.') && !b.ends_with('.') && !text.contains(char::is_whitespace))
        }
        Format::Uri => text.split_once(':').is_some_and(|(scheme, rest)| !rest.is_empty() && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))) && !text.contains(char::is_whitespace),
        Format::Date => date(text),
        Format::DateTime => text.split_once(['T', 't']).is_some_and(|(d, time)| date(d) && time.len() >= 8 && time[..8].split(':').count() == 3 && (time.ends_with('Z') || time.ends_with('z') || time[8..].contains(['+', '-']))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Value {
        json!({ "message": "Tell us about the trip", "requestedSchema": { "type": "object", "properties": {
            "name": { "type": "string", "title": "Your name", "minLength": 2, "maxLength": 20, "default": "Ada" },
            "email": { "type": "string", "format": "email" },
            "nights": { "type": "integer", "minimum": 1, "maximum": 14, "default": 3 },
            "budget": { "type": "number" },
            "window": { "type": "boolean", "default": true },
            "class": { "type": "string", "oneOf": [{ "const": "1", "title": "First" }, { "const": "2", "title": "Second" }], "default": "2" },
            "meals": { "type": "array", "minItems": 1, "maxItems": 2, "items": { "type": "string", "enum": ["breakfast", "lunch", "dinner"] }, "default": ["breakfast"] },
            "code": { "type": "string", "pattern": "^[A-Z]{3}$" }
        }, "required": ["name", "email", "nights"] } })
    }

    #[test]
    fn a_forms_fields_come_in_its_order_with_their_defaults() {
        let params = schema();
        let Ask::Form(form) = Ask::read(&params).unwrap() else { panic!() };
        let keys: Vec<&str> = form.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["name", "email", "nights", "budget", "window", "class", "meals", "code"], "as the server wrote them");
        assert_eq!((form.fields[0].title.as_str(), form.fields[0].required, form.fields[3].required), ("Your name", true, false));
        assert!(matches!(&form.fields[5].kind, Kind::One(o) if o[0].title == "First" && o[1].value == "2"));
        assert!(matches!(&form.fields[6].kind, Kind::Many { options, min: Some(1), max: Some(2) } if options.len() == 3));
        let answers = form.start.clone();
        assert_eq!(answers[0], Answer::Text("Ada".into()));
        assert_eq!(answers[2], Answer::Text("3".into()));
        assert_eq!((&answers[4], &answers[5], &answers[6]), (&Answer::Switch(true), &Answer::One(Some(1)), &Answer::Many(vec![true, false, false])));
        assert!(Ask::read(&json!({ "mode": "form", "requestedSchema": { "type": "object", "properties": { "x": { "type": "object" } } } })).is_err());
        assert!(Ask::read(&json!({ "mode": "sampling" })).is_err());
    }

    #[test]
    fn answers_are_checked_as_the_schema_says_before_they_go_back() {
        crate::i18n::set_language("en", &[]);
        let params = schema();
        let Ask::Form(form) = Ask::read(&params).unwrap() else { panic!() };
        let mut answers = form.start.clone();
        // The email is required and missing; the code doesn't match.
        answers[7] = Answer::Text("abc".into());
        let problems = form.content(&answers).unwrap_err();
        assert_eq!(problems.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [1, 7]);
        answers[1] = Answer::Text("ada@example".into());
        answers[7] = Answer::Text("KIX".into());
        assert_eq!(form.content(&answers).unwrap_err()[0].0, 1, "an address needs its domain");
        answers[1] = Answer::Text("ada@example.com".into());
        answers[2] = Answer::Text("2.5".into());
        assert_eq!(form.content(&answers).unwrap_err()[0].1, t("server_ask.not_integer"));
        answers[2] = Answer::Text("20".into());
        assert!(form.content(&answers).unwrap_err()[0].1.contains("14"));
        answers[2] = Answer::Text("4".into());
        answers[6] = Answer::Many(vec![true, true, true]);
        assert!(form.content(&answers).is_err(), "at most two meals");
        answers[6] = Answer::Many(vec![false, true, true]);
        let sent = form.content(&answers).unwrap();
        assert_eq!(sent, json!({ "name": "Ada", "email": "ada@example.com", "nights": 4, "window": true, "class": "2", "meals": ["lunch", "dinner"], "code": "KIX" }));
        assert!(sent.get("budget").is_none(), "a field left empty isn't sent");
        assert!(formatted("2026-11-16", Format::Date) && !formatted("2026-13-01", Format::Date));
        assert!(formatted("2026-11-16T09:15:00+01:00", Format::DateTime) && !formatted("2026-11-16 09:15", Format::DateTime));
        assert!(formatted("https://example.com/x", Format::Uri) && !formatted("not a uri", Format::Uri));
    }

    #[test]
    fn a_link_shows_its_host_and_warns_of_one_that_may_pass_for_another() {
        crate::i18n::set_language("en", &[]);
        let link = |url: &str| LinkAsk { message: String::new(), url: url.into(), id: "e1".into() };
        assert_eq!(link("https://user@mcp.example.com:8443/connect?x=1").around_host(), ("https://user@", "mcp.example.com", ":8443/connect?x=1"));
        assert_eq!(link("https://[::1]/x").around_host(), ("https://", "[::1]", "/x"));
        assert_eq!(link("https://example.com").around_host(), ("https://", "example.com", ""));
        assert_eq!(link("https://example.com/x").warning(), None);
        assert!(link("https://xn--pple-43d.com/").warning().is_some());
        assert!(link("https://аpple.com/").warning().is_some(), "a Cyrillic а");
        assert!(link("http://example.com/").warning().is_some());
        assert!(LinkAsk::read(&json!({ "url": "javascript:alert(1)" })).is_err());
    }

    #[test]
    fn a_link_the_opener_could_take_for_more_than_an_address_is_refused() {
        // Explorer read commas and quotes as its own, and a browser's
        // command line takes a quote for the address's end.
        for url in ["https://example.com/a\",/root,\\\\evil.example\\share", "https://example.com/a b", "https://evil.example\\@good.example/", "https://", "https:///x"] {
            assert!(LinkAsk::read(&json!({ "url": url })).is_err(), "{url}");
        }
        let read = LinkAsk::read(&json!({ "url": "https://mcp.example.com/connect?state=a,b", "elicitationId": "e1" })).unwrap();
        assert_eq!(read.url, "https://mcp.example.com/connect?state=a,b");
    }
}
