import { SPECIAL_KEYS, NAMED_KEYS } from "./tokenizer.mjs"
import { parse } from "./parser.mjs"
import { stringifyDsl } from "./dsl-stringify.mjs"

export class CompileError extends Error {
  constructor(message) {
    super(`tekl: ${message}`)
  }
}

// 命名键/修饰键 (esc/CTRL/f1) 规范化为大写;引号字面键与普通标识符 (如被修饰的 'd') 保留原样
function keyName(node) {
  if (node.kind === "literal") return node.key
  const upper = node.key.toUpperCase()
  if (SPECIAL_KEYS.has(upper) || NAMED_KEYS.has(upper)) return upper
  return node.key
}

// A(A) 组合键:递归收集叶子键 -> "MAIN ALT X"(大写规范化)
function collectComboKeys(node, out = []) {
  if (node.type === "combo") {
    collectComboKeys(node.node, out)
    collectComboKeys(node.inner, out)
    return out
  }
  if (node.type === "key") {
    out.push(keyName(node))
    return out
  }
  throw new CompileError(`combo contains '${node.type}' (only keys allowed inside '()')`)
}

// A ::= N | S | A(A) | A:N
//   N              -> "ESC"           (字符串键)
//   A(A)           -> {macro: ...}     (默认无 display)
//   A(A):Label     -> {macro: ..., display: Label}
//   N:Label        -> {key: ..., display: Label}
function compileAtom(node) {
  switch (node.type) {
    case "key":
      return keyName(node)

    case "label": {
      const base = compileAtom(node.node)
      if (typeof base === "string") return { key: base, display: node.label }
      if (base && typeof base === "object") {
        const keys = Object.keys(base)
        if (keys.includes("key") || keys.includes("macro")) {
          return { ...base, display: node.label }
        }
      }
      throw new CompileError(`cannot label '${JSON.stringify(base)}'`)
    }

    case "combo": {
      const keys = collectComboKeys(node)
      if (keys.length < 2) {
        throw new CompileError(`combo '${keys.join(" ")}' must contain the modifier and at least one key`)
      }
      return { macro: keys.join(" ") }
    }

    case "popup":
      throw new CompileError(`'${node.popup}': popup is not valid inside '()'`)

    default:
      throw new CompileError(`unknown node type '${node.type}'`)
  }
}

// A.A -> {key: main, popup} / {macro: ..., popup}
function compileEntry(node) {
  if (node.type !== "popup") return compileAtom(node)

  const main = compileAtom(node.main)
  const popup = compileAtom(node.popupNode)

  if (typeof main === "string") {
    return { key: main, popup }
  }
  if (main && typeof main === "object" && main.popup === undefined) {
    return { ...main, popup }
  }
  throw new CompileError(`'${JSON.stringify(main)}': popup cannot be nested or followed by a key`)
}

export function compile(rows) {
  return rows.map((row) => row.map(compileEntry))
}

// 编译期警告 (不阻断编译):
//   1. display/macro/popup/key 值为空字符串
//   2. 各键盘行键位数不一致
//   3. 键盘方案缺少应用抽屉键 (DRAWER)
//   4. 键盘方案缺少键盘切换键 (KEYBOARD)
export function collectWarnings(json) {
  const warnings = []

  function emptyFields(obj) {
    return Object.entries(obj)
      .filter(([k, v]) => typeof v === "string" && v === "")
      .map(([k]) => k)
  }

  for (let r = 0; r < json.length; r++) {
    for (let c = 0; c < json[r].length; c++) {
      const entry = json[r][c]
      if (typeof entry !== "object" || entry === null) continue
      for (const field of emptyFields(entry)) {
        warnings.push(`第 ${r + 1} 键盘行第 ${c + 1} 键: '${field}' 的值为空`)
      }
      if (typeof entry.popup === "object" && entry.popup !== null) {
        for (const field of emptyFields(entry.popup)) {
          warnings.push(`第 ${r + 1} 键盘行第 ${c + 1} 键的 popup: '${field}' 的值为空`)
        }
      }
    }
  }

  const lengths = json.map((row) => row.length)
  if (new Set(lengths).size > 1) {
    warnings.push(
      `各键盘行键位数不同: ${lengths.map((n, i) => `第${i + 1}行 ${n} 键`).join(", ")}`,
    )
  }

  function hasKey(name) {
    for (const row of json) {
      for (const entry of row) {
        if (typeof entry === "string") {
          if (entry === name) return true
          continue
        }
        if (typeof entry !== "object" || entry === null) continue
        if (entry.key === name) return true
        if (entry.popup === name) return true
        if (typeof entry.popup === "object" && entry.popup !== null && entry.popup.key === name) {
          return true
        }
      }
    }
    return false
  }

  if (!hasKey("DRAWER")) {
    warnings.push("键盘方案中未找到应用抽屉键 (DRAWER)")
  }
  if (!hasKey("KEYBOARD")) {
    warnings.push("键盘方案中未找到键盘切换键 (KEYBOARD)")
  }

  return warnings
}

export function toExtraKeysJson(src) {
  return stringifyDsl(compile(parse(src)))
}

export function validateTermuxJson(json) {
  try {
    JSON.parse(JSON.stringify(json))
  } catch (err) {
    throw new CompileError(`generated config is not valid JSON: ${err.message}`)
  }

  for (const row of json) {
    for (const key of row) {
      if (typeof key === "string") continue
      if (typeof key !== "object" || key === null) {
        throw new CompileError(`invalid extra key entry: ${JSON.stringify(key)}`)
      }
      if (key.key !== undefined && key.macro !== undefined) {
        throw new CompileError(
          `entry ${JSON.stringify(key)}: Termux rejects keys with both 'key' and 'macro'`,
        )
      }
      if (key.key === undefined && key.macro === undefined && key.display === undefined) {
        throw new CompileError(
          `entry ${JSON.stringify(key)}: Termux requires 'key' or 'macro'`,
        )
      }
    }
  }
}