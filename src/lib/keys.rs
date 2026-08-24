use std::collections::{HashMap, HashSet};

use thiserror::Error;

use super::keyset;

// ═══════════════════════════════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum TokenizerError {
    #[error("keys: {message} (line {line}, column {column})")]
    UnterminatedString { message: String, line: usize, column: usize },
    #[error("keys: unexpected character '{ch}' (line {line}, column {column})")]
    UnexpectedChar { ch: char, line: usize, column: usize },
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("keys: {message} (line {line}, column {column})")]
    UnexpectedToken { message: String, line: usize, column: usize },
}

impl ParseError {
    fn unexpected(expected: &str, got: &str, line: usize, column: usize) -> Self {
        ParseError::UnexpectedToken {
            message: format!("expected {expected}, got {got}"),
            line,
            column,
        }
    }

    fn general(message: impl Into<String>, line: usize, column: usize) -> Self {
        ParseError::UnexpectedToken {
            message: message.into(),
            line,
            column,
        }
    }
}

#[derive(Debug, Error)]
#[error("keys: {message}")]
pub struct CompileError {
    message: String,
}

impl CompileError {
    fn new(message: impl Into<String>) -> Self {
        CompileError { message: message.into() }
    }
}

#[derive(Debug, Error)]
#[error("tekl: {message}")]
pub struct IoError {
    message: String,
    #[source]
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl IoError {
    fn new(message: impl Into<String>) -> Self {
        IoError { message: message.into(), cause: None }
    }

    fn with_cause(message: impl Into<String>, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        IoError { message: message.into(), cause: Some(Box::new(cause)) }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tokenizer
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Newline { line: usize, column: usize },
    Semi { line: usize, column: usize },
    Dot { line: usize, column: usize },
    LParen { line: usize, column: usize },
    RParen { line: usize, column: usize },
    Colon { line: usize, column: usize },
    Eq { line: usize, column: usize },
    Number { value: i64, line: usize, column: usize },
    Literal { value: String, line: usize, column: usize },
    Ident { value: String, line: usize, column: usize },
    Eof { line: usize, column: usize },
}

impl Token {
    pub fn line(&self) -> usize {
        match self {
            Token::Newline { line, .. }
            | Token::Semi { line, .. }
            | Token::Dot { line, .. }
            | Token::LParen { line, .. }
            | Token::RParen { line, .. }
            | Token::Colon { line, .. }
            | Token::Eq { line, .. }
            | Token::Number { line, .. }
            | Token::Literal { line, .. }
            | Token::Ident { line, .. }
            | Token::Eof { line, .. } => *line,
        }
    }

    pub fn column(&self) -> usize {
        match self {
            Token::Newline { column, .. }
            | Token::Semi { column, .. }
            | Token::Dot { column, .. }
            | Token::LParen { column, .. }
            | Token::RParen { column, .. }
            | Token::Colon { column, .. }
            | Token::Eq { column, .. }
            | Token::Number { column, .. }
            | Token::Literal { column, .. }
            | Token::Ident { column, .. }
            | Token::Eof { column, .. } => *column,
        }
    }

    pub fn value_str(&self) -> &str {
        match self {
            Token::Ident { value, .. } | Token::Literal { value, .. } => value,
            _ => "",
        }
    }

    pub fn value_i64(&self) -> i64 {
        match self {
            Token::Number { value, .. } => *value,
            _ => 0,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Token::Newline { .. } => "NEWLINE",
            Token::Semi { .. } => "SEMI",
            Token::Dot { .. } => "DOT",
            Token::LParen { .. } => "LPAREN",
            Token::RParen { .. } => "RPAREN",
            Token::Colon { .. } => "COLON",
            Token::Eq { .. } => "EQ",
            Token::Number { .. } => "NUMBER",
            Token::Literal { .. } => "LITERAL",
            Token::Ident { .. } => "IDENT",
            Token::Eof { .. } => "EOF",
        }
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, TokenizerError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line: usize = 1;
    let mut column: usize = 1;

    while i < chars.len() {
        let c = chars[i];

        if c == '\n' {
            tokens.push(Token::Newline { line, column });
            line += 1;
            column = 1;
            i += 1;
            continue;
        }

        if c == ' ' || c == '\t' || c == '\r' {
            column += 1;
            i += 1;
            continue;
        }

        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        if c == ';' {
            tokens.push(Token::Semi { line, column });
            column += 1;
            i += 1;
            continue;
        }

        if c == '.' {
            tokens.push(Token::Dot { line, column });
            column += 1;
            i += 1;
            continue;
        }

        if c == '(' {
            tokens.push(Token::LParen { line, column });
            column += 1;
            i += 1;
            continue;
        }

        if c == ')' {
            tokens.push(Token::RParen { line, column });
            column += 1;
            i += 1;
            continue;
        }

        if c == ':' {
            tokens.push(Token::Colon { line, column });
            column += 1;
            i += 1;
            continue;
        }

        if c == '=' {
            tokens.push(Token::Eq { line, column });
            column += 1;
            i += 1;
            continue;
        }

        // Number only after EQ token
        if is_digit(c) && matches!(tokens.last(), Some(Token::Eq { .. })) {
            let start = i;
            while i < chars.len() && is_digit(chars[i]) {
                i += 1;
            }
            let val_str: String = chars[start..i].iter().collect();
            let value = val_str.parse::<i64>().unwrap();
            tokens.push(Token::Number { value, line, column });
            column += i - start;
            continue;
        }

        if c == '\'' || c == '"' {
            let quote = c;
            let mut j = i + 1;
            while j < chars.len() && chars[j] != quote && chars[j] != '\n' {
                j += 1;
            }
            if j >= chars.len() || chars[j] != quote {
                return Err(TokenizerError::UnterminatedString {
                    message: "unterminated string literal".to_string(),
                    line,
                    column,
                });
            }
            let val: String = chars[i + 1..j].iter().collect();
            tokens.push(Token::Literal { value: val, line, column });
            column += j - i + 1;
            i = j + 1;
            continue;
        }

        if is_ident_char(c) {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let val: String = chars[start..i].iter().collect();
            tokens.push(Token::Ident { value: val, line, column });
            column += i - start;
            continue;
        }

        return Err(TokenizerError::UnexpectedChar { ch: c, line, column });
    }

    tokens.push(Token::Eof { line, column });
    Ok(tokens)
}

// ═══════════════════════════════════════════════════════════════════
// AST
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    Key {
        key: String,
        kind: KeyKind,
    },
    Combo {
        node: Box<AstNode>,
        inner: Box<AstNode>,
    },
    Label {
        node: Box<AstNode>,
        label: String,
    },
    Popup {
        main: Box<AstNode>,
        popup_node: Box<AstNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Named,
    Special,
    Literal,
}

impl AstNode {
    pub fn line(&self) -> usize {
        match self {
            AstNode::Key { .. } => 0,
            AstNode::Combo { node, .. } => node.line(),
            AstNode::Label { node, .. } => node.line(),
            AstNode::Popup { main, .. } => main.line(),
        }
    }

    pub fn column(&self) -> usize {
        match self {
            AstNode::Key { .. } => 0,
            AstNode::Combo { node, .. } => node.column(),
            AstNode::Label { node, .. } => node.column(),
            AstNode::Popup { main, .. } => main.column(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Global {
    pub key: String,
    pub value: GlobalValue,
}

#[derive(Debug, Clone)]
pub enum GlobalValue {
    Number(i64),
    Ident(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonEntry {
    Simple(String),
    Object(HashMap<String, JsonValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Str(String),
    Num(i64),
    Bool(bool),
    Null,
    Object(HashMap<String, JsonValue>),
    Array(Vec<JsonValue>),
}

impl JsonValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            JsonValue::Bool(b) => *b,
            JsonValue::Null => false,
            JsonValue::Num(n) => *n != 0,
            JsonValue::Str(s) => !s.is_empty(),
            _ => true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Parser
// ═══════════════════════════════════════════════════════════════════

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof { line: 0, column: 0 })
    }

    fn next(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof { line: 0, column: 0 });
        self.pos += 1;
        tok
    }

    fn expect(&mut self, kind: &str) -> Result<Token, ParseError> {
        let tok = self.next();
        if tok.kind_name() != kind {
            return Err(ParseError::unexpected(kind, tok.kind_name(), tok.line(), tok.column()));
        }
        Ok(tok)
    }

    /// A ::= N | S | A(A) | A:N
    fn parse_base_key(&mut self) -> Result<AstNode, ParseError> {
        let tok = self.next();
        match &tok {
            Token::Ident { value, .. } => {
                let kind = if keyset::is_special_key(value) {
                    KeyKind::Special
                } else {
                    KeyKind::Named
                };
                Ok(AstNode::Key { key: value.clone(), kind })
            }
            Token::Literal { value, .. } => {
                Ok(AstNode::Key { key: value.clone(), kind: KeyKind::Literal })
            }
            _ => Err(ParseError::general(
                format!("expected key, got {}", tok.kind_name()),
                tok.line(),
                tok.column(),
            )),
        }
    }

    /// Atom: base key, then chain of ( ) or : label
    fn parse_atom(&mut self) -> Result<AstNode, ParseError> {
        let mut node = self.parse_base_key()?;

        loop {
            let t = self.peek().clone();

            if let Token::LParen { .. } = &t {
                self.next();
                let inner = self.parse_atom()?;
                self.expect("RPAREN")?;
                node = AstNode::Combo { node: Box::new(node), inner: Box::new(inner) };
                continue;
            }

            if let Token::Colon { .. } = &t {
                self.next();
                let label_tok = self.peek().clone();
                if let Token::Literal { line, column, .. } = &label_tok {
                    return Err(ParseError::general("label must not be quoted", *line, *column));
                }
                if let Token::Ident { value, .. } = &label_tok {
                    self.next();
                    node = AstNode::Label { node: Box::new(node), label: value.clone() };
                } else {
                    node = AstNode::Label { node: Box::new(node), label: String::new() };
                }
                continue;
            }

            break;
        }

        Ok(node)
    }

    /// M ::= A | A.A | M M | M'\n' | M';\n'
    fn parse_entry(&mut self) -> Result<AstNode, ParseError> {
        let main = self.parse_atom()?;
        if let Token::Dot { .. } = self.peek() {
            self.next();
            let popup_node = self.parse_atom()?;
            if matches!(&popup_node, AstNode::Popup { .. }) {
                return Err(ParseError::general(
                    "nested popup is not supported",
                    popup_node.line(),
                    popup_node.column(),
                ));
            }
            return Ok(AstNode::Popup { main: Box::new(main), popup_node: Box::new(popup_node) });
        }
        Ok(main)
    }

    /// G ::= "height" '=' NUMBER | "theme" '=' IDENT | "layout" '=' IDENT | "reload"
    fn parse_global(&mut self) -> Result<Global, ParseError> {
        let key_tok = self.expect("IDENT")?;
        let key = key_tok.value_str().to_lowercase();

        if key == "reload" {
            return Ok(Global {
                key: "reload".to_string(),
                value: GlobalValue::Bool(true),
            });
        }

        self.expect("EQ")?;

        if key == "height" {
            let num_tok = self.expect("NUMBER")?;
            return Ok(Global {
                key: "height".to_string(),
                value: GlobalValue::Number(num_tok.value_i64()),
            });
        }

        let val_tok = self.expect("IDENT")?;
        Ok(Global {
            key,
            value: GlobalValue::Ident(val_tok.value_str().to_string()),
        })
    }

    fn parse_program(&mut self) -> Result<(Vec<Vec<AstNode>>, Vec<Global>), ParseError> {
        let mut rows: Vec<Vec<AstNode>> = Vec::new();
        let mut row: Vec<AstNode> = Vec::new();
        let mut globals: Vec<Global> = Vec::new();

        loop {
            let tok = self.peek().clone();

            match &tok {
                Token::Eof { .. } => break,
                Token::Newline { .. } => {
                    self.next();
                    continue;
                }
                Token::Semi { .. } => {
                    self.next();
                    if !row.is_empty() {
                        rows.push(row);
                        row = Vec::new();
                    }
                    continue;
                }
                Token::Ident { value, .. } => {
                    let lower = value.to_lowercase();
                    if matches!(lower.as_str(), "height" | "theme" | "layout" | "reload") {
                        globals.push(self.parse_global()?);
                        continue;
                    }
                    row.push(self.parse_entry()?);
                    continue;
                }
                Token::Literal { .. } => {
                    row.push(self.parse_entry()?);
                    continue;
                }
                _ => {
                    return Err(ParseError::general(
                        format!("unexpected token {}", tok.kind_name()),
                        tok.line(),
                        tok.column(),
                    ));
                }
            }
        }

        if !row.is_empty() {
            rows.push(row);
        }

        validate_rows(&rows)?;

        Ok((rows, globals))
    }
}

fn validate_rows(rows: &[Vec<AstNode>]) -> Result<(), ParseError> {
    for (r, row) in rows.iter().enumerate() {
        let mut specials: HashSet<String> = HashSet::new();
        for entry in row {
            let is_key = matches!(entry, AstNode::Key { kind: KeyKind::Special, .. });
            let is_labeled_special = matches!(
                entry,
                AstNode::Label { node, .. } if matches!(node.as_ref(), AstNode::Key { kind: KeyKind::Special, .. })
            );

            if is_key || is_labeled_special {
                let key_str = if is_key {
                    match entry {
                        AstNode::Key { key, .. } => key.clone(),
                        _ => unreachable!(),
                    }
                } else {
                    match entry {
                        AstNode::Label { node, .. } => match node.as_ref() {
                            AstNode::Key { key, .. } => key.clone(),
                            _ => unreachable!(),
                        },
                        _ => unreachable!(),
                    }
                };

                let upper = key_str.to_uppercase();
                if specials.contains(&upper) {
                    return Err(ParseError::general(
                        format!("special key '{key_str}' appears more than once in row {}", r + 1),
                        0,
                        0,
                    ));
                }
                specials.insert(upper);
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Public parse function
// ═══════════════════════════════════════════════════════════════════

pub struct ParseResult {
    pub rows: Vec<Vec<AstNode>>,
    pub globals: Vec<Global>,
}

pub fn parse(src: &str) -> Result<ParseResult, ParseError> {
    let tokens = tokenize(src).map_err(|e| match e {
        TokenizerError::UnterminatedString { message, line, column } => {
            ParseError::general(message, line, column)
        }
        TokenizerError::UnexpectedChar { ch, line, column } => {
            ParseError::general(format!("unexpected character '{ch}'"), line, column)
        }
    })?;
    let mut parser = Parser::new(tokens);
    let (rows, globals) = parser.parse_program()?;
    Ok(ParseResult { rows, globals })
}

pub fn format_parse_error(source: &str, err: &ParseError) -> String {
    match err {
        ParseError::UnexpectedToken { message, line, column } => {
            let lines: Vec<&str> = source.split('\n').collect();
            let text = lines.get(line.wrapping_sub(1)).copied().unwrap_or("");
            let col = column.wrapping_sub(1);
            format!(
                "keys: {message}\n  location: line {line}, column {column}\n  {:3} | {text}\n      | {:col$}^",
                line,
                "",
            )
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Compiler
// ═══════════════════════════════════════════════════════════════════

/// Named/special keys normalize to uppercase; quoted literals and plain identifiers keep original case.
fn key_name(node: &AstNode) -> Result<String, CompileError> {
    match node {
        AstNode::Key { key, kind } => {
            if *kind == KeyKind::Literal {
                return Ok(key.clone());
            }
            let upper = key.to_uppercase();
            if keyset::is_special_key(key) || keyset::is_named_key(key) {
                Ok(upper)
            } else {
                Ok(key.clone())
            }
        }
        _ => Err(CompileError::new("key_name called on non-key node")),
    }
}

/// A(A) combo: recursively collect leaf keys → "MAIN ALT X" (uppercased)
fn collect_combo_keys(node: &AstNode, out: &mut Vec<String>) -> Result<(), CompileError> {
    match node {
        AstNode::Combo { node, inner } => {
            collect_combo_keys(node, out)?;
            collect_combo_keys(inner, out)?;
            Ok(())
        }
        AstNode::Key { .. } => {
            out.push(key_name(node)?);
            Ok(())
        }
        _ => Err(CompileError::new(format!(
            "combo contains '{}' (only keys allowed inside '()')",
            match node {
                AstNode::Label { .. } => "label",
                AstNode::Popup { .. } => "popup",
                AstNode::Combo { .. } => unreachable!(),
                AstNode::Key { .. } => unreachable!(),
            }
        ))),
    }
}

fn compile_atom(node: &AstNode) -> Result<JsonValue, CompileError> {
    match node {
        AstNode::Key { .. } => Ok(JsonValue::Str(key_name(node)?)),

        AstNode::Label { node: inner, label } => {
            let base = compile_atom(inner)?;
            match &base {
                JsonValue::Str(s) => {
                    let mut m = HashMap::new();
                    m.insert("key".to_string(), JsonValue::Str(s.clone()));
                    m.insert("display".to_string(), JsonValue::Str(label.clone()));
                    Ok(JsonValue::Object(m))
                }
                JsonValue::Object(obj) => {
                    if obj.contains_key("key") || obj.contains_key("macro") {
                        let mut m = obj.clone();
                        m.insert("display".to_string(), JsonValue::Str(label.clone()));
                        Ok(JsonValue::Object(m))
                    } else {
                        Err(CompileError::new(format!(
                            "cannot label '{}'",
                            json_value_to_string(&base)
                        )))
                    }
                }
                _ => Err(CompileError::new(format!(
                    "cannot label '{}'",
                    json_value_to_string(&base)
                ))),
            }
        }

        AstNode::Combo { node, inner } => {
            let mut keys = Vec::new();
            collect_combo_keys(node, &mut keys)?;
            collect_combo_keys(inner, &mut keys)?;
            if keys.len() < 2 {
                return Err(CompileError::new(format!(
                    "combo '{}' must contain the modifier and at least one key",
                    keys.join(" ")
                )));
            }
            let mut m = HashMap::new();
            m.insert("macro".to_string(), JsonValue::Str(keys.join(" ")));
            Ok(JsonValue::Object(m))
        }

        AstNode::Popup { .. } => Err(CompileError::new("popup is not valid inside '()'")),
    }
}

fn compile_entry(node: &AstNode) -> Result<JsonValue, CompileError> {
    if !matches!(node, AstNode::Popup { .. }) {
        return compile_atom(node);
    }

    if let AstNode::Popup { main, popup_node } = node {
        let main_val = compile_atom(main)?;
        let popup_val = compile_atom(popup_node)?;

        match &main_val {
            JsonValue::Str(s) => {
                let mut m = HashMap::new();
                m.insert("key".to_string(), JsonValue::Str(s.clone()));
                m.insert("popup".to_string(), popup_val);
                Ok(JsonValue::Object(m))
            }
            JsonValue::Object(obj) => {
                if !obj.contains_key("popup") {
                    let mut m = obj.clone();
                    m.insert("popup".to_string(), popup_val);
                    Ok(JsonValue::Object(m))
                } else {
                    Err(CompileError::new(format!(
                        "'{}': popup cannot be nested or followed by a key",
                        json_value_to_string(&main_val)
                    )))
                }
            }
            _ => Err(CompileError::new(format!(
                "'{}': popup cannot be nested or followed by a key",
                json_value_to_string(&main_val)
            ))),
        }
    } else {
        unreachable!()
    }
}

pub fn compile(rows: &[Vec<AstNode>]) -> Result<Vec<Vec<JsonValue>>, CompileError> {
    rows.iter()
        .map(|row| row.iter().map(compile_entry).collect())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// Compiler validation (Termux-specific JSON rules)
// ═══════════════════════════════════════════════════════════════════

pub fn validate_termux_json(json: &[Vec<JsonValue>]) -> Result<(), CompileError> {
    for row in json {
        for key in row {
            match key {
                JsonValue::Str(_) => continue,
                JsonValue::Object(obj) => {
                    let has_key = obj.contains_key("key");
                    let has_macro = obj.contains_key("macro");
                    let has_display = obj.contains_key("display");

                    if has_key && has_macro {
                        return Err(CompileError::new(format!(
                            "entry {}: Termux rejects keys with both 'key' and 'macro'",
                            json_value_to_string(&JsonValue::Object(obj.clone()))
                        )));
                    }
                    if !has_key && !has_macro && !has_display {
                        return Err(CompileError::new(format!(
                            "entry {}: Termux requires 'key' or 'macro'",
                            json_value_to_string(&JsonValue::Object(obj.clone()))
                        )));
                    }
                }
                _ => {
                    return Err(CompileError::new(format!(
                        "invalid extra key entry: {}",
                        json_value_to_string(key)
                    )));
                }
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Compiler warnings (non-fatal, yellow, Chinese)
// ═══════════════════════════════════════════════════════════════════

pub fn collect_warnings(json: &[Vec<JsonValue>]) -> Vec<String> {
    let mut warnings = Vec::new();

    // Empty field values
    for (r, row) in json.iter().enumerate() {
        for (c, entry) in row.iter().enumerate() {
            if let JsonValue::Object(obj) = entry {
                for (k, v) in obj {
                    if matches!(v, JsonValue::Str(s) if s.is_empty()) {
                        warnings.push(format!(
                            "第 {} 键盘行第 {} 键: '{k}' 的值为空",
                            r + 1,
                            c + 1,
                        ));
                    }
                }
                if let Some(JsonValue::Object(popup)) = obj.get("popup") {
                    for (k, v) in popup {
                        if matches!(v, JsonValue::Str(s) if s.is_empty()) {
                            warnings.push(format!(
                                "第 {} 键盘行第 {} 键的 popup: '{k}' 的值为空",
                                r + 1,
                                c + 1,
                            ));
                        }
                    }
                }
            }
        }
    }

    // Uneven row lengths
    let lengths: Vec<usize> = json.iter().map(|row| row.len()).collect();
    if lengths.windows(2).any(|w| w[0] != w[1]) {
        let parts: Vec<String> = lengths
            .iter()
            .enumerate()
            .map(|(i, n)| format!("第{}行 {n} 键", i + 1))
            .collect();
        warnings.push(format!("各键盘行键位数不同: {}", parts.join(", ")));
    }

    // Missing DRAWER / KEYBOARD
    fn has_key(json: &[Vec<JsonValue>], name: &str) -> bool {
        for row in json {
            for entry in row {
                match entry {
                    JsonValue::Str(s) if s == name => return true,
                    JsonValue::Object(obj) => {
                        if obj.get("key").map(|v| v == &JsonValue::Str(name.to_string())).unwrap_or(false) {
                            return true;
                        }
                        if obj.get("popup").map(|v| v == &JsonValue::Str(name.to_string())).unwrap_or(false) {
                            return true;
                        }
                        if let Some(JsonValue::Object(popup_obj)) = obj.get("popup") {
                            if popup_obj.get("key").map(|v| v == &JsonValue::Str(name.to_string())).unwrap_or(false) {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    if !has_key(json, "DRAWER") {
        warnings.push("键盘方案中未找到应用抽屉键 (DRAWER)".to_string());
    }
    if !has_key(json, "KEYBOARD") {
        warnings.push("键盘方案中未找到键盘切换键 (KEYBOARD)".to_string());
    }

    warnings
}

// ═══════════════════════════════════════════════════════════════════
// Stringify (DSL → Android-compatible JSON dialect)
// ═══════════════════════════════════════════════════════════════════

fn quote_string(s: &str) -> String {
    let mut out = String::from('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn is_bare_token(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_keyword(s: &str) -> bool {
    matches!(s, "true" | "false" | "null")
}

fn stringify_value(v: &JsonValue) -> String {
    match v {
        JsonValue::Str(s) => {
            if !s.is_empty() && is_bare_token(s) && !is_keyword(s) {
                s.clone()
            } else {
                quote_string(s)
            }
        }
        JsonValue::Num(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        JsonValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(stringify_value).collect();
            format!("[{}]", items.join(", "))
        }
        JsonValue::Object(obj) => stringify_object(obj),
    }
}

fn stringify_object(obj: &HashMap<String, JsonValue>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (k, v) in obj {
        parts.push(format!("{}: {}", k, stringify_value(v)));
    }
    format!("{{{}}}", parts.join(", "))
}

pub fn stringify_dsl(value: &JsonValue) -> String {
    stringify_value(value)
}

pub fn stringify_json_rows(rows: &[Vec<JsonValue>]) -> String {
    stringify_value(&JsonValue::Array(
        rows.iter()
            .map(|row| JsonValue::Array(row.clone()))
            .collect(),
    ))
}

// ═══════════════════════════════════════════════════════════════════
// Printer (AST → tekl source text, inverse of parser)
// ═══════════════════════════════════════════════════════════════════

pub fn print_literal(s: &str) -> String {
    let q = if s.contains('"') { '\'' } else { '"' };
    format!("{q}{s}{q}")
}

fn print_atom(node: &AstNode) -> String {
    match node {
        AstNode::Key { key, kind } => {
            if *kind == KeyKind::Literal {
                print_literal(key)
            } else {
                key.clone()
            }
        }
        AstNode::Combo { node, inner } => {
            format!("{}({})", print_atom(node), print_atom(inner))
        }
        AstNode::Label { node, label } => {
            format!("{}:{label}", print_atom(node))
        }
        AstNode::Popup { main, popup_node } => {
            format!("{}.{}", print_atom(main), print_atom(popup_node))
        }
    }
}

pub fn print_entry(entry: &AstNode) -> String {
    print_atom(entry)
}

pub fn print_row(row: &[AstNode]) -> String {
    let entries: Vec<String> = row.iter().map(print_entry).collect();
    format!("{};", entries.join(" "))
}

pub fn print_rows(rows: &[Vec<AstNode>]) -> String {
    rows.iter()
        .map(|r| print_row(r))
        .collect::<Vec<_>>()
        .join("\n")
}

// ═══════════════════════════════════════════════════════════════════
// JSON value helpers
// ═══════════════════════════════════════════════════════════════════

fn json_value_to_string(v: &JsonValue) -> String {
    match v {
        JsonValue::Str(s) => s.clone(),
        JsonValue::Num(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        JsonValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_value_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        JsonValue::Object(obj) => {
            let mut parts: Vec<String> = Vec::new();
            for (k, v) in obj {
                parts.push(format!("{k}: {}", json_value_to_string(v)));
            }
            format!("{{{}}}", parts.join(", "))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Public convenience wrappers
// ═══════════════════════════════════════════════════════════════════

pub struct CompileSourceResult {
    pub rows: Vec<Vec<AstNode>>,
    pub json: Vec<Vec<JsonValue>>,
    pub warnings: Vec<String>,
}

pub fn compile_source(src: &str) -> Result<CompileSourceResult, CompileError> {
    let result = parse(src).map_err(|e| CompileError::new(format!("{e}")))?;
    let json = compile(&result.rows)?;
    validate_termux_json(&json)?;
    let warnings = collect_warnings(&json);
    Ok(CompileSourceResult { rows: result.rows, json, warnings })
}

pub fn to_extra_keys_json(src: &str) -> Result<String, CompileError> {
    let result = compile_source(src)?;
    Ok(stringify_json_rows(&result.json))
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_ident() {
        let tokens = tokenize("ESC").unwrap();
        assert_eq!(tokens.len(), 2); // IDENT + EOF
        assert_eq!(tokens[0], Token::Ident { value: "ESC".into(), line: 1, column: 1 });
    }

    #[test]
    fn tokenize_string_literal() {
        let tokens = tokenize("'hello'").unwrap();
        assert_eq!(tokens[0], Token::Literal { value: "hello".into(), line: 1, column: 1 });
    }

    #[test]
    fn tokenize_comment() {
        let tokens = tokenize("# this is a comment\nESC").unwrap();
        assert_eq!(tokens[0], Token::Newline { line: 1, column: 1 });
        assert_eq!(tokens[1], Token::Ident { value: "ESC".into(), line: 2, column: 1 });
    }

    #[test]
    fn parse_simple_key() {
        let result = parse("ESC").unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].len(), 1);
        assert_eq!(result.rows[0][0], AstNode::Key { key: "ESC".into(), kind: KeyKind::Named });
    }

    #[test]
    fn parse_special_key() {
        let result = parse("CTRL").unwrap();
        assert_eq!(result.rows[0][0], AstNode::Key { key: "CTRL".into(), kind: KeyKind::Special });
    }

    #[test]
    fn parse_combo() {
        let result = parse("CTRL(d)").unwrap();
        assert_eq!(result.rows[0].len(), 1);
        assert!(matches!(&result.rows[0][0], AstNode::Combo { .. }));
    }

    #[test]
    fn parse_label() {
        let result = parse("CTRL:C").unwrap();
        assert_eq!(result.rows[0].len(), 1);
        assert!(matches!(&result.rows[0][0], AstNode::Label { .. }));
    }

    #[test]
    fn parse_popup() {
        let result = parse("ESC.DRAWER").unwrap();
        assert_eq!(result.rows[0].len(), 1);
        assert!(matches!(&result.rows[0][0], AstNode::Popup { .. }));
    }

    #[test]
    fn parse_row_separator() {
        let result = parse("ESC;\nCTRL").unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn compile_simple() {
        let result = compile_source("ESC").unwrap();
        assert_eq!(result.json.len(), 1);
        assert_eq!(result.json[0][0], JsonValue::Str("ESC".into()));
    }

    #[test]
    fn compile_combo() {
        let result = compile_source("CTRL(d)").unwrap();
        assert!(matches!(&result.json[0][0], JsonValue::Object(m) if m.get("macro").is_some()));
    }

    #[test]
    fn compile_label() {
        let result = compile_source("CTRL:C").unwrap();
        if let JsonValue::Object(m) = &result.json[0][0] {
            assert_eq!(m.get("key").unwrap(), &JsonValue::Str("CTRL".into()));
            assert_eq!(m.get("display").unwrap(), &JsonValue::Str("C".into()));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn compile_popup() {
        let result = compile_source("ESC.DRAWER").unwrap();
        if let JsonValue::Object(m) = &result.json[0][0] {
            assert_eq!(m.get("key").unwrap(), &JsonValue::Str("ESC".into()));
            assert_eq!(m.get("popup").unwrap(), &JsonValue::Str("DRAWER".into()));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn round_trip() {
        let src = "CTRL(d); ESC:C";
        let result = compile_source(src).unwrap();
        let printed = print_rows(&result.rows);
        assert_eq!(printed, "CTRL(d);\nESC:C;");
    }

    #[test]
    fn global_height() {
        let result = parse("height=40\nESC").unwrap();
        assert_eq!(result.globals.len(), 1);
        assert_eq!(result.globals[0].key, "height");
        assert!(matches!(result.globals[0].value, GlobalValue::Number(40)));
    }

    #[test]
    fn global_theme() {
        let result = parse("theme=dark\nESC").unwrap();
        assert_eq!(result.globals.len(), 1);
        assert_eq!(result.globals[0].key, "theme");
        assert!(matches!(&result.globals[0].value, GlobalValue::Ident(s) if s == "dark"));
    }

    #[test]
    fn global_reload() {
        let result = parse("reload\nESC").unwrap();
        assert_eq!(result.globals.len(), 1);
        assert_eq!(result.globals[0].key, "reload");
        assert!(matches!(result.globals[0].value, GlobalValue::Bool(true)));
    }

    #[test]
    fn warnings_empty_display() {
        let mut obj = HashMap::new();
        obj.insert("key".to_string(), JsonValue::Str("ESC".into()));
        obj.insert("display".to_string(), JsonValue::Str("".into()));
        let json = vec![vec![JsonValue::Object(obj)]];
        let warnings = collect_warnings(&json);
        assert!(warnings.iter().any(|w| w.contains("display") && w.contains("为空")));
    }

    #[test]
    fn warnings_missing_drawer() {
        let json = vec![vec![JsonValue::Str("ESC".into())]];
        let warnings = collect_warnings(&json);
        assert!(warnings.iter().any(|w| w.contains("DRAWER")));
    }

    #[test]
    fn warnings_missing_keyboard() {
        let json = vec![vec![JsonValue::Str("ESC".into())]];
        let warnings = collect_warnings(&json);
        assert!(warnings.iter().any(|w| w.contains("KEYBOARD")));
    }

    #[test]
    fn validate_termux_json_rejects_key_and_macro() {
        let mut obj = HashMap::new();
        obj.insert("key".to_string(), JsonValue::Str("ESC".into()));
        obj.insert("macro".to_string(), JsonValue::Str("CTRL d".into()));
        let json = vec![vec![JsonValue::Object(obj)]];
        assert!(validate_termux_json(&json).is_err());
    }

    #[test]
    fn validate_termux_json_rejects_no_key_no_macro_no_display() {
        let mut obj = HashMap::new();
        obj.insert("popup".to_string(), JsonValue::Str("X".into()));
        let json = vec![vec![JsonValue::Object(obj)]];
        assert!(validate_termux_json(&json).is_err());
    }

    #[test]
    fn validate_termux_json_accepts_display_only() {
        let mut obj = HashMap::new();
        obj.insert("key".to_string(), JsonValue::Str("ESC".into()));
        obj.insert("display".to_string(), JsonValue::Str("X".into()));
        let json = vec![vec![JsonValue::Object(obj)]];
        assert!(validate_termux_json(&json).is_ok());
    }

    #[test]
    fn special_key_duplicate_in_row() {
        let result = parse("CTRL CTRL");
        assert!(result.is_err());
    }

    #[test]
    fn quoted_literal_preserves_case() {
        let result = compile_source("'d'").unwrap();
        assert_eq!(result.json[0][0], JsonValue::Str("d".into()));
    }

    #[test]
    fn stringify_bare_token() {
        let mut obj = HashMap::new();
        obj.insert("key".to_string(), JsonValue::Str("ESC".into()));
        let s = stringify_dsl(&JsonValue::Object(obj));
        assert_eq!(s, "{key: ESC}");
    }

    #[test]
    fn stringify_quoted_string() {
        let mut obj = HashMap::new();
        obj.insert("key".to_string(), JsonValue::Str("/".into()));
        let s = stringify_dsl(&JsonValue::Object(obj));
        assert_eq!(s, "{key: \"/\"}");
    }
}
