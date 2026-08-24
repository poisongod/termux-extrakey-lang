/// ExtraKeys JSON validation (four-field structure: key/macro/display/popup).
use thiserror::Error;

use crate::lib::json_dsl::JsonValue;

#[derive(Debug, Error)]
#[error("termux extra-keys: {path}: {message}")]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

const ALLOWED_FIELDS: &[&str] = &["key", "macro", "display", "popup"];

fn json_type(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Array(_) => "array",
        JsonValue::String(_) => "string",
        JsonValue::Number(_) => "number",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Object(_) => "object",
    }
}

pub fn validate_extra_key_json(json: &JsonValue) -> Result<(), ValidationError> {
    let path = "extra-keys".to_string();
    match json {
        JsonValue::Array(rows) => {
            if rows.is_empty() {
                return Err(ValidationError {
                    path,
                    message: "no rows defined (empty keyboard)".to_string(),
                });
            }
            for (r, row) in rows.iter().enumerate() {
                let row_path = format!("{path}[{r}]");
                match row {
                    JsonValue::Array(entries) => {
                        if entries.is_empty() {
                            return Err(ValidationError {
                                path: row_path,
                                message: "empty row".to_string(),
                            });
                        }
                        for (c, entry) in entries.iter().enumerate() {
                            validate_entry(entry, &format!("{row_path}[{c}]"))?;
                        }
                    }
                    _ => {
                        return Err(ValidationError {
                            path: row_path,
                            message: format!("row must be an array, got {}", json_type(row)),
                        });
                    }
                }
            }
            Ok(())
        }
        _ => Err(ValidationError {
            path,
            message: format!("must be a JSON array of key rows, got {}", json_type(json)),
        }),
    }
}

fn validate_entry(entry: &JsonValue, path: &str) -> Result<(), ValidationError> {
    match entry {
        JsonValue::String(_) => return Ok(()),
        JsonValue::Object(obj) => {
            for (field, _) in obj {
                if !ALLOWED_FIELDS.contains(&field.as_str()) {
                    return Err(ValidationError {
                        path: path.to_string(),
                        message: format!(
                            "unknown field '{field}' (allowed: key, macro, display, popup)"
                        ),
                    });
                }
            }

            let key = get_string_field(obj, path, "key")?;
            let mac = get_string_field(obj, path, "macro")?;

            if key.is_some() && mac.is_some() {
                return Err(ValidationError {
                    path: path.to_string(),
                    message: "both 'key' and 'macro' set; Termux rejects this (ExtraKeyButton)"
                        .to_string(),
                });
            }
            if key.is_none() && mac.is_none() {
                return Err(ValidationError {
                    path: path.to_string(),
                    message: "must specify either 'key' or 'macro' (ExtraKeyButton)".to_string(),
                });
            }

            let _ = get_string_field(obj, path, "display")?;

            if let Some((_, popup_val)) = obj.iter().find(|(k, _)| k == "popup") {
                match popup_val {
                    JsonValue::String(_) => {}
                    JsonValue::Object(popup_obj) => {
                        validate_popup_object(popup_obj, &format!("{path}.popup"))?;
                    }
                    _ => {
                        return Err(ValidationError {
                            path: path.to_string(),
                            message: format!(
                                "'popup' must be a string or object, got {}",
                                json_type(popup_val)
                            ),
                        });
                    }
                }
            }
            Ok(())
        }
        _ => Err(ValidationError {
            path: path.to_string(),
            message: format!("entry must be a string or object, got {}", json_type(entry)),
        }),
    }
}

fn get_string_field(
    obj: &[(String, JsonValue)],
    path: &str,
    field: &str,
) -> Result<Option<String>, ValidationError> {
    match obj.iter().find(|(k, _)| k == field) {
        Some((_, JsonValue::String(s))) => Ok(Some(s.clone())),
        Some((_, val)) => Err(ValidationError {
            path: path.to_string(),
            message: format!("'{}' must be a string, got {}", field, json_type(val)),
        }),
        None => Ok(None),
    }
}

fn validate_popup_object(
    obj: &[(String, JsonValue)],
    path: &str,
) -> Result<(), ValidationError> {
    for (field, _) in obj {
        if !ALLOWED_FIELDS.contains(&field.as_str()) {
            return Err(ValidationError {
                path: path.to_string(),
                message: format!("unknown field '{field}'"),
            });
        }
    }

    let key = get_string_field(obj, path, "key")?;
    let mac = get_string_field(obj, path, "macro")?;

    if key.is_some() && mac.is_some() {
        return Err(ValidationError {
            path: path.to_string(),
            message: "both 'key' and 'macro' set (invalid popup)".to_string(),
        });
    }
    if key.is_none() && mac.is_none() {
        return Err(ValidationError {
            path: path.to_string(),
            message: "popup must specify either 'key' or 'macro'".to_string(),
        });
    }

    if obj.iter().any(|(k, _)| k == "popup") {
        return Err(ValidationError {
            path: path.to_string(),
            message: "'popup' cannot be nested inside another 'popup' (unsupported by Termux)"
                .to_string(),
        });
    }

    let _ = get_string_field(obj, path, "display")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::json_dsl::parse_json_dsl;

    #[test]
    fn test_valid_simple() {
        let json = parse_json_dsl("[[{key: 'a'}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_ok());
    }

    #[test]
    fn test_not_array() {
        let json = parse_json_dsl("{key: 'a'}").unwrap();
        assert!(validate_extra_key_json(&json).is_err());
    }

    #[test]
    fn test_empty_array() {
        let json = parse_json_dsl("[]").unwrap();
        assert!(validate_extra_key_json(&json).is_err());
    }

    #[test]
    fn test_both_key_and_macro() {
        let json = parse_json_dsl("[[{key: 'a', macro: 'b'}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_err());
    }

    #[test]
    fn test_neither_key_nor_macro() {
        let json = parse_json_dsl("[[{display: 'a'}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_err());
    }

    #[test]
    fn test_unknown_field() {
        let json = parse_json_dsl("[[{key: 'a', unknown: 'b'}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_err());
    }

    #[test]
    fn test_nested_popup_rejected() {
        let json = parse_json_dsl("[[{key: 'a', popup: {key: 'b', popup: {key: 'c'}}}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_err());
    }

    #[test]
    fn test_popup_string_valid() {
        let json = parse_json_dsl("[[{key: 'a', popup: 'b'}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_ok());
    }

    #[test]
    fn test_popup_object_valid() {
        let json = parse_json_dsl("[[{key: 'a', popup: {key: 'b'}}]]").unwrap();
        assert!(validate_extra_key_json(&json).is_ok());
    }
}
