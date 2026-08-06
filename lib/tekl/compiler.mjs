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