//! A server's tools as the model sees them: each named
//! `mcp__<server>__<tool>`, in the letters every provider takes, and its
//! schema cleaned for Gemini, which reads only part of JSON Schema.

use serde_json::{Map, Value};

/// The longest a tool's name may be for any provider.
const MOST: usize = 64;

/// Tool `tool` of server `server`, named for the model: letters, digits,
/// `_` and `-`, at most 64, with a short hash of the whole in place of what
/// doesn't fit, so two long names stay apart.
pub fn for_model(server: &str, tool: &str) -> String {
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>();
    let name = format!("mcp__{}__{}", clean(server), clean(tool));
    if name.len() <= MOST {
        return name;
    }
    use sha2::Digest;
    let hash = sha2::Sha256::digest(format!("{server}\0{tool}").as_bytes());
    let tag: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{}_{tag}", &name[..MOST - tag.len() - 1])
}

/// Whether `name` is one of a server's, as the model calls it.
pub fn is_server_tool(name: &str) -> bool {
    name.starts_with("mcp__")
}

/// `schema` as Gemini takes it: its OpenAPI subset, without `$schema`,
/// `$ref` and `$defs`, `additionalProperties`, `default` or `examples`; a
/// `const` an enum of one; a list of types its first that isn't null; an
/// object with no properties left as one.
pub fn for_gemini(schema: &Value) -> Value {
    match schema {
        Value::Object(o) => {
            let mut out = Map::new();
            for (key, value) in o {
                match key.as_str() {
                    "$schema" | "$ref" | "$defs" | "definitions" | "additionalProperties" | "default" | "examples" | "$id" | "$comment" | "title" => {}
                    "const" => {
                        out.insert("enum".into(), Value::Array(vec![value.clone()]));
                    }
                    "type" => {
                        let kind = match value {
                            Value::Array(kinds) => kinds.iter().find(|k| k.as_str() != Some("null")).cloned().unwrap_or(Value::String("string".into())),
                            other => other.clone(),
                        };
                        out.insert("type".into(), kind);
                    }
                    "properties" => {
                        let props: Map<String, Value> = value.as_object().map(|p| p.iter().map(|(k, v)| (k.clone(), for_gemini(v))).collect()).unwrap_or_default();
                        out.insert("properties".into(), Value::Object(props));
                    }
                    "anyOf" | "oneOf" | "allOf" => {
                        // Gemini takes anyOf alone; one of several is kept,
                        // the first that isn't null.
                        if let Some(first) = value.as_array().and_then(|v| v.iter().find(|s| s.get("type").and_then(Value::as_str) != Some("null"))) {
                            for (k, v) in for_gemini(first).as_object().into_iter().flatten() {
                                out.entry(k.clone()).or_insert(v.clone());
                            }
                        }
                    }
                    _ => {
                        out.insert(key.clone(), for_gemini(value));
                    }
                }
            }
            if out.get("type").and_then(Value::as_str) == Some("object") && !out.contains_key("properties") {
                out.insert("properties".into(), Value::Object(Map::new()));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(for_gemini).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_take_only_what_every_provider_takes_and_stay_apart_when_cut() {
        assert_eq!(for_model("files", "read_file"), "mcp__files__read_file");
        assert_eq!(for_model("my server", "do.it/now"), "mcp__my_server__do_it_now");
        let long = "a".repeat(80);
        let (one, two) = (for_model("s", &long), for_model("s", &format!("{long}b")));
        assert!(one.len() <= 64 && two.len() <= 64 && one != two, "{one} {two}");
        assert!(one.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        assert!(is_server_tool(&one) && !is_server_tool("run_command"));
    }

    #[test]
    fn a_schema_is_cleaned_for_gemini() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "path": { "type": ["string", "null"], "description": "Where", "default": "." },
                "mode": { "const": "fast" },
                "deep": { "type": "object", "additionalProperties": { "type": "string" } },
                "either": { "anyOf": [{ "type": "null" }, { "type": "integer", "minimum": 1 }] }
            },
            "required": ["path"]
        });
        let clean = for_gemini(&schema);
        assert_eq!(
            clean,
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Where" },
                    "mode": { "enum": ["fast"] },
                    "deep": { "type": "object", "properties": {} },
                    "either": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            })
        );
    }
}
