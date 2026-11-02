//! ttoll behavior language — parser + static checks.
//!
//! Grammar (from docs/ttoll.ebnf):
//!   M = '0x01' | '0x02' | '0x03'                        (mode states)
//!   K = 'ESC' | 'ALT' | 'CTRL' | 'SPACE' | 'ENTER'      (function keys)
//!   D = 'UP' | 'DOWN' | 'LEFT' | 'RIGHT'                 (direction keys)
//!   ID = M | /0x[0-9A-Fa-f]{2}/                           (action ID)
//!   C = K ',' C | K | '{' C '}'                           (key sequence)
//!   R = M ',' C '>' ID | M ',' D '>' ID | C '>' ID | D '>' ID
//!   P = { R '\n' }                                        (program)

use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

// ── Error ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BehaviorError {
    #[error("behavior: {0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, BehaviorError>;

// ── Constants ────────────────────────────────────────────────────────

pub const MODE_0x01: &str = "0x01";
pub const MODE_0x02: &str = "0x02";
pub const MODE_0x03: &str = "0x03";

pub const MODES: &[&str] = &[MODE_0x01, MODE_0x02, MODE_0x03];

pub const FUNC_KEYS: &[&str] = &["ESC", "ALT", "CTRL", "SPACE", "ENTER"];
pub const DIR_KEYS: &[&str] = &["UP", "DOWN", "LEFT", "RIGHT"];

/// All recognized key names (K ∪ D).
pub fn all_keys() -> HashSet<&'static str> {
    FUNC_KEYS.iter().chain(DIR_KEYS.iter()).copied().collect()
}

/// 该键名是否属于文法词汇。
/// 仅 K ∪ D 共 9 个键名合法；`CTRL+x`/`ALT+x` 和弦中 x 也必须是文法键
/// （`CTRL+SPACE`、`ALT+LEFT` 合法；`CTRL+C` 不合法 — C 不在 K∪D）。
pub fn is_known_key(name: &str) -> bool {
    if all_keys().contains(name) {
        return true;
    }
    if let Some(rest) = name.strip_prefix("CTRL+").or_else(|| name.strip_prefix("ALT+")) {
        return all_keys().contains(rest);
    }
    false
}

fn is_mode(s: &str) -> bool {
    MODES.contains(&s)
}

/// Test if a string matches `0x[0-9A-Fa-f]{2}`.
fn is_hex_mode(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 4
        && bytes[0] == b'0'
        && bytes[1] == b'x'
        && bytes[2].is_ascii_hexdigit()
        && bytes[3].is_ascii_hexdigit()
}

/// Test if a line is a grammar header like `K = …` or `D = …`.
fn is_grammar_header(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_uppercase()
        && (bytes[1] == b' ' || bytes[1] == b'=' || bytes[1] == b'\t')
}

// ── Data types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub mode: Option<String>,
    pub steps: Vec<Step>,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub keys: HashSet<String>,
    pub star: bool,
    pub block: bool,
}

// ── Rule parsing ─────────────────────────────────────────────────────

/// Parse a single rule line into an `Option<Rule>`.
///
/// Lines that are empty, comments `(*…*)`, or grammar headers (`K = …`) return `None`.
pub fn tokenize_rule(line: &str) -> Option<Rule> {
    let s = line.trim();
    if s.is_empty() || s.starts_with("(*") {
        return None;
    }
    if is_grammar_header(s) {
        return None;
    }

    // Find the `>` separator (the last one that isn't inside braces).
    let gt_pos = find_gt_position(s)?;

    let (lhs, action) = s.split_at(gt_pos);
    let action = &action[1..]; // skip '>'

    if lhs.trim().is_empty() || action.trim().is_empty() {
        panic!("invalid rule: {:?}", line);
    }

    // Check if lhs starts with a mode prefix: `0xHH,rest`
    if let Some(comma_pos) = lhs.find(',') {
        let prefix = &lhs[..comma_pos];
        if is_hex_mode(prefix) {
            let mode = prefix;
            if !is_mode(mode) {
                panic!(
                    "invalid mode '{}': must be 0x01, 0x02, or 0x03",
                    mode
                );
            }
            let rest = &lhs[comma_pos + 1..];
            if rest.trim().is_empty() {
                panic!("invalid rule: {:?}", line);
            }
            let steps = split_steps(rest);
            if steps.is_empty() {
                panic!("invalid rule (empty steps): {:?}", line);
            }
            let steps = steps.iter().map(|s| parse_step(s)).collect();
            return Some(Rule {
                mode: Some(mode.to_string()),
                steps,
                action: action.trim().to_string(),
            });
        }
    }

    // No mode prefix
    let steps = split_steps(lhs);
    if steps.is_empty() {
        panic!("invalid rule (empty steps): {:?}", line);
    }
    let steps = steps.iter().map(|s| parse_step(s)).collect();
    Some(Rule {
        mode: None,
        steps,
        action: action.trim().to_string(),
    })
}

