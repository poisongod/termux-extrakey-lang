/**
 * Serialize to the relaxed JSON dialect Android's org.json.JSONTokener accepts,
 * which Termux uses to parse the extra-keys value (mirror of json-dsl.mjs).
 *
 * - object keys are always bare:  {key: ESC, popup: DRAWER}
 * - string values are bare when the bare token round-trips exactly:
 *   identifiers like ESC / HOME / copy / A-j
 *   (not starting with a digit or '-', not true/false/null, no separators)
 * - otherwise values are quoted JSON strings: "/", "~", "退出", "CTRL d"
 */

const BARE_TOKEN = /^[A-Za-z_][A-Za-z0-9_-]*$/
const KEYWORDS = new Set(["true", "false", "null"])

function quoteString(s) {
  let out = '"'
  for (const ch of s) {
    switch (ch) {
      case '"': out += '\\"'; break
      case "\\": out += "\\\\"; break
      case "\n": out += "\\n"; break
      case "\t": out += "\\t"; break
      case "\r": out += "\\r"; break
      case "\b": out += "\\b"; break
      case "\f": out += "\\f"; break
      default:
        if (ch < " ") out += `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`
        else out += ch
    }
  }
  return out + '"'
}

function stringifyValue(v) {
  if (typeof v === "string") {
    if (v.length > 0 && BARE_TOKEN.test(v) && !KEYWORDS.has(v)) return v
    return quoteString(v)
  }
  if (typeof v === "number") return Number.isFinite(v) ? String(v) : "null"
  if (typeof v === "boolean") return String(v)
  if (v === null) return "null"
  if (Array.isArray(v)) return `[${v.map(stringifyValue).join(", ")}]`
  if (typeof v === "object") return stringifyObject(v)
  return "null"
}

function stringifyObject(obj) {
  const parts = []
  for (const [k, v] of Object.entries(obj)) {
    parts.push(`${k}: ${stringifyValue(v)}`)
  }
  return `{${parts.join(", ")}}`
}

export function stringifyDsl(value) {
  return stringifyValue(value)
}
