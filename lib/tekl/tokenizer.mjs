export const NAMED_KEYS = new Set([
  "ESC",
  "TAB",
  "HOME",
  "END",
  "PGUP",
  "PGDN",
  "INS",
  "DEL",
  "BKSP",
  "UP",
  "LEFT",
  "RIGHT",
  "DOWN",
  "ENTER",
  "BACKSLASH",
  "QUOTE",
  "APOSTROPHE",
  "F1",
  "F2",
  "F3",
  "F4",
  "F5",
  "F6",
  "F7",
  "F8",
  "F9",
  "F10",
  "F11",
  "F12",
  "KEYBOARD",
  "DRAWER",
  "SCROLL",
  "SPACE",
])

export const SPECIAL_KEYS = new Set(["CTRL", "ALT", "FN", "SHIFT"])

export class TokenizerError extends Error {
  constructor(message, line, column) {
    super(`tekl: ${message} (line ${line}, column ${column})`)
    this.line = line
    this.column = column
  }
}

export function tokenize(src) {
  const tokens = []
  let i = 0
  let line = 1
  let column = 1

  function error(msg) {
    throw new TokenizerError(msg, line, column)
  }

  while (i < src.length) {
    const c = src[i]

    if (c === "\n") {
      tokens.push({ type: "NEWLINE", line, column })
      line++
      column = 1
      i++
      continue
    }

    if (c === " " || c === "\t" || c === "\r") {
      column++
      i++
      continue
    }

    if (c === "#") {
      while (i < src.length && src[i] !== "\n") i++
      continue
    }

    if (c === ";") {
      tokens.push({ type: "SEMI", line, column })
      column++
      i++
      continue
    }

    if (c === ".") {
      tokens.push({ type: "DOT", line, column })
      column++
      i++
      continue
    }

    if (c === "(") {
      tokens.push({ type: "LPAREN", line, column })
      column++
      i++
      continue
    }

    if (c === ")") {
      tokens.push({ type: "RPAREN", line, column })
      column++
      i++
      continue
    }

    if (c === ":") {
      tokens.push({ type: "COLON", line, column })
      column++
      i++
      continue
    }

    if (/[A-Za-z0-9_]/.test(c)) {
      let j = i
      while (j < src.length && /[A-Za-z0-9_]/.test(src[j])) j++
      tokens.push({ type: "IDENT", value: src.slice(i, j), line, column })
      column += j - i
      i = j
      continue
    }

    error(`unexpected character '${c}'`)
  }

  tokens.push({ type: "EOF", line, column })
  return tokens
}