/// Find the position of `>` that acts as the rule separator (not inside braces).
fn find_gt_position(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            '>' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split a comma-separated step string, respecting `{…}` nesting.
fn split_steps(rest: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut cur = String::new();
    let mut depth: i32 = 0;

    for ch in rest.chars() {
        match ch {
            '{' => {
                if depth > 0 {
                    panic!("nested block: {}", rest);
                }
                depth = 1;
                cur.push(ch);
            }
            '}' => {
                if depth != 1 {
                    panic!("unbalanced block: {}", rest);
                }
                depth = 0;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                if !cur.is_empty() {
                    segs.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }

    if depth != 0 {
        panic!("unbalanced block: {}", rest);
    }
    if !cur.is_empty() {
        segs.push(cur);
    }
    segs
}

/// Parse a single step segment → `Step`.
fn parse_step(seg: &str) -> Step {
    if seg.starts_with('{') && seg.ends_with('}') {
        let inner = &seg[1..seg.len() - 1];
        let keys = parse_key_set(inner);
        return Step {
            keys,
            star: true,
            block: true,
        };
    }

    let star = seg.ends_with('*') || seg.contains('+');
    let k = if star && !seg.contains('+') { &seg[..seg.len() - 1] } else { seg };
    let keys = expand_key(k).into_iter().collect();

    // Chord keys (containing '+') are treated as block steps
    let is_chord = k.contains('+');

    Step {
        keys,
        star,
        block: is_chord,
    }
}

/// Parse an inner key set (comma-separated names) → `HashSet<String>`.
fn parse_key_set(inner: &str) -> HashSet<String> {
    let items: Vec<&str> = inner
        .split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect();

    if items.is_empty() {
        panic!("empty set: '{}'", inner);
    }

    let mut keys = HashSet::new();
    for item in items {
        for k in expand_key(item) {
            keys.insert(k);
        }
    }
    keys
}

/// Expand a single key name through non-terminals (K → FUNC_KEYS, D → DIR_KEYS).
///
/// Also handles chord keys like "CTRL+SPACE" → set containing both "CTRL" and "SPACE".
/// 和弦的每一 part 都必须是文法键 (K ∪ D)；`CTRL+C` 这类含非文法键的和弦直接拒绝。
fn expand_key(name: &str) -> Vec<String> {
    // Handle chord keys: "CTRL+SPACE" → ["CTRL", "SPACE"]
    if name.contains('+') {
        let parts: Vec<String> = name.split('+').map(|s| s.trim().to_uppercase()).collect();
        if parts.len() > 1 && parts.iter().all(|p| !p.is_empty()) {
            for p in &parts {
                if !all_keys().contains(p.as_str()) {
                    panic!("unknown chord key '{p}': must be a grammar key (K ∪ D)");
                }
            }
            return parts;
        }
    }

    match name {
        "K" => FUNC_KEYS.iter().map(|s| s.to_string()).collect(),
        "D" => DIR_KEYS.iter().map(|s| s.to_string()).collect(),
        other => {
            if all_keys().contains(other) {
                vec![other.to_string()]
            } else {
                panic!("unknown key '{}'", other);
            }
        }
    }
}

// ── Program parsing ──────────────────────────────────────────────────

/// Parse all non-empty rule lines from `source` into a `Vec<Rule>`.
pub fn parse_rules(source: &str) -> Vec<Rule> {
    source
        .lines()
        .filter_map(|line| tokenize_rule(line))
        .collect()
}

// ── Table & reachability ─────────────────────────────────────────────

/// Group rules by mode (`None` rules keyed as `"*"`).
pub fn build_table(rules: &[Rule]) -> HashMap<Option<String>, Vec<Rule>> {
    let mut table: HashMap<Option<String>, Vec<Rule>> = HashMap::new();
    for r in rules {
        table
            .entry(r.mode.clone())
            .or_default()
            .push(r.clone());
    }
    table
}

/// BFS reachability from mode 0x01 through action-ID mode jumps.
pub fn reachability(table: &HashMap<Option<String>, Vec<Rule>>) -> HashSet<String> {
    // Build adjacency: mode → set of destination modes (via action IDs that are hex modes).
    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
    for mode_key in table.keys() {
        if let Some(m) = mode_key {
            edges.entry(m.clone()).or_default();
        }
    }

    for rules in table.values() {
        for r in rules {
            if is_hex_mode(&r.action) {
                if let Some(ref mode) = r.mode {
                    edges
                        .entry(mode.clone())
                        .or_default()
                        .insert(r.action.clone());
                }
            }
        }
    }

    // BFS from 0x01
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(MODE_0x01.to_string());

    while let Some(cur) = queue.pop_front() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if let Some(nexts) = edges.get(&cur) {
            for next in nexts {
                if !seen.contains(next) {
                    queue.push_back(next.clone());
                }
            }
        }
    }

    seen
}

/// Static validation: keys valid at parse time, action IDs in whitelist, modes reachable.
pub fn check_program(source: &str, action_ids: Option<&HashSet<String>>) -> Result<CheckResult> {
    let rules = parse_rules(source);
    let table = build_table(&rules);
    let reachable = reachability(&table);

    for r in &rules {
        if let Some(ids) = action_ids {
            if !ids.is_empty() && !ids.contains(&r.action) {
                return Err(BehaviorError::Msg(format!(
                    "unknown action id '{}'",
                    r.action
                )));
            }
        }
        if let Some(ref mode) = r.mode {
            if !reachable.contains(mode) {
                return Err(BehaviorError::Msg(format!(
                    "mode '{}' unreachable from mode 0",
                    mode
                )));
            }
        }
    }

    Ok(CheckResult { rules, table })
}

#[derive(Debug)]
pub struct CheckResult {
    pub rules: Vec<Rule>,
    pub table: HashMap<Option<String>, Vec<Rule>>,
}

// ── Dispatch lookup ──────────────────────────────────────────────────

/// Look up the action ID for a given mode + key combination.
///
/// Returns `None` if no rule matches.
///
/// The lookup order:
/// 1. Mode-specific rules (e.g., `0x01,ESC>0x02`)
/// 2. Global rules (e.g., `ESC>0x01`)
pub fn dispatch(
    table: &HashMap<Option<String>, Vec<Rule>>,
    current_mode: &str,
    key: &str,
) -> Option<String> {
    let key_upper = key.to_uppercase();

    // 1. Check mode-specific rules first
    if let Some(mode_rules) = table.get(&Some(current_mode.to_string())) {
        for rule in mode_rules {
            for step in &rule.steps {
                // Skip chord steps (block) — they need dispatch_chord
                if step.block {
                    continue;
                }
                if step.keys.contains(key_upper.as_str()) {
                    return Some(rule.action.clone());
                }
            }
        }
    }

    // 2. Check global (mode-less) rules
    if let Some(global_rules) = table.get(&None) {
        for rule in global_rules {
            for step in &rule.steps {
                // Skip chord steps (block) — they need dispatch_chord
                if step.block {
                    continue;
                }
                if step.keys.contains(key_upper.as_str()) {
                    return Some(rule.action.clone());
                }
            }
        }
    }

    None
}

/// Look up the action ID for a key chord (e.g., "CTRL+SPACE").
///
/// Chord rules use `{...}` block steps with star=true.
pub fn dispatch_chord(
    table: &HashMap<Option<String>, Vec<Rule>>,
    current_mode: &str,
    chord_keys: &[String],
) -> Option<String> {
    let chord_upper: HashSet<String> = chord_keys.iter().map(|k| k.to_uppercase()).collect();

    // Check mode-specific rules
    if let Some(mode_rules) = table.get(&Some(current_mode.to_string())) {
        for rule in mode_rules {
            for step in &rule.steps {
                if step.block && step.star && chord_upper == step.keys {
                    return Some(rule.action.clone());
                }
            }
        }
    }

    // Check global rules
    if let Some(global_rules) = table.get(&None) {
        for rule in global_rules {
            for step in &rule.steps {
                if step.block && step.star && chord_upper == step.keys {
                    return Some(rule.action.clone());
                }
            }
        }
    }

    None
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn hs<const N: usize>(keys: [&str; N]) -> HashSet<String> {
        keys.into_iter().map(String::from).collect()
    }

    #[test]
    fn tokenize_simple_rule() {
        let r = tokenize_rule("0x01,ESC>0x02").unwrap();
        assert_eq!(r.mode.as_deref(), Some("0x01"));
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.steps[0].keys, hs(["ESC"]));
        assert!(!r.steps[0].star);
        assert!(!r.steps[0].block);
        assert_eq!(r.action, "0x02");
    }

    #[test]
    fn tokenize_no_mode_rule() {
        let r = tokenize_rule("UP>0x03").unwrap();
        assert_eq!(r.mode, None);
        assert_eq!(r.steps[0].keys, hs(["UP"]));
        assert_eq!(r.action, "0x03");
    }

    #[test]
    fn tokenize_block_step() {
        let r = tokenize_rule("0x01,SPACE,{D},SPACE>0x09").unwrap();
        assert_eq!(r.steps.len(), 3);
        assert!(!r.steps[0].block);
        assert!(r.steps[1].block);
        assert!(r.steps[1].star);
        assert_eq!(r.steps[1].keys, hs(["UP", "DOWN", "LEFT", "RIGHT"]));
    }

    #[test]
    fn tokenize_star_step() {
        let r = tokenize_rule("0x01,ESC*>0x02").unwrap();
        assert!(r.steps[0].star);
        assert!(!r.steps[0].block);
        assert_eq!(r.steps[0].keys, hs(["ESC"]));
    }

    #[test]
    fn tokenize_comment_skipped() {
        assert!(tokenize_rule("(* comment *)").is_none());
        assert!(tokenize_rule("").is_none());
    }

    #[test]
    fn tokenize_grammar_header_skipped() {
        assert!(tokenize_rule("K = ESC | ALT").is_none());
        assert!(tokenize_rule("D = UP | DOWN").is_none());
    }

    #[test]
    fn parse_rules_multiline() {
        let src = "0x01,ESC>0x02\nUP>0x03\n(* skip *)\n";
        let rules = parse_rules(src);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn grammar_vocabulary_is_frozen() {
        // 文法冻结: 键词汇 = K ∪ D 共 9 个; 字母键不属于文法
        assert_eq!(all_keys().len(), 9);
        for l in ["h", "j", "k", "l", "i", "G", "x", "?", "TAB", "BKSP"] {
            assert!(!all_keys().contains(l), "{l} 不在文法内");
        }
    }

    #[test]
    fn multi_step_sequences_parse() {
        let r = tokenize_rule("0x01,CTRL,CTRL>0x12").unwrap();
        assert_eq!(r.steps.len(), 2);
        assert!(r.steps[0].keys.contains("CTRL"));
        assert!(r.steps[1].keys.contains("CTRL"));
    }

    #[test]
    fn editor_rules_conform_to_grammar() {
        // editor.ttoll 全部规则形态 — 仅使用文法词汇 (K ∪ D) 与文法键和弦
        let src = "\
0x01,LEFT>0x01\n\
0x01,RIGHT>0x02\n\
0x01,DOWN>0x03\n\
0x01,UP>0x04\n\
0x01,CTRL>0x05\n\
0x01,ALT>0x06\n\
0x01,ENTER>0x07\n\
0x01,SPACE>0x08\n\
0x01,ESC>0x00\n\
0x01,ALT,LEFT>0x0D\n\
0x01,SPACE,SPACE>0x11\n\
0x02,ESC>0x09\n\
0x02,ENTER>0x0B\n\
0x03,ENTER>0x13\n";
        for line in src.lines() {
            assert!(tokenize_rule(line).is_some(), "规则不符合文法: {line}");
        }
    }

    #[test]
    fn grammar_chord_is_grammar_keys_only() {
        // 和弦只能是文法键组合: CTRL+SPACE / ALT+LEFT 合法
        assert!(tokenize_rule("CTRL+SPACE>0x00").is_some());
        assert!(tokenize_rule("ALT+LEFT>0x00").is_some());
        assert!(is_known_key("CTRL+SPACE"));
        assert!(is_known_key("ALT+LEFT"));

        // 非文法键和弦必须被拒绝: C/S/E/H/G 不属于 K ∪ D
        assert!(!is_known_key("CTRL+C"));
        assert!(!is_known_key("CTRL+S"));
        assert!(!is_known_key("ALT+H"));
        assert!(std::panic::catch_unwind(|| tokenize_rule("CTRL+C>0x00")).is_err());
        assert!(std::panic::catch_unwind(|| tokenize_rule("CTRL+S>0x15")).is_err());
    }

    #[test]
    fn repo_editor_ttoll_conforms() {
        // 仓库根 editor.ttoll — 提供用户目录模板; 每一行必须能被 tokenize（注释返回 None 也是合法）
        let src = include_str!("../../editor.ttoll");
        for line in src.lines() {
            let s = line.trim();
            if s.is_empty() || s.starts_with("(*") {
                continue;
            }
            assert!(tokenize_rule(s).is_some(), "editor.ttoll 不符合文法: {line}");
        }
    }

    #[test]
    fn repo_tekl_ttoll_conforms_and_targets_registered() {
        // 仓库根 tekl.ttoll — 布局 TUI 的出厂接线，随 Release 发放。
        // 每一行都要符合文法，且目标 ID 必须在内置 tui 注册表里，否则首次运行按键无效。
        let src = include_str!("../../tekl.ttoll");
        let registry = crate::lib::id::IdRegistry::for_tui();
        let mut count = 0;
        for line in src.lines() {
            let s = line.trim();
            if s.is_empty() || s.starts_with("(*") {
                continue;
            }
            let rule = tokenize_rule(s).unwrap_or_else(|| panic!("tekl.ttoll 不符合文法: {line}"));
            assert!(
                registry.is_registered(&rule.action),
                "tekl.ttoll 指向未注册的 ID: {} ({line})",
                rule.action
            );
            count += 1;
        }
        assert!(count > 0, "tekl.ttoll 不应为空");
    }

    #[test]
    fn expand_k_nonterminal() {
        let r = tokenize_rule("0x01,K>0x02").unwrap();
        assert_eq!(r.steps[0].keys, hs(["ESC", "ALT", "CTRL", "SPACE", "ENTER"]));
    }

    #[test]
    fn expand_d_nonterminal() {
        let r = tokenize_rule("0x01,D>0x02").unwrap();
        assert_eq!(r.steps[0].keys, hs(["UP", "DOWN", "LEFT", "RIGHT"]));
    }

    #[test]
    fn build_table_groups() {
        let rules = parse_rules("0x01,ESC>0x02\nUP>0x03\n");
        let table = build_table(&rules);
        assert_eq!(table.len(), 2); // Some("0x01") and None
    }

    #[test]
    fn reachability_basic() {
        let rules = parse_rules("0x01,ESC>0x02\n0x02,ESC>0x03\n");
        let table = build_table(&rules);
        let reachable = reachability(&table);
        assert!(reachable.contains("0x01"));
        assert!(reachable.contains("0x02"));
        assert!(reachable.contains("0x03"));
    }

    #[test]
    fn reachability_unreachable_mode() {
        // 0x03 is never a target of a rule action
        let rules = parse_rules("0x01,ESC>0x02\n0x03,ESC>0x01\n");
        let table = build_table(&rules);
        let reachable = reachability(&table);
        assert!(reachable.contains("0x01"));
        assert!(!reachable.contains("0x03"));
    }

    #[test]
    fn check_program_valid() {
        let src = "0x01,ESC>0x02\n0x02,ESC>0x03\n";
        let ids: HashSet<String> = ["0x02".into(), "0x03".into()].into();
        let result = check_program(src, Some(&ids)).unwrap();
        assert_eq!(result.rules.len(), 2);
    }

    #[test]
    fn check_program_unknown_action() {
        let src = "0x01,ESC>0xFF\n";
        let ids: HashSet<String> = ["0x02".into()].into();
        let err = check_program(src, Some(&ids)).unwrap_err();
        assert!(err.to_string().contains("unknown action id"));
    }

    #[test]
    fn check_program_unreachable_mode() {
        let src = "0x03,ESC>0x01\n";
        let err = check_program(src, None).unwrap_err();
        assert!(err.to_string().contains("unreachable"));
    }

    #[test]
    fn split_steps_nesting() {
        let segs = split_steps("A,{B,C},D");
        assert_eq!(segs, vec!["A", "{B,C}", "D"]);
    }

    #[test]
    #[should_panic(expected = "nested block")]
    fn split_steps_nested_block() {
        split_steps("{{A}}");
    }

    #[test]
    #[should_panic(expected = "unbalanced block")]
    fn split_steps_unbalanced() {
        split_steps("{A");
    }

    #[test]
    #[should_panic(expected = "unknown key")]
    fn expand_unknown_key() {
        expand_key("NOTAKEY");
    }

    #[test]
    fn find_gt_basic() {
        assert_eq!(find_gt_position("A>B"), Some(1));
        assert_eq!(find_gt_position("{A}>B"), Some(3));
        assert_eq!(find_gt_position("A>{B}"), Some(1));
        assert_eq!(find_gt_position("nope"), None);
    }

    #[test]
    fn dispatch_mode_specific() {
        let src = "0x01,ESC>0x02\n0x01,UP>0x03\n";
        let rules = parse_rules(src);
        let table = build_table(&rules);
        assert_eq!(dispatch(&table, "0x01", "ESC"), Some("0x02".into()));
        assert_eq!(dispatch(&table, "0x01", "UP"), Some("0x03".into()));
    }

    #[test]
    fn dispatch_global_fallback() {
        let src = "ESC>0x01\n0x01,UP>0x02\n";
        let rules = parse_rules(src);
        let table = build_table(&rules);
        assert_eq!(dispatch(&table, "0x01", "ESC"), Some("0x01".into()));
        assert_eq!(dispatch(&table, "0x01", "UP"), Some("0x02".into()));
    }

    #[test]
    fn dispatch_no_match() {
        let src = "0x01,ESC>0x02\n";
        let rules = parse_rules(src);
        let table = build_table(&rules);
        assert_eq!(dispatch(&table, "0x01", "F1"), None);
    }

    #[test]
    fn dispatch_mode_specific_over_global() {
        let src = "ESC>0x01\n0x01,ESC>0x02\n";
        let rules = parse_rules(src);
        let table = build_table(&rules);
        assert_eq!(dispatch(&table, "0x01", "ESC"), Some("0x02".into()));
    }

    #[test]
    fn dispatch_chord_match() {
        let src = "0x01,{CTRL,SPACE}>0x09\n";
        let rules = parse_rules(src);
        let table = build_table(&rules);
        let chord = vec!["CTRL".into(), "SPACE".into()];
        assert_eq!(dispatch_chord(&table, "0x01", &chord), Some("0x09".into()));
    }

    #[test]
    fn dispatch_chord_no_match() {
        let src = "0x01,{CTRL,SPACE}>0x09\n";
        let rules = parse_rules(src);
        let table = build_table(&rules);
        let chord = vec!["CTRL".into(), "ALT".into()];
        assert_eq!(dispatch_chord(&table, "0x01", &chord), None);
    }

    #[test]
    fn is_hex_mode_works() {
        assert!(is_hex_mode("0x01"));
        assert!(is_hex_mode("0xFF"));
        assert!(is_hex_mode("0xab"));
        assert!(!is_hex_mode("0x1"));
        assert!(!is_hex_mode("0x123"));
        assert!(!is_hex_mode("hello"));
    }
}
