import { tokenize, SPECIAL_KEYS } from "./tokenizer.mjs"

export class ParseError extends Error {
  constructor(message, line, column) {
    super(`tekl: ${message} (line ${line}, column ${column})`)
    this.line = line
    this.column = column
  }
}

export function formatParseError(source, err) {
  if (!(err instanceof ParseError)) return err.message
  const lines = source.split("\n")
  const text = lines[err.line - 1] ?? ""
  const col = Math.max(0, err.column - 1)
  const msg = err.message.replace(/^tekl: /, "")
  return [
    `tekl: ${msg}`,
    `  location: line ${err.line}, column ${err.column}`,
    `  ${String(err.line).padStart(3)} | ${text}`,
    `      | ${" ".repeat(col)}^`,
  ].join("\n")
}

function isSpecial(id) {
  return SPECIAL_KEYS.has(id)
}

// A ::= N | S | A(A) | A:N
// 尾随后缀可叠加: A(B):C  => 组合键(重命名外显 C)
export function parse(src) {
  const tokens = tokenize(src)
  const rows = []
  let row = []
  let i = 0

  function peek() {
    return tokens[i]
  }

  function next() {
    return tokens[i++]
  }

  function expect(type) {
    const tok = next()
    if (tok.type !== type) {
      throw new ParseError(`expected ${type}, got ${tok.type}`, tok.line, tok.column)
    }
    return tok
  }

  // N/S 基座:
  //   ESC, TAB, ..., CTRL, ALT, 任意 [A-Za-z0-9_-]*
  function parseBaseKey() {
    const tok = next()
    if (tok.type !== "IDENT") {
      throw new ParseError(`expected key, got ${tok.type}`, tok.line, tok.column)
    }
    return { type: "key", key: tok.value, kind: isSpecial(tok.value) ? "special" : "named" }
  }

  function parseAtom() {
    let node = parseBaseKey()

    while (true) {
      const t = peek()

      if (t.type === "LPAREN") {
        next()
        const inner = parseAtom()
        expect("RPAREN")
        node = { type: "combo", node, inner }
        continue
      }

      if (t.type === "COLON") {
        next()
        const labelTok = peek()
        if (labelTok.type === "IDENT") {
          next()
          node = { type: "label", node, label: labelTok.value }
        } else {
          node = { type: "label", node, label: "" }
        }
        continue
      }

      break
    }

    return node
  }

  // M ::= A | A.A | M M | M'\n' | M';\n'
  function parseEntry() {
    const main = parseAtom()
    if (peek().type === "DOT") {
      next()
      const popupNode = parseAtom()
      if (popupNode.type === "popup") {
        throw new ParseError("nested popup is not supported", popupNode.line, popupNode.column)
      }
      return { type: "popup", main, popupNode }
    }
    return main
  }

  while (true) {
    const tok = peek()

    if (tok.type === "EOF") break

    if (tok.type === "NEWLINE") {
      next()
      continue
    }

    if (tok.type === "SEMI") {
      next()
      if (row.length > 0) {
        rows.push(row)
        row = []
      }
      continue
    }

    if (tok.type === "IDENT") {
      row.push(parseEntry())
      continue
    }

    throw new ParseError(`unexpected token ${tok.type}`, tok.line, tok.column)
  }

  if (row.length > 0) rows.push(row)

  validateRows(rows)
  return rows
}

function validateRows(rows) {
  for (let r = 0; r < rows.length; r++) {
    const specials = new Map()
    for (const entry of rows[r]) {
      // 只统计"根按键"是特殊键的条目(组合/弹出里的内部键不算)
      const isKey = entry.type === "key" && entry.kind === "special"
      const isLabeledSpecial =
        entry.type === "label" && entry.node?.type === "key" && entry.node.kind === "special"
      const t = isKey ? entry : isLabeledSpecial ? entry.node : null
      if (t) {
        if (specials.has(t.key)) {
          throw new ParseError(`special key '${t.key}' appears more than once in row ${r + 1}`, 0, 0)
        }
        specials.set(t.key, true)
      }
    }
  }
}

// visit every leaf key in the parse tree (used for duplicate checks)
function visitEntries(entries, cb) {
  const walk = (n) => {
    if (n === undefined || n === null) return
    if (n.type === "key") cb(n)
    else if (n.type === "combo") {
      walk(n.node)
      walk(n.inner)
    } else if (n.type === "label") {
      walk(n.node)
    } else if (n.type === "popup") {
      walk(n.main)
    }
  }
  for (const e of entries) walk(e)
}