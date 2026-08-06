import { SPECIAL_KEYS } from "./tokenizer.mjs"

export class CompileError extends Error {
  constructor(message) {
    super(`tekl: ${message}`)
  }
}

// A(A) 组合键:递归收集叶子键 -> "CTRL ALT X"
function collectComboKeys(node, out = []) {
  if (node.type === "combo") {
    collectComboKeys(node.node, out)
    collectComboKeys(node.inner, out)
    return out
  }
  if (node.type === "key") {
    out.push(node.key)
    return out
  }
  throw new CompileError(`combo contains '${node.type}' (only keys allowed inside '()')`)
}

// 单个 A 的编译结果:
//   N                    -> "ESC"         (字符串键)
//   N:Label              -> {key, popup}?  -> {key, display}
//   A(B)                 -> {macro, display}
//   A(B):Label           -> {macro, display: Label}
function compileAtom(node) {
  switch (node.type) {
    case "key":
      return node.key

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
      const macro = keys.join(" ")
      return { macro, display: macro }
    }

    case "popup":
      throw new CompileError(`'${node.popup}': popup is not valid inside '()'`)

    default:
      throw new CompileError(`unknown node type '${node.type}'`)
  }
}

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

// 收集编译期警告(不阻断编译):
//   1. display/macro/popup/key 值为空字符串
//   2. 各键盘行键位数不一致
//   3. 键盘方案缺少应用抽屉键 (DRAWER)
//   4. 键盘方案缺少键盘切换键 (KEYBOARD)
export function collectWarnings(json) {
  const warnings = []

  // 1. 空值
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

  // 2. 键盘行键位数不一致
  const lengths = json.map((row) => row.length)
  if (new Set(lengths).size > 1) {
    warnings.push(
      `各键盘行键位数不同: ${lengths.map((n, i) => `第${i + 1}行 ${n} 键`).join(", ")}`,
    )
  }

  // 3/4. 顶层缺失系统键
  function hasTopLevelKey(name) {
    return json.some((row) =>
      row.some(
        (e) => e === name || (typeof e === "object" && e !== null && e.key === name),
      ),
    )
  }
  if (!hasTopLevelKey("DRAWER")) {
    warnings.push("键盘方案中未找到应用抽屉键 (DRAWER)")
  }
  if (!hasTopLevelKey("KEYBOARD")) {
    warnings.push("键盘方案中未找到键盘切换键 (KEYBOARD)")
  }

  return warnings
}

export function toExtraKeysJson(rows) {
  return JSON.stringify(compile(rows))
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