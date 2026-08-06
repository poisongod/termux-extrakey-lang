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
  return SPECIAL_KEYS.has(id.toUpperCase())
}

// A ::= N | S | A(A) | A:N
export function parse(src) {
  const tokens = tokenize(src)
  const rows = []
  let row = []
  let i = 0

  const peek = () => tokens[i]
  const next = () => tokens[i++]

  function expect(type) {
    const tok = next()
    if (tok.type !== type) {
      throw new ParseError(`expected ${type}, got ${tok.type}`, tok.line, tok.column)
    }
    return tok
  }

  function parseBaseKey() {
    const tok = next()
    if (tok.type === "IDENT") {
      return {
        type: "key",
        key: tok.value,
        kind: isSpecial(tok.value) ? "special" : "named",
      }
    }
    if (tok.type === "LITERAL") {
      return { type: "key", key: tok.value, kind: "literal" }
    }
    throw new ParseError(`expected key, got ${tok.type}`, tok.line, tok.column)
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
        if (labelTok.type === "LITERAL") {
          throw new ParseError("label must not be quoted", labelTok.line, labelTok.column)
        }
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

    if (tok.type === "IDENT" || tok.type === "LITERAL") {
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
      const isKey = entry.type === "key" && entry.kind === "special"
      const isLabeledSpecial =
        entry.type === "label" && entry.node?.type === "key" && entry.node.kind === "special"
      const t = isKey ? entry : isLabeledSpecial ? entry.node : null
      if (t) {
        const upper = t.key.toUpperCase()
        if (specials.has(upper)) {
          throw new ParseError(`special key '${t.key}' appears more than once in row ${r + 1}`, 0, 0)
        }
        specials.set(upper, true)
      }
    }
  }
}