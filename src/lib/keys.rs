use std::collections::HashSet;

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

impl Global {
    /// Serialize back to tekl source. The result is accepted by [`parse`], so globals
    /// survive a TUI save/load round trip.
    pub fn to_source(&self) -> String {
        match &self.value {
            GlobalValue::Bool(true) => self.key.clone(),
            GlobalValue::Bool(false) => format!("{} = false", self.key),
            GlobalValue::Number(n) => format!("{} = {}", self.key, n),
            GlobalValue::Ident(s) => format!("{} = {}", self.key, s),
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


/// Validates a parsed matrix using rules that are not expressible in the grammar.
///
/// - a combo must contain the modifier plus at least one key, and only plain keys
/// - a popup may not be nested
pub fn validate_compiled_rows(rows: &[Vec<AstNode>]) -> Result<(), CompileError> {
    for row in rows {
        for entry in row {
            validate_entry(entry)?;
        }
    }
    Ok(())
}

fn validate_entry(node: &AstNode) -> Result<(), CompileError> {
    match node {
        AstNode::Key { .. } => Ok(()),
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
            Ok(())
        }
        AstNode::Label { node, .. } => validate_entry(node),
        AstNode::Popup { main, popup_node } => {
            validate_entry(main)?;
            validate_entry(popup_node)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Compiler warnings (non-fatal, Chinese)
// ═══════════════════════════════════════════════════════════════════

pub fn collect_warnings(rows: &[Vec<AstNode>]) -> Vec<String> {
    let mut warnings = Vec::new();

    for (r, row) in rows.iter().enumerate() {
        for (c, entry) in row.iter().enumerate() {
            collect_empty_warnings(entry, r + 1, c + 1, false, &mut warnings);
        }
    }

    // Uneven row lengths
    let lengths: Vec<usize> = rows.iter().map(|row| row.len()).collect();
    if lengths.windows(2).any(|w| w[0] != w[1]) {
        let parts: Vec<String> = lengths
            .iter()
            .enumerate()
            .map(|(i, n)| format!("第{}行 {n} 键", i + 1))
            .collect();
        warnings.push(format!("各键盘行键位数不同: {}", parts.join(", ")));
    }

    // Missing DRAWER / KEYBOARD
    let mut names = Vec::new();
    for row in rows {
        for entry in row {
            collect_names(entry, &mut names);
        }
    }

    if !names.iter().any(|name| name == "DRAWER") {
        warnings.push("键盘方案中未找到应用抽屉键 (DRAWER)".to_string());
    }
    if !names.iter().any(|name| name == "KEYBOARD") {
        warnings.push("键盘方案中未找到键盘切换键 (KEYBOARD)".to_string());
    }

    warnings
}

fn collect_empty_warnings(
    node: &AstNode,
    row: usize,
    column: usize,
    in_popup: bool,
    warnings: &mut Vec<String>,
) {
    let suffix = if in_popup { "的 popup" } else { "" };

    match node {
        AstNode::Key { key, kind } => {
            if *kind == KeyKind::Literal && key.is_empty() {
                warnings.push(format!(
                    "第 {row} 键盘行第 {column} 键{suffix}: 'key' 的值为空"
                ));
            }
        }
        AstNode::Label { node, label } => {
            if label.is_empty() {
                warnings.push(format!(
                    "第 {row} 键盘行第 {column} 键{suffix}: 'display' 的值为空"
                ));
            }
            collect_empty_warnings(node, row, column, in_popup, warnings);
        }
        AstNode::Combo { .. } => {}
        AstNode::Popup { main, popup_node } => {
            collect_empty_warnings(main, row, column, in_popup, warnings);
            collect_empty_warnings(popup_node, row, column, true, warnings);
        }
    }
}

/// Collects the normalized key names of an entry (including its popup) so the
/// DRAWER/KEYBOARD presence warnings do not need a JSON representation.
fn collect_names(node: &AstNode, out: &mut Vec<String>) {
    match node {
        AstNode::Key { .. } => {
            if let Ok(name) = key_name(node) {
                out.push(name);
            }
        }
        AstNode::Label { node, .. } => collect_names(node, out),
        AstNode::Combo { node, inner } => {
            collect_names(node, out);
            collect_names(inner, out);
        }
        AstNode::Popup { main, popup_node } => {
            collect_names(main, out);
            collect_names(popup_node, out);
        }
    }
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
// Public convenience wrappers
// ═══════════════════════════════════════════════════════════════════


pub struct CompileSourceResult {
    pub rows: Vec<Vec<AstNode>>,
    pub globals: Vec<Global>,
    pub warnings: Vec<String>,
}

pub fn compile_source(src: &str) -> Result<CompileSourceResult, CompileError> {
    let result = parse(src).map_err(|e| match e {
        ParseError::UnexpectedToken { message, line, column } => {
            CompileError::new(format!("{message} (line {line}, column {column})"))
        }
    })?;
    validate_compiled_rows(&result.rows)?;
    let mut warnings = collect_warnings(&result.rows);

    // The Termux app accepts only the key matrix; G assignments are rejected there.
    // Keep them parseable for the TUI but tell the user they will not reach the app.
    for g in &result.globals {
        warnings.push(format!(
            "全局设置 '{}' 暂不被 Termux App 支持，写入布局时会被忽略",
            g.to_source()
        ));
    }

    Ok(CompileSourceResult { rows: result.rows, globals: result.globals, warnings })
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
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0][0],
            AstNode::Key { key: "ESC".into(), kind: KeyKind::Named }
        );
    }

    #[test]
    fn compile_combo() {
        let result = compile_source("CTRL(d)").unwrap();
        assert!(matches!(&result.rows[0][0], AstNode::Combo { .. }));
    }

    #[test]
    fn compile_label() {
        let result = compile_source("CTRL:C").unwrap();
        match &result.rows[0][0] {
            AstNode::Label { node, label } => {
                assert_eq!(label, "C");
                assert!(matches!(&**node, AstNode::Key { key, .. } if key == "CTRL"));
            }
            other => panic!("expected label, got {other:?}"),
        }
    }

    #[test]
    fn compile_popup() {
        let result = compile_source("ESC.DRAWER").unwrap();
        match &result.rows[0][0] {
            AstNode::Popup { main, popup_node } => {
                assert!(matches!(&**main, AstNode::Key { key, .. } if key == "ESC"));
                assert!(matches!(&**popup_node, AstNode::Key { key, .. } if key == "DRAWER"));
            }
            other => panic!("expected popup, got {other:?}"),
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
        let result = compile_source("ESC:").unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("display") && w.contains("为空")));
    }

    #[test]
    fn warnings_missing_drawer() {
        let result = compile_source("ESC").unwrap();
        assert!(result.warnings.iter().any(|w| w.contains("DRAWER")));
    }

    #[test]
    fn warnings_missing_keyboard() {
        let result = compile_source("ESC").unwrap();
        assert!(result.warnings.iter().any(|w| w.contains("KEYBOARD")));
    }

    #[test]
    fn warnings_global_settings_are_ignored() {
        let result = compile_source("height = 40\nESC").unwrap();
        assert_eq!(result.globals.len(), 1);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("height = 40") && w.contains("忽略")));
    }

    #[test]
    fn validate_rejects_label_inside_combo() {
        assert!(compile_source("CTRL(ESC:x)").is_err());
    }

    #[test]
    fn validate_rejects_popup_inside_combo() {
        assert!(compile_source("CTRL(ESC.DRAWER)").is_err());
    }

    #[test]
    fn validate_accepts_label_on_key() {
        assert!(compile_source("ESC:Quit").is_ok());
    }

    #[test]
    fn special_key_duplicate_in_row() {
        let result = parse("CTRL CTRL");
        assert!(result.is_err());
    }

    #[test]
    fn quoted_literal_preserves_case() {
        let result = compile_source("'d'").unwrap();
        assert_eq!(
            result.rows[0][0],
            AstNode::Key { key: "d".into(), kind: KeyKind::Literal }
        );
    }

}
