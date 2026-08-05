export class ValidationError extends Error {
  constructor(path, message) {
    super(`termux extra-keys: ${path}: ${message}`)
    this.path = path
  }
}

const KEY = "key"
const MACRO = "macro"
const DISPLAY = "display"
const POPUP = "popup"
const ALLOWED_FIELDS = new Set(["key", "macro", "display", "popup"])

function jsonType(value) {
  if (value === null) return "null"
  if (Array.isArray(value)) return "array"
  return typeof value
}

export function validateExtraKeyJson(json, path = "extra-keys") {
  if (!Array.isArray(json)) {
    throw new ValidationError(path, `must be a JSON array of key rows, got ${jsonType(json)}`)
  }
  if (json.length === 0) {
    throw new ValidationError(path, "no rows defined (empty keyboard)")
  }

  for (let r = 0; r < json.length; r++) {
    const row = json[r]
    const rowPath = `${path}[${r}]`
    if (!Array.isArray(row)) {
      throw new ValidationError(rowPath, `row must be an array, got ${jsonType(row)}`)
    }
    if (row.length === 0) {
      throw new ValidationError(rowPath, "empty row")
    }
    row.forEach((entry, c) => validateEntry(entry, `${rowPath}[${c}]`))
  }

  return true
}

function validateEntry(entry, path) {
  if (typeof entry === "string") return

  if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
    throw new ValidationError(path, `entry must be a string or object, got ${jsonType(entry)}`)
  }

  for (const field of Object.keys(entry)) {
    if (!ALLOWED_FIELDS.has(field)) {
      throw new ValidationError(path, `unknown field '${field}' (allowed: key, macro, display, popup)`)
    }
  }

  const key = getStringField(entry, path, KEY)
  const macro = getStringField(entry, path, MACRO)

  if (key !== undefined && macro !== undefined) {
    throw new ValidationError(path, `both 'key' and 'macro' set; Termux rejects this (ExtraKeyButton)`)
  }
  if (key === undefined && macro === undefined) {
    throw new ValidationError(path, `must specify either 'key' or 'macro' (ExtraKeyButton)`)
  }

  getStringField(entry, path, DISPLAY)

  const popup = entry.popup
  if (popup !== undefined) {
    if (typeof popup === "string") return
    if (typeof popup !== "object" || popup === null || Array.isArray(popup)) {
      throw new ValidationError(path, `'popup' must be a string or object, got ${jsonType(popup)}`)
    }
    validatePopupObject(popup, `${path}.popup`)
  }
}

function getStringField(obj, path, field) {
  const value = obj[field]
  if (value === undefined) return undefined
  if (typeof value !== "string") {
    throw new ValidationError(path, `'${field}' must be a string, got ${jsonType(value)}`)
  }
  return value
}

function validatePopupObject(obj, path) {
  for (const field of Object.keys(obj)) {
    if (!ALLOWED_FIELDS.has(field)) {
      throw new ValidationError(path, `unknown field '${field}'`)
    }
  }

  const key = getStringField(obj, path, KEY)
  const macro = getStringField(obj, path, MACRO)

  if (key !== undefined && macro !== undefined) {
    throw new ValidationError(path, `both 'key' and 'macro' set (invalid popup)`)
  }
  if (key === undefined && macro === undefined) {
    throw new ValidationError(path, `popup must specify either 'key' or 'macro'`)
  }

  if (obj.popup !== undefined) {
    throw new ValidationError(path, `'popup' cannot be nested inside another 'popup' (unsupported by Termux)`)
  }

  getStringField(obj, path, DISPLAY)
}