use pedantix::config::Config;
use serde_json::Value;

fn main() {
    let schema = serde_json::to_value(schemars::schema_for!(Config)).unwrap();
    let defs = &schema["definitions"];

    render_struct(
        "Top-level keys",
        None,
        &schema,
        defs,
        &["args", "attrs", "lets", "inherits", "lists", "overrides"],
    );

    render_struct(
        "Construct rules (`[args]`, `[attrs]`, `[lets]`, `[inherits]`, `[lists]`)",
        Some("The `merge` and `blank-lines*` keys only apply to `[attrs]`."),
        &defs["SortRules"],
        defs,
        &[],
    );

    render_struct(
        "Override keys (`[[overrides]]`)",
        None,
        &defs["Override"],
        defs,
        &[],
    );

    for name in ["FormatterChoice", "BlankLinesMode", "InheritPlacement"] {
        render_enum(name, &defs[name]);
    }
}

fn render_struct(title: &str, note: Option<&str>, schema: &Value, defs: &Value, skip: &[&str]) {
    println!("### {title}\n");
    if let Some(note) = note {
        println!("> {note}\n");
    }
    println!("| Key | Type | Default | Description |");
    println!("|-----|------|---------|-------------|");
    let props = schema["properties"].as_object().unwrap();
    let mut keys: Vec<&String> = props
        .keys()
        .filter(|k| !skip.contains(&k.as_str()))
        .collect();
    keys.sort();
    for key in keys {
        let prop = &props[key];
        println!(
            "| `{}` | {} | {} | {} |",
            key,
            type_of(prop, defs),
            default_of(prop),
            escape(prop["description"].as_str().unwrap_or("")),
        );
    }
    println!();
}

fn render_enum(name: &str, schema: &Value) {
    println!("### `{name}` values\n");
    println!("| Value | Description |");
    println!("|-------|-------------|");
    for (value, description) in enum_variants(schema) {
        println!("| `\"{}\"` | {} |", value, escape(&description));
    }
    println!();
}

fn enum_variants(schema: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for branch in schema["oneOf"].as_array().into_iter().flatten() {
        let description = branch["description"].as_str().unwrap_or("").to_string();
        for value in branch["enum"].as_array().into_iter().flatten() {
            if let Some(value) = value.as_str() {
                out.push((value.to_string(), description.clone()));
            }
        }
    }
    out
}

fn type_of(prop: &Value, defs: &Value) -> String {
    if let Some(name) = ref_name(prop) {
        let def = &defs[&name];
        if def["oneOf"].is_array() {
            return format!("[`{name}`](#{}-values)", name.to_lowercase());
        }
        return match name.as_str() {
            "SortRules" => "sort rules".to_string(),
            "PartialRules" => "partial rules".to_string(),
            other => other.to_string(),
        };
    }
    match &prop["type"] {
        Value::String(t) => scalar_type(t, prop),
        Value::Array(types) => {
            let non_null: Vec<&str> = types
                .iter()
                .filter_map(|t| t.as_str())
                .filter(|t| *t != "null")
                .collect();
            match non_null.as_slice() {
                [t] => scalar_type(t, prop),
                _ => "—".to_string(),
            }
        }
        _ => "—".to_string(),
    }
}

fn scalar_type(t: &str, prop: &Value) -> String {
    match t {
        "boolean" => "bool".to_string(),
        "integer" => "int".to_string(),
        "string" => "string".to_string(),
        "array" => {
            let inner = prop["items"]["type"].as_str().unwrap_or("value");
            format!("list of {}", scalar_type(inner, &prop["items"]))
        }
        other => other.to_string(),
    }
}

fn default_of(prop: &Value) -> String {
    match &prop["default"] {
        Value::Bool(b) => format!("`{b}`"),
        Value::Number(n) => format!("`{n}`"),
        Value::String(s) => format!("`\"{s}\"`"),
        Value::Null => "—".to_string(),
        Value::Array(a) if a.is_empty() => "`[]`".to_string(),
        _ => "—".to_string(),
    }
}

fn ref_name(prop: &Value) -> Option<String> {
    for key in ["allOf", "anyOf"] {
        if let Some(items) = prop[key].as_array() {
            for item in items {
                if let Some(r) = item["$ref"].as_str() {
                    return Some(r.trim_start_matches("#/definitions/").to_string());
                }
            }
        }
    }
    None
}

fn escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}
