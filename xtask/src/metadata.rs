use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMetadata {
    pub hut_version:          String,
    pub hut_publication_date: String,
    pub hut_json_attachment:  String,
    pub hut_json_sha256:      String,
    pub hut_url:              String,
    pub hid_version:          String,
    pub hid_url:              String,
}

pub fn read(workspace: &Path) -> Result<SourceMetadata, String> {
    let path = workspace.join("spec/source.toml");
    let source = fs::read_to_string(&path).map_err(|error| format!("Read {}: {error}", path.display()))?;
    let values = parse(&source)?;
    Ok(SourceMetadata {
        hut_version:          required(&values, "hut.version")?,
        hut_publication_date: required(&values, "hut.publication_date")?,
        hut_json_attachment:  required(&values, "hut.json_attachment")?,
        hut_json_sha256:      required(&values, "hut.json_sha256")?,
        hut_url:              required(&values, "hut.url")?,
        hid_version:          required(&values, "hid.version")?,
        hid_url:              required(&values, "hid.url")?,
    })
}

pub fn update_hut(
    workspace: &Path,
    version: &str,
    publication_date: &str,
    json_sha256: &str,
    url: Option<&str>,
) -> Result<(), String> {
    let path = workspace.join("spec/source.toml");
    let source = fs::read_to_string(&path).map_err(|error| format!("Read {}: {error}", path.display()))?;
    let mut section = String::new();
    let mut output = String::with_capacity(source.len() + 64);
    for original in source.lines() {
        let line = original.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1 .. line.len() - 1].trim().to_string();
        }
        let replacement = if section == "hut" {
            key_name(line).and_then(|key| {
                match key {
                    | "version" => Some(("version", version)),
                    | "publication_date" => Some(("publication_date", publication_date)),
                    | "json_sha256" => Some(("json_sha256", json_sha256)),
                    | "url" => url.map(|value| ("url", value)),
                    | _ => None,
                }
            })
        } else {
            None
        };
        if let Some((key, value)) = replacement {
            output.push_str(key);
            output.push_str(" = ");
            output.push_str(&quote(value));
        } else {
            output.push_str(original);
        }
        output.push('\n');
    }
    fs::write(&path, output).map_err(|error| format!("Write {}: {error}", path.display()))
}

fn parse(source: &str) -> Result<BTreeMap<String, String>, String> {
    let mut section = String::new();
    let mut values = BTreeMap::new();
    for (index, original) in source.lines().enumerate() {
        let line = strip_comment(original).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1 .. line.len() - 1].trim().to_string();
            if section.is_empty() {
                return Err(format!("Source.toml:{}: empty section", index + 1));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("Source.toml:{}: expected key = value", index + 1));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("Source.toml:{}: empty key", index + 1));
        }
        let value = parse_string(value.trim()).map_err(|error| format!("Source.toml:{}: {error}", index + 1))?;
        let full_key = if section.is_empty() {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        if values.insert(full_key.clone(), value).is_some() {
            return Err(format!("Source.toml:{}: duplicate `{full_key}`", index + 1));
        }
    }
    Ok(values)
}

fn required(values: &BTreeMap<String, String>, key: &str) -> Result<String, String> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| format!("Spec/source.toml is missing `{key}`"))
}

fn key_name(line: &str) -> Option<&str> {
    line.split_once('=').map(|(key, _)| key.trim())
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
        } else if character == '"' {
            quoted = true;
        } else if character == '#' {
            return &line[.. index];
        }
    }
    line
}

fn parse_string(value: &str) -> Result<String, String> {
    if !value.starts_with('"') || !value.ends_with('"') || value.len() < 2 {
        return Err("Only quoted string values are supported".to_string());
    }
    let mut output = String::new();
    let mut characters = value[1 .. value.len() - 1].chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match characters.next() {
            | Some('\\') => output.push('\\'),
            | Some('"') => output.push('"'),
            | Some('n') => output.push('\n'),
            | Some('r') => output.push('\r'),
            | Some('t') => output.push('\t'),
            | Some(other) => return Err(format!("Unsupported escape `\\{other}`")),
            | None => return Err("Trailing backslash in string".to_string()),
        }
    }
    Ok(output)
}

fn quote(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            | '\\' => output.push_str("\\\\"),
            | '"' => output.push_str("\\\""),
            | '\n' => output.push_str("\\n"),
            | '\r' => output.push_str("\\r"),
            | '\t' => output.push_str("\\t"),
            | other => output.push(other),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_and_comments() {
        let values = parse("[hut]\nversion = \"1.7.0\" # current\n").unwrap();
        assert_eq!(values.get("hut.version").map(String::as_str), Some("1.7.0"));
    }
}
