import { parseJsonDsl, JsonDslSyntaxError } from "./json-dsl.mjs"
import { validateExtraKeyJson, ValidationError } from "./validate-json.mjs"

/**
 * Render a detailed, human-readable error report for a termux extra-keys DSL.
 * Shows the exact source snippet with a caret, error type, and JSON path.
 */
export function formatDslError(source, err) {
  const lines = []
  lines.push(`✖ ${err.message}`)

  if (err instanceof JsonDslSyntaxError) {
    lines.push(`  type: syntax error at index ${err.index}`)
    const snippet = extractLine(source, err.index)
    if (snippet) lines.push(snippet)
    lines.push(`  hint: fix the highlighted character or complete the missing token`)
    return lines
  }
  return lines
}

export function snippetForIndex(source, index) {
  const upTo = source.slice(0, index)
  const lineStart = upTo.lastIndexOf("\n") + 1
  const lineEnd = source.indexOf("\n", index)
  const lineText = source.slice(lineStart, lineEnd === -1 ? source.length : lineEnd)
  return [lineText, index - lineStart]
}

export function snippet(lineno, lineText, col) {
  return `  ${String(lineno).padStart(3)} | ${lineText}\n      | ${" ".repeat(Math.max(0, col))}^`
}

export function formatFullReport(source, err, context = {}) {
  const parts = []

  if (err instanceof JsonDslSyntaxError) {
    parts.push(`SyntaxError: ${err.message}`)
    parts.push(`  location: line ${err.line}, column ${err.column} (char ${err.index})`)
    const { text, col } = lineColumn(source, err)
    parts.push(snippet(err.line, text, col))
    parts.push(`  hint: ${syntaxHint(err)}`)
  } else if (err instanceof ValidationError) {
    parts.push(`ValidationError: ${err.message}`)
    parts.push(`  path: ${err.path}`)
    if (context.json) {
      const loc = locatePath(context.json, err.path)
      if (loc) parts.push(`  value: ${perror(loc)}`)
    }
    parts.push(`  hint: ${structureHint(err)}`)
  } else {
    parts.push(`Error: ${err.message}`)
  }

  return parts.join("\n")
}

function lineColumn(source, err) {
  const lines = source.split("\n")
  const text = lines[err.line - 1] ?? ""
  return { text, col: err.column - 1 }
}

function perror(value) {
  if (typeof value === "string") return JSON.stringify(value)
  return JSON.stringify(value)
}

function locatePath(json, path) {
  const m = path.match(/^extra-keys(\[(\d+)\])?(\[(\d+)\])?(\.popup)?$/)
  if (!m) return undefined
  let node = json
  if (m[2] !== undefined) node = node && node[+m[2]]
  if (m[4] !== undefined) node = node && node[+m[4]]
  if (m[5]) node = node && node.popup
  return node
}

function syntaxHint(err) {
  if (/unterminated/.test(err.message)) return "add the missing closing bracket or quote"
  if (/unexpected character/.test(err.message)) return "this character is not allowed here"
  if (/expected ':'/.test(err.message)) return "object keys must be followed by ':'"
  if (/duplicate key/.test(err.message)) return "each key must appear at most once per object"
  if (/escape/.test(err.message)) return "only n, t, u, b, f, r, /, \\' are valid escapes"
  if (/after top-level/.test(err.message)) return "remove trailing content after the value"
  return "fix the JSON structure"
}

function structureHint(err) {
  if (/both 'key' and 'macro'/.test(err.message)) return "keep exactly one of key/macro per object"
  if (/key or 'macro/.test(err.message)) return "add a 'key' or 'macro'"
  if (/must be a string/.test(err.message)) return "wrap the value in quotes, e.g. display: 'CTRL'"
  if (/cannot be nested/.test(err.message)) return "remove the nested 'popup'"
  if (/row must be/.test(err.message)) return "make sure each row is a list"
  if (/must be a string or object/.test(err.message)) return "each key is a string or an object"
  return "fix the extra-key object"
}