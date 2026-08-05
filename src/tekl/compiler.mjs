import { SPECIAL_KEYS } from "./tokenizer.mjs"

export class CompileError extends Error {
  constructor(message) {
    super(`tekl: ${message}`)
  }
}

const MODIFIER_KEYS = ["CTRL", "ALT", "SHIFT", "FN"]

export function compile(rows) {
  return rows.map((row) => row.map(compileKey))
}

function compileKey(node) {
  switch (node.type) {
    case "key":
      return node.key

    case "popup":
      return { key: node.key, popup: node.popup }

    case "combo":
      return compileCombo(node)

    case "label":
      return { key: node.key, display: node.label }

    case "macro":
      for (const key of node.keys) {
        if (!MODIFIER_KEYS.includes(key) && !/^[A-Za-z][A-Za-z0-9_]*$/.test(key)) {
          throw new CompileError(`invalid key '${key}' in macro`)
        }
      }
      return { macro: node.keys.join(" "), display: node.label }

    default:
      throw new CompileError(`unknown node type '${node.type}'`)
  }
}

function compileCombo(node) {
  const { modifier, target } = node

  if (!MODIFIER_KEYS.includes(modifier)) {
    throw new CompileError(
      `'${modifier}(${target})': Termux macros only support CTRL/ALT/SHIFT/FN as modifier keys, got '${modifier}'`,
    )
  }

  return { macro: `${modifier} ${target}`, display: `${modifier} ${target}` }
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
