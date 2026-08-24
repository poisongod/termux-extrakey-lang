/// Android JSONTokener-compatible parser for Termux extra-keys DSL.
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("json-dsl: {message} (line {line}, column {column})")]
pub struct JsonDslSyntaxError {
    pub index: usize,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            JsonValue::Object(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, JsonValue::Array(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, JsonValue::Object(_))
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Null => write!(f, "null"),
            JsonValue::Bool(b) => write!(f, "{b}"),
            JsonValue::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            JsonValue::String(s) => write!(f, "{s}"),
            JsonValue::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            JsonValue::Object(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn parse_json_dsl(text: &str) -> Result<JsonValue, JsonDslSyntaxError> {
    let mut p = Parser::new(text);
    let value = p.parse_value()?;
    p.skip_whitespace();
    if !p.at_end() {
        return Err(p.error(&format!(
            "unexpected character '{}' after top-level value",
            p.peek_char(0).unwrap_or('\0')
        )));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    i: usize,
    line: usize,
    col: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, i: 0, line: 1, col: 1 }
    }

    fn peek_byte(&self, offset: usize) -> Option<u8> {
        self.text.as_bytes().get(self.i + offset).copied()
    }

    fn peek_char(&self, offset: usize) -> Option<char> {
        self.text[self.i + offset..].chars().next()
    }

    fn at_end(&self) -> bool {
        self.i >= self.text.len()
    }

    fn advance_byte(&mut self) -> Option<u8> {
        let c = *self.text.as_bytes().get(self.i)?;
        self.i += 1;
        if c == b'\n' { self.line += 1; self.col = 1; } else { self.col += 1; }
        Some(c)
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char(0)?;
        let len = ch.len_utf8();
        self.i += len;
        if ch == '\n' { self.line += 1; self.col = 1; } else { self.col += 1; }
        Some(ch)
    }

    fn error(&self, message: &str) -> JsonDslSyntaxError {
        JsonDslSyntaxError {
            index: self.i,
            line: self.line,
            column: self.col,
            message: message.to_string(),
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.at_end() {
            if let Some(c) = self.peek_byte(0) {
                if is_ws(c) { self.advance_byte(); continue; }
            }
            break;
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, JsonDslSyntaxError> {
        self.skip_whitespace();
        if self.at_end() {
            return Err(self.error("unexpected end of input, expected a value"));
        }
        match self.peek_byte(0).unwrap() {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' | b'\'' => self.parse_quoted_string().map(JsonValue::String),
            _ => self.parse_bare_token(),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, JsonDslSyntaxError> {
        self.advance_byte(); // {
        let mut entries: Vec<(String, JsonValue)> = Vec::new();

        self.skip_whitespace();
        if self.peek_byte(0) == Some(b'}') {
            self.advance_byte();
            return Ok(JsonValue::Object(entries));
        }

        loop {
            self.skip_whitespace();
            if self.at_end() {
                return Err(self.error("unterminated object, expected key or '}'"));
            }
            let key = self.parse_key()?;
            if entries.iter().any(|(k, _)| k == &key) {
                return Err(self.error(&format!("duplicate key '{key}'")));
            }
            self.skip_whitespace();
            match self.peek_byte(0) {
                Some(b':') => { self.advance_byte(); }
                Some(c) => {
                    return Err(self.error(&format!(
                        "expected ':' after key '{key}', got '{}'", c as char
                    )));
                }
                None => return Err(self.error("expected ':' after key")),
            }
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_whitespace();
            if self.at_end() {
                return Err(self.error("unterminated object, expected ',' or '}'"));
            }
            match self.peek_byte(0).unwrap() {
                b',' => {
                    self.advance_byte();
                    self.skip_whitespace();
                    if self.peek_byte(0) == Some(b'}') {
                        self.advance_byte();
                        return Ok(JsonValue::Object(entries));
                    }
                }
                b'}' => {
                    self.advance_byte();
                    return Ok(JsonValue::Object(entries));
                }
                c => return Err(self.error(&format!("expected ',' or '}}', got '{}'", c as char))),
            }
        }
    }

    fn parse_key(&mut self) -> Result<String, JsonDslSyntaxError> {
        match self.peek_byte(0) {
            Some(b'"' | b'\'') => self.parse_quoted_string(),
            _ => self.parse_bare_token_string(),
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, JsonDslSyntaxError> {
        self.advance_byte(); // [
        let mut items = Vec::new();

        self.skip_whitespace();
        if self.peek_byte(0) == Some(b']') {
            self.advance_byte();
            return Ok(JsonValue::Array(items));
        }

        loop {
            let value = self.parse_value()?;
            items.push(value);
            self.skip_whitespace();
            if self.at_end() {
                return Err(self.error("unterminated array, expected ',' or ']'"));
            }
            match self.peek_byte(0).unwrap() {
                b',' => {
                    self.advance_byte();
                    self.skip_whitespace();
                    if self.peek_byte(0) == Some(b']') {
                        self.advance_byte();
                        return Ok(JsonValue::Array(items));
                    }
                }
                b']' => {
                    self.advance_byte();
                    return Ok(JsonValue::Array(items));
                }
                c => return Err(self.error(&format!("expected ',' or ']', got '{}'", c as char))),
            }
        }
    }

    fn parse_quoted_string(&mut self) -> Result<String, JsonDslSyntaxError> {
        let quote = self.advance_byte().unwrap();
        let mut out = String::new();

        loop {
            if self.at_end() {
                return Err(self.error(&format!(
                    "unterminated string (missing closing {})", quote as char
                )));
            }
            let c = self.advance_char().unwrap();
            if c == quote as char {
                return Ok(out);
            }
            if c == '\\' {
                if self.at_end() {
                    return Err(self.error("unterminated escape sequence"));
                }
                let esc = self.advance_char().unwrap();
                match esc {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'u' => {
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            if self.at_end() {
                                return Err(self.error("invalid \\u escape, expected 4 hex digits"));
                            }
                            let h = self.peek_char(0).unwrap();
                            if !h.is_ascii_hexdigit() {
                                return Err(self.error("invalid \\u escape, expected 4 hex digits"));
                            }
                            self.advance_byte();
                            hex.push(h);
                        }
                        let cp = u32::from_str_radix(&hex, 16)
                            .map_err(|_| self.error("invalid \\u escape"))?;
                        match char::from_u32(cp) {
                            Some(ch) => out.push(ch),
                            None => return Err(self.error("invalid unicode codepoint")),
                        }
                    }
                    '"' | '\'' | '\\' | '/' => out.push(esc),
                    _ => return Err(self.error(&format!("invalid escape sequence '\\{esc}'"))),
                }
            } else {
                out.push(c);
            }
        }
    }

    fn parse_bare_token_string(&mut self) -> Result<String, JsonDslSyntaxError> {
        let first = self.peek_byte(0).unwrap_or(0) as char;
        // Bare tokens must start with a valid character: letter, digit, minus, underscore, or dot
        if !first.is_ascii_alphanumeric() && first != '-' && first != '_' && first != '.' {
            return Err(self.error(&format!("unexpected character '{first}'")));
        }
        let mut token = String::new();
        while !self.at_end() {
            let c = self.peek_byte(0).unwrap();
            if is_ws(c) || c == b'{' || c == b'}' || c == b'[' || c == b']' || c == b',' || c == b':' {
                break;
            }
            let ch = self.advance_char().unwrap();
            token.push(ch);
        }
        if token.is_empty() {
            let ch = self.peek_char(0).unwrap_or('\0');
            return Err(self.error(&format!("unexpected character '{ch}'")));
        }
        Ok(token)
    }

    fn parse_bare_token(&mut self) -> Result<JsonValue, JsonDslSyntaxError> {
        let token = self.parse_bare_token_string()?;
        if token == "true" { return Ok(JsonValue::Bool(true)); }
        if token == "false" { return Ok(JsonValue::Bool(false)); }
        if token == "null" { return Ok(JsonValue::Null); }

        let first = token.as_bytes()[0];
        let looks_numeric = first == b'-' || first.is_ascii_digit();

        if looks_numeric {
            if let Ok(n) = parse_number(&token) {
                return Ok(JsonValue::Number(n));
            }
        }

        Ok(JsonValue::String(token))
    }
}

fn parse_number(s: &str) -> Result<f64, ()> {
    let bytes = s.as_bytes();
    let mut pos = 0;

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
    }
    if pos >= bytes.len() {
        return Err(());
    }

    if bytes[pos] == b'.' {
        pos += 1;
        if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
            return Err(());
        }
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    } else if bytes[pos].is_ascii_digit() {
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos < bytes.len() && bytes[pos] == b'.' {
            pos += 1;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
        }
    } else {
        return Err(());
    }

    if pos < bytes.len() && (bytes[pos] == b'e' || bytes[pos] == b'E') {
        pos += 1;
        if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
            pos += 1;
        }
        if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
            return Err(());
        }
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }

    if pos != bytes.len() {
        return Err(());
    }

    s.parse::<f64>().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null() {
        assert_eq!(parse_json_dsl("null").unwrap(), JsonValue::Null);
    }

    #[test]
    fn test_bool() {
        assert_eq!(parse_json_dsl("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(parse_json_dsl("false").unwrap(), JsonValue::Bool(false));
    }

    #[test]
    fn test_number() {
        assert_eq!(parse_json_dsl("42").unwrap(), JsonValue::Number(42.0));
        assert_eq!(parse_json_dsl("-3.14").unwrap(), JsonValue::Number(-3.14));
        assert_eq!(parse_json_dsl("1e5").unwrap(), JsonValue::Number(100000.0));
    }

    #[test]
    fn test_string() {
        assert_eq!(parse_json_dsl("\"hello\"").unwrap(), JsonValue::String("hello".into()));
        assert_eq!(parse_json_dsl("'hello'").unwrap(), JsonValue::String("hello".into()));
    }

    #[test]
    fn test_unquoted_string() {
        assert_eq!(parse_json_dsl("ESC").unwrap(), JsonValue::String("ESC".into()));
    }

    #[test]
    fn test_array() {
        let v = parse_json_dsl("[1, 2, 3]").unwrap();
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_trailing_comma() {
        let v = parse_json_dsl("[1, 2, 3,]").unwrap();
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_object() {
        let v = parse_json_dsl("{key: ESC}").unwrap();
        assert!(v.get("key").is_some());
    }

    #[test]
    fn test_escape_sequences() {
        let v = parse_json_dsl("\"\\n\\t\\r\"").unwrap();
        assert_eq!(v.as_str().unwrap(), "\n\t\r");
    }

    #[test]
    fn test_nested() {
        let v = parse_json_dsl("{a: [1, {b: true}]}").unwrap();
        assert!(v.get("a").is_some());
    }

    #[test]
    fn test_duplicate_key_error() {
        assert!(parse_json_dsl("{a: 1, a: 2}").is_err());
    }

    #[test]
    fn test_error_has_position() {
        let err = parse_json_dsl("{a: @}").unwrap_err();
        assert!(err.line > 0);
        assert!(err.column > 0);
    }
}
