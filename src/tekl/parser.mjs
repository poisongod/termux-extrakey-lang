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

  function isMacroAhead() {
    let j = i
    let sawIdent = false
    while (tokens[j].type === "IDENT") {
      sawIdent = true
      j++
    }
    return sawIdent && tokens[j].type === "COLON"
  }

  function parseKey() {
    const tok = next()
    if (tok.type !== "IDENT") {
      throw new ParseError(`expected key, got ${tok.type}`, tok.line, tok.column)
    }


    const key = tok.value
    const kind = isSpecial(key) ? "special" : "named"

    const nextTok = peek()

    if (nextTok.type === "DOT") {
      next()
      const popupTok = next()
      if (popupTok.type !== "IDENT") {
        throw new ParseError(`expected key after '.', got ${popupTok.type}`, popupTok.line, popupTok.column)
      }

      return { type: "popup", key, popup: popupTok.value }
    }

    if (nextTok.type === "LPAREN") {
      next()
      const innerTok = next()
      if (innerTok.type !== "IDENT") {
        throw new ParseError(`expected key inside '()', got ${innerTok.type}`, innerTok.line, innerTok.column)
      }

      expect("RPAREN")
      return { type: "combo", modifier: key, target: innerTok.value }
    }

    if (nextTok.type === "COLON") {
      next()
      const labelTok = next()
      if (labelTok.type !== "IDENT") {
        throw new ParseError(`expected label after ':', got ${labelTok.type}`, labelTok.line, labelTok.column)
      }
      return { type: "label", key, label: labelTok.value }
    }

    return { type: "key", key, kind }
  }

  function parseMacro() {
    const keys = []
    while (peek().type === "IDENT") {
      keys.push(next().value)
    }

    expect("COLON")

    if (keys.length === 1) {
      const labelTok = next()
      if (labelTok.type !== "IDENT") {
        throw new ParseError(`expected label after ':', got ${labelTok.type}`, labelTok.line, labelTok.column)
      }
      return { type: "label", key: keys[0], label: labelTok.value }
    }

    const labels = []
    while (peek().type === "IDENT") {
      labels.push(next().value)
    }

    if (labels.length === 0) {
      const tok = peek()
      throw new ParseError(`expected label after ':' in macro, got ${tok.type}`, tok.line, tok.column)
    }

    return { type: "macro", keys, label: labels.join(" ") }
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
      if (isMacroAhead()) {
        row.push(parseMacro())
      } else {
        row.push(parseKey())
      }
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
    for (const key of rows[r]) {
      if (key.kind === "special" && key.type === "key") {
        if (specials.has(key.key)) {
          throw new ParseError(`special key '${key.key}' appears more than once in row ${r + 1}`, 0, 0)
        }
        specials.set(key.key, true)
      }
    }
  }
}
