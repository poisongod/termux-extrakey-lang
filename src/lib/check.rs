/// Tjek validator — extra-keys JSON DSL validation with formatted error reports.
use crate::lib::json_dsl::{parse_json_dsl, JsonDslSyntaxError, JsonValue};
use crate::lib::validate::{validate_extra_key_json, ValidationError};

#[derive(Debug)]
pub enum CheckError {
    Syntax(JsonDslSyntaxError),
    Validation(ValidationError),
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::Syntax(e) => write!(f, "{e}"),
            CheckError::Validation(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CheckError::Syntax(e) => Some(e),
            CheckError::Validation(e) => Some(e),
        }
    }
}

impl From<JsonDslSyntaxError> for CheckError {
    fn from(e: JsonDslSyntaxError) -> Self {
        CheckError::Syntax(e)
    }
}

impl From<ValidationError> for CheckError {
    fn from(e: ValidationError) -> Self {
        CheckError::Validation(e)
    }
}

pub fn run(source: &str) -> Vec<String> {
    let json = match parse_json_dsl(source) {
        Ok(j) => j,
        Err(err) => {
            return format_full_report(source, &CheckError::Syntax(err), None);
        }
    };

    if let Err(ve) = validate_extra_key_json(&json) {
        return format_full_report(source, &CheckError::Validation(ve), Some(&json));
    }

    vec!["OK: valid termux extra-keys JSON DSL".to_string()]
}

pub fn format_full_report(source: &str, err: &CheckError, _context_json: Option<&JsonValue>) -> Vec<String> {
    let mut parts = Vec::new();

    match err {
        CheckError::Syntax(syntax_err) => {
            parts.push(format!("SyntaxError: {syntax_err}"));
            parts.push(format!(
                "  location: line {}, column {} (char {})",
                syntax_err.line, syntax_err.column, syntax_err.index
            ));
            if let Some((text, col)) = line_column(source, syntax_err) {
                parts.push(snippet(syntax_err.line, &text, col));
            }
            parts.push(format!("  hint: {}", syntax_hint(syntax_err)));
        }
        CheckError::Validation(validation_err) => {
            parts.push(format!("ValidationError: {validation_err}"));
            parts.push(format!("  path: {}", validation_err.path));
            parts.push(format!("  hint: {}", structure_hint(validation_err)));
        }
    }

    parts
}

fn line_column(source: &str, err: &JsonDslSyntaxError) -> Option<(String, usize)> {
    let text = source.lines().nth(err.line - 1)?.to_string();
    let col = err.column.saturating_sub(1);
    Some((text, col))
}

pub fn snippet(lineno: usize, line_text: &str, col: usize) -> String {
    let padding = " ".repeat(col);
    format!("  {:>3} | {line_text}\n      | {padding}^", lineno)
}

fn snippet_for_index(source: &str, index: usize) -> (String, usize) {
    let up_to = &source[..index];
    let line_start = up_to.rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[index..].find('\n').map_or(source.len(), |i| index + i);
    let line_text = &source[line_start..line_end];
    (line_text.to_string(), index - line_start)
}

pub fn format_dsl_error(source: &str, err: &JsonDslSyntaxError) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("✖ {err}"));
    lines.push(format!("  type: syntax error at index {}", err.index));
    let (text, col) = snippet_for_index(source, err.index);
    let snippet_str = format!("  {:>3} | {text}\n      | {}^", err.line, " ".repeat(col));
    lines.push(snippet_str);
    lines.push("  hint: fix the highlighted character or complete the missing token".to_string());
    lines
}

fn syntax_hint(err: &JsonDslSyntaxError) -> &'static str {
    let msg = &err.message;
    if msg.contains("unterminated") {
        "add the missing closing bracket or quote"
    } else if msg.contains("unexpected character") {
        "this character is not allowed here"
    } else if msg.contains("expected ':'") {
        "object keys must be followed by ':'"
    } else if msg.contains("duplicate key") {
        "each key must appear at most once per object"
    } else if msg.contains("escape") {
        "only n, t, u, b, f, r, /, \\, ', \" are valid escapes"
    } else if msg.contains("after top-level") {
        "remove trailing content after the value"
    } else {
        "fix the JSON structure"
    }
}

fn structure_hint(err: &ValidationError) -> &'static str {
    let msg = &err.message;
    if msg.contains("both 'key' and 'macro'") {
        "keep exactly one of key/macro per object"
    } else if msg.contains("key or 'macro") {
        "add a 'key' or 'macro'"
    } else if msg.contains("must be a string") {
        "wrap the value in quotes, e.g. display: 'CTRL'"
    } else if msg.contains("cannot be nested") {
        "remove the nested 'popup'"
    } else if msg.contains("row must be") {
        "make sure each row is a list"
    } else if msg.contains("must be a string or object") {
        "each key is a string or an object"
    } else {
        "fix the extra-key object"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_valid() {
        let result = run("[[{key: 'a'}]]");
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("OK"));
    }

    #[test]
    fn test_run_syntax_error() {
        let result = run("{key: @}");
        assert!(result.iter().any(|l| l.contains("SyntaxError")));
    }

    #[test]
    fn test_run_validation_error() {
        let result = run("[[{display: 'a'}]]");
        assert!(result.iter().any(|l| l.contains("ValidationError")));
    }

    #[test]
    fn test_format_dsl_error() {
        let err = parse_json_dsl("{a: @}").unwrap_err();
        let lines = format_dsl_error("{a: @}", &err);
        assert!(lines[0].contains("✖"));
        assert!(lines.iter().any(|l| l.contains("hint")));
    }

    #[test]
    fn test_snippet() {
        let s = snippet(1, "hello world", 5);
        assert!(s.contains("1 | hello world"));
        assert!(s.contains("^"));
    }

    #[test]
    fn test_format_full_report_syntax() {
        let err = parse_json_dsl("{a: @}").unwrap_err();
        let report = format_full_report("{a: @}", &CheckError::Syntax(err), None);
        assert!(report.iter().any(|l| l.contains("SyntaxError")));
        assert!(report.iter().any(|l| l.contains("hint")));
    }

    #[test]
    fn test_format_full_report_validation() {
        let json = parse_json_dsl("[[{display: 'a'}]]").unwrap();
        let ve = validate_extra_key_json(&json).unwrap_err();
        let report = format_full_report(
            "[[{display: 'a'}]]",
            &CheckError::Validation(ve),
            Some(&json),
        );
        assert!(report.iter().any(|l| l.contains("ValidationError")));
    }
}
