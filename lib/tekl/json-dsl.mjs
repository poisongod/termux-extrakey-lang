/**
 * JSON DSL parser replicating Android's org.json.JSONTokener semantics,
 * which Termux uses to parse the extra-keys value.
 *
 * Features (Android JSONTokener compatible):
 * - unquoted keys:            {key: ESC}
 * - unquoted string values:   {key: ESC}
 * - single or double quotes:  'ESC' / "ESC"
 * - numbers: 123, 3.14, 1e5, -2 (only when the token starts with '-' or a digit,
 *   so '+5' and '.5' stay strings, exactly like Android)
 * - booleans: true/false, null
 * - \uXXXX unicode escapes
 * - trailing commas in objects and arrays are allowed (Android accepts them)
 * - nested objects and arrays
 *
 * Also rejects Android-tokener-invalid syntax with precise positions.
 */

export class JsonDslSyntaxError extends Error {
  constructor(message, index, line, column) {
    super(`json-dsl: ${message} (line ${line}, column ${column})`)
    this.index = index
    this.line = line
    this.column = column
  }
}

const WHITESPACE = new Set([" ", "\t", "\r", "\n"])

export function parseJsonDsl(text) {
  const p = new Parser(text)
  const value = p.parseValue()
  p.skipWhitespace()
  if (!p.atEnd()) {
    p.error(`unexpected character '${p.peek()}' after top-level value`)
  }
  return value
}

class Parser {
  constructor(text) {
    this.text = text
    this.i = 0
    this.line = 1
    this.col = 1
  }

  peek(offset = 0) {
    return this.text[this.i + offset]
  }

  atEnd() {
    return this.i >= this.text.length
  }

  advance() {
    const c = this.text[this.i]
    this.i++
    if (c === "\n") {
      this.line++
      this.col = 1
    } else {
      this.col++
    }
    return c
  }

  error(message) {
    throw new JsonDslSyntaxError(message, this.i, this.line, this.col)
  }

  skipWhitespace() {
    while (!this.atEnd() && WHITESPACE.has(this.peek())) this.advance()
  }

  parseValue() {
    this.skipWhitespace()
    if (this.atEnd()) this.error("unexpected end of input, expected a value")

    const c = this.peek()
    if (c === "{") return this.parseObject()
    if (c === "[") return this.parseArray()
    if (c === '"' || c === "'") return this.parseQuotedString()
    return this.parseBareToken()
  }

  parseObject() {
    this.advance() // {
    const obj = {}

    this.skipWhitespace()
    if (this.peek() === "}") {
      this.advance()
      return obj
    }

    while (true) {
      this.skipWhitespace()
      if (this.atEnd()) this.error("unterminated object, expected key or '}'")

      const key = this.parseKey()
      if (Object.prototype.hasOwnProperty.call(obj, key)) {
        this.error(`duplicate key '${key}'`)
      }

      this.skipWhitespace()
      if (this.peek() !== ":") {
        this.error(`expected ':' after key '${key}', got '${this.peek()}'`)
      }
      this.advance()

      obj[key] = this.parseValue()

      this.skipWhitespace()
      if (this.atEnd()) this.error("unterminated object, expected ',' or '}'")
      const c = this.peek()
      if (c === ",") {
        this.advance()
        this.skipWhitespace()
        if (this.peek() === "}") {
          this.advance()
          return obj
        }
        continue
      }
      if (c === "}") {
        this.advance()
        return obj
      }
      this.error(`expected ',' or '}', got '${c}'`)
    }
  }

  parseKey() {
    const c = this.peek()
    if (c === '"' || c === "'") return this.parseQuotedString()
    return this.parseBareTokenString()
  }

  parseArray() {
    this.advance() // [
    const arr = []

    this.skipWhitespace()
    if (this.peek() === "]") {
      this.advance()
      return arr
    }

    while (true) {
      arr.push(this.parseValue())

      this.skipWhitespace()
      if (this.atEnd()) this.error("unterminated array, expected ',' or ']'")
      const c = this.peek()
      if (c === ",") {
        this.advance()
        this.skipWhitespace()
        if (this.peek() === "]") {
          this.advance()
          return arr
        }
        continue
      }
      if (c === "]") {
        this.advance()
        return arr
      }
      this.error(`expected ',' or ']', got '${c}'`)
    }
  }

  parseQuotedString() {
    const quote = this.advance()
    let out = ""

    while (true) {
      if (this.atEnd()) this.error(`unterminated string (missing closing ${quote})`)
      const c = this.advance()

      if (c === quote) return out

      if (c === "\\") {
        if (this.atEnd()) this.error("unterminated escape sequence")
        const esc = this.advance()
        switch (esc) {
          case "n": out += "\n"; break
          case "t": out += "\t"; break
          case "r": out += "\r"; break
          case "b": out += "\b"; break
          case "f": out += "\f"; break
          case "u": {
            let hex = ""
            for (let k = 0; k < 4; k++) {
              if (this.atEnd() || !/[0-9a-fA-F]/.test(this.peek())) {
                this.error("invalid \\u escape, expected 4 hex digits")
              }
              hex += this.advance()
            }
            out += String.fromCharCode(parseInt(hex, 16))
            break
          }
          case '"':
          case "'":
          case "\\":
          case "/":
            out += esc
            break
          default:
            this.error(`invalid escape sequence '\\${esc}'`)
        }
      } else {
        out += c
      }
    }
  }

  parseBareTokenString() {
    let token = ""
    while (!this.atEnd()) {
      const c = this.peek()
      if (WHITESPACE.has(c) || c === "{" || c === "}" || c === "[" || c === "]" || c === "," || c === ":") {
        break
      }
      token += this.advance()
    }
    if (token.length === 0) {
      this.error(`unexpected character '${this.peek()}'`)
    }
    return token
  }

  parseBareToken() {
    const token = this.parseBareTokenString()

    if (token === "true") return true
    if (token === "false") return false
    if (token === "null") return null

    const first = token[0]
    const looksNumeric = first === "-" || (first >= "0" && first <= "9")
    if (looksNumeric && /^-?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/.test(token)) {
      return Number(token)
    }

    return token
  }
}
