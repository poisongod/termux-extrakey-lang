import { execFile } from "node:child_process"
import { watch } from "node:fs"
import { readFile, writeFile, rename, mkdir } from "node:fs/promises"
import { fileURLToPath, pathToFileURL } from "node:url"
import { isMainThread } from "node:worker_threads"
// keys 单文件插件 — tekl DSL 工具链 + TUI io + worker 装配 (`termux-extrakey tekl` 子命令 + 布局读写)。
// 原 lib/tekl/{tokenizer,dsl-stringify,printer,parser,compiler,compiler-io,jit,cli}.mjs
// + lib/tui/io.mjs + lib/plugins/{tekl,tekl-worker}.mjs 合并为一个 worker 自持插件。
// worker = 本文件 (new Worker(keysPlugin.worker) 进入 !isMainThread 分支做 serve)。



import { definePlugin } from "./kernel/kernel.mjs"
import { serve } from "./kernel/worker.mjs"
import { parseJsonDsl } from "./json-dsl.mjs"
import { validateExtraKeyJson } from "./validate.mjs"
import { NAMED_KEYS, SPECIAL_KEYS } from "./keyset.mjs"
import { defaultLayoutFile } from "./user-dir.mjs"


// ══════════════════════════════════════════════════════════════════
// tokenizer (原 lib/tekl/tokenizer.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
export class TokenizerError extends Error {
  constructor(message, line, column) {
    super(`keys: ${message} (line ${line}, column ${column})`)
    this.line = line
    this.column = column
  }
}

const IDENT_RE = /[A-Za-z0-9_]/

export function tokenize(src) {
  const tokens = []
  let i = 0
  let line = 1
  let column = 1

  const error = (msg) => {
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

    if (c === "'" || c === '"') {
      const quote = c
      let j = i + 1
      while (j < src.length && src[j] !== quote && src[j] !== "\n") j++
      if (j >= src.length || src[j] !== quote) error("unterminated string literal")
      tokens.push({ type: "LITERAL", value: src.slice(i + 1, j), line, column })
      column += j - i + 1
      i = j + 1
      continue
    }

    if (IDENT_RE.test(c)) {
      let j = i
      while (j < src.length && IDENT_RE.test(src[j])) j++
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

// ══════════════════════════════════════════════════════════════════
// stringify (原 lib/tekl/dsl-stringify.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
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

// ══════════════════════════════════════════════════════════════════
// printer (原 lib/tekl/printer.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// AST → tekl source (reverse of parser.mjs). TUI 持久化与回写使用。
// 按 tekl.ebnf 每条产生式逐节点打印;语法无歧义 ⇒ 打印确定,round-trip 可测。
//
//   key(named/special)  → key       (保留原大小写)
//   key(literal)        → "text"    (优先双引号;含 `"` 时用单引号)
//   combo               → node(inner)
//   label               → node:label
//   popup               → main.popupNode
//   row                 → 空格连接条目 + ";"
//   rows                → "\n" 连接行

export function printLiteral(s) {
  const q = s.includes('"') ? "'" : '"'
  return `${q}${s}${q}`
}

function printAtom(node) {
  switch (node.type) {
    case "key":
      if (node.kind === "literal") return printLiteral(node.key)
      return node.key
    case "combo":
      return `${printAtom(node.node)}(${printAtom(node.inner)})`
    case "label":
      return `${printAtom(node.node)}:${node.label}`
    case "popup":
      return `${printAtom(node.main)}.${printAtom(node.popupNode)}`
    default:
      throw new Error(`printer: unknown node type '${node.type}'`)
  }
}

export function printEntry(entry) {
  return printAtom(entry)
}

export function printRow(row) {
  return `${row.map(printEntry).join(" ")};`
}

export function printRows(rows) {
  return rows.map(printRow).join("\n")
}

// ══════════════════════════════════════════════════════════════════
// parser (原 lib/tekl/parser.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════

export class ParseError extends Error {
  constructor(message, line, column) {
    super(`keys: ${message} (line ${line}, column ${column})`)
    this.line = line
    this.column = column
  }
}

export function formatParseError(source, err) {
  if (!(err instanceof ParseError)) return err.message
  const lines = source.split("\n")
  const text = lines[err.line - 1] ?? ""
  const col = Math.max(0, err.column - 1)
  const msg = err.message.replace(/^keys: /, "")
  return [
    `keys: ${msg}`,
    `  location: line ${err.line}, column ${err.column}`,
    `  ${String(err.line).padStart(3)} | ${text}`,
    `      | ${" ".repeat(col)}^`,
  ].join("\n")
}

function isSpecial(id) {
  return SPECIAL_KEYS.has(id.toUpperCase())
}

// A ::= N | S | A(A) | A:N
export function parse(src) {
  const tokens = tokenize(src)
  const rows = []
  let row = []
  let i = 0

  const peek = () => tokens[i]
  const next = () => tokens[i++]

  function expect(type) {
    const tok = next()
    if (tok.type !== type) {
      throw new ParseError(`expected ${type}, got ${tok.type}`, tok.line, tok.column)
    }
    return tok
  }

  function parseBaseKey() {
    const tok = next()
    if (tok.type === "IDENT") {
      return {
        type: "key",
        key: tok.value,
        kind: isSpecial(tok.value) ? "special" : "named",
      }
    }
    if (tok.type === "LITERAL") {
      return { type: "key", key: tok.value, kind: "literal" }
    }
    throw new ParseError(`expected key, got ${tok.type}`, tok.line, tok.column)
  }

  function parseAtom() {
    let node = parseBaseKey()

    while (true) {
      const t = peek()

      if (t.type === "LPAREN") {
        next()
        const inner = parseAtom()
        expect("RPAREN")
        node = { type: "combo", node, inner }
        continue
      }

      if (t.type === "COLON") {
        next()
        const labelTok = peek()
        if (labelTok.type === "LITERAL") {
          throw new ParseError("label must not be quoted", labelTok.line, labelTok.column)
        }
        if (labelTok.type === "IDENT") {
          next()
          node = { type: "label", node, label: labelTok.value }
        } else {
          node = { type: "label", node, label: "" }
        }
        continue
      }

      break
    }

    return node
  }

  // M ::= A | A.A | M M | M'\n' | M';\n'
  function parseEntry() {
    const main = parseAtom()
    if (peek().type === "DOT") {
      next()
      const popupNode = parseAtom()
      if (popupNode.type === "popup") {
        throw new ParseError("nested popup is not supported", popupNode.line, popupNode.column)
      }
      return { type: "popup", main, popupNode }
    }
    return main
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

    if (tok.type === "IDENT" || tok.type === "LITERAL") {
      row.push(parseEntry())
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
    for (const entry of rows[r]) {
      const isKey = entry.type === "key" && entry.kind === "special"
      const isLabeledSpecial =
        entry.type === "label" && entry.node?.type === "key" && entry.node.kind === "special"
      const t = isKey ? entry : isLabeledSpecial ? entry.node : null
      if (t) {
        const upper = t.key.toUpperCase()
        if (specials.has(upper)) {
          throw new ParseError(`special key '${t.key}' appears more than once in row ${r + 1}`, 0, 0)
        }
        specials.set(upper, true)
      }
    }
  }
}
// ══════════════════════════════════════════════════════════════════
// compiler (原 lib/tekl/compiler.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════

export class CompileError extends Error {
  constructor(message) {
    super(`keys: ${message}`)
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
// ══════════════════════════════════════════════════════════════════
// compiler-io (原 lib/tekl/compiler-io.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════




const HOME = process.env.HOME
export const DEFAULT_PROPERTIES = `${HOME}/.termux/termux.properties`

export class IoError extends Error {
  constructor(message, cause) {
    super(`tekl: ${message}`)
    this.cause = cause
  }
}

export function compileSource(src) {
  const rows = parse(src)
  const json = compile(rows)
  validateTermuxJson(json)
  validateExtraKeyJson(json)
  return { rows, json, warnings: collectWarnings(json) }
}

export async function updateProperties(json, propsPath = DEFAULT_PROPERTIES) {
  const serialized = stringifyDsl(json)

  // round-trip: Android will re-parse this exact text with JSONTokener
  const reparsed = parseJsonDsl(serialized)
  if (JSON.stringify(reparsed) !== JSON.stringify(json)) {
    throw new IoError("generated extra-keys does not round-trip through Android JSONTokener")
  }

  let content = ""

  try {
    content = await readFile(propsPath, "utf8")
  } catch {
    content = ""
  }

  const lines = content.split("\n")
  let replaced = false

  const out = lines.map((line) => {
    if (/^\s*extra-keys\s*=/.test(line)) {
      replaced = true
      return `extra-keys = ${serialized}`
    }
    return line
  })

  if (!replaced) {
    if (out.length > 0 && out[out.length - 1] !== "") out.push("")
    out.push(`extra-keys = ${serialized}`)
  }

  await writePropertiesAtomic(propsPath, out.join("\n"))

  const written = await readFile(propsPath, "utf8")
  if (!written.includes(`extra-keys = ${serialized}`)) {
    throw new IoError(`verification failed: 'extra-keys' not found in ${propsPath}`)
  }

  return propsPath
}

async function writePropertiesAtomic(propsPath, content) {
  const dir = propsPath.includes("/") ? propsPath.slice(0, propsPath.lastIndexOf("/")) : "."
  await mkdir(dir, { recursive: true })

  const backup = `${propsPath}.tekl.bak`
  try {
    await rename(propsPath, backup)
  } catch {
    // no existing file to back up
  }

  try {
    await writeFile(propsPath, content)
  } catch (err) {
    try {
      await rename(backup, propsPath)
    } catch {
      // restore failed, report original error
    }
    throw new IoError(`failed to write ${propsPath}`, err)
  }
}

export function reloadSettings() {
  return new Promise((resolve, reject) => {
    execFile("termux-reload-settings", (error, stdout, stderr) => {
      if (error) {
        reject(new IoError(`termux-reload-settings failed: ${stderr || error.message}`))
        return
      }
      resolve(stdout)
    })
  })
}

export async function compileFile(filePath, options = {}) {
  const { propsPath, apply = false, reload = false } = options
  let src
  try {
    src = await readFile(filePath, "utf8")
  } catch (err) {
    throw new IoError(`cannot read '${filePath}'`, err)
  }

  const { json, warnings } = compileSource(src)

  if (apply) {
    await updateProperties(json, propsPath)
    if (reload) await reloadSettings()
  }

  return { json, propsPath, src, warnings }
}

// ══════════════════════════════════════════════════════════════════
// jit (原 lib/tekl/jit.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════


export async function jit(teklPath, options = {}) {
  const { propsPath, apply = true, reload = false, onError, onUpdate } = options
  let pending = false
  let lastError = null

  async function run() {
    try {
      const { json, warnings } = await compileFile(teklPath, { propsPath, apply })
      if (reload) {
        try {
          await reloadSettings()
        } catch (err) {
          if (onError) onError(err)
        }
      }
      lastError = null
      if (onUpdate) onUpdate(json, warnings)
    } catch (err) {
      lastError = err
      if (onError) onError(err)
    }
  }

  try {
    await run()
  } catch {
    // errors reported via onError
  }

  watch(teklPath, { persistent: true }, () => {
    if (pending) return
    pending = true
    setTimeout(() => {
      pending = false
      run()
    }, 100)
  })

  return {
    getLastError: () => lastError,
  }
}

// ══════════════════════════════════════════════════════════════════
// io (TUI 布局读写) (原 lib/tui/io.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// TUI 的 IO 层 (设计 §2/§5 io.mjs) — 加载/保存 tekl 文件 + 写 properties + reload。
// 复用 lib/tekl/compiler-io.mjs,把编译/校验/写盘逻辑留在 tekl 工具链,TUI 不复制。



export const DEFAULT_LAYOUT_FILE = defaultLayoutFile()

export async function loadDoc(file = DEFAULT_LAYOUT_FILE) {
  file ??= DEFAULT_LAYOUT_FILE
  let src
  try {
    src = await readFile(file, "utf8")
  } catch {
    return { rows: [], src: "", file }
  }
  const rows = parse(src)
  return { rows, src, file }
}

// 保存: 打印 AST → 写入 tekl 文件 → 写 termux.properties → reload
export async function saveDoc(rows, options = {}) {
  const file = options.file ?? DEFAULT_LAYOUT_FILE
  const propsPath = options.propsPath ?? DEFAULT_PROPERTIES
  const reload = options.reload ?? true
  const src = printRows(rows)
  await writeFileIfChanged(file, src + "\n")
  const json = await compileForWrite(src, propsPath)
  if (reload) {
    try {
      await reloadSettings()
    } catch (err) {
      // reload 失败不阻断保存
      console.error(`termux-extrakey: ${err.message}`)
    }
  }
  return { src, json }
}

async function writeFileIfChanged(file, content) {
  let existing = ""
  try {
    existing = await readFile(file, "utf8")
  } catch {
    // 文件不存在时不比较
  }
  if (existing === content) return
  const { mkdir, writeFile } = await import("node:fs/promises")
  await mkdir(file.slice(0, file.lastIndexOf("/")), { recursive: true })
  await writeFile(file, content, "utf8")
}

// 复用 compiler-io 的 compileSource 校验语义: 写盘前须 round-trip + 双校验
async function compileForWrite(src, propsPath) {
  const { json, warnings } = compileSource(src)
  await updateProperties(json, propsPath)
  return json
}
// ══════════════════════════════════════════════════════════════════
// tekl CLI (原 lib/tekl/cli.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════



const USAGE = `usage: termux-extrakey tekl <file.tekl> [--output <termux.properties>] [--reload] [--watch]
  termux-extrakey tekl <file.tekl>       compile tekl and write extra-keys into ~/.termux/termux.properties
  --output <path>                        write to a custom termux.properties (default: ~/.termux/termux.properties)
  --reload                               run termux-reload-settings after writing
  --watch                                JIT mode: watch the file, recompile + write on change`

function parseArgs(argv) {
  const opts = { watch: false, reload: false }
  const rest = []
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (arg === "--watch") {
      opts.watch = true
    } else if (arg === "--reload") {
      opts.reload = true
    } else if (arg === "--output") {
      opts.output = argv[++i]
      if (!opts.output) {
        console.error(USAGE)
        process.exit(1)
      }
    } else if (arg.startsWith("-")) {
      console.error(`unknown option: ${arg}\n${USAGE}`)
      process.exit(1)
    } else {
      rest.push(arg)
    }
  }
  opts.file = rest[0]
  if (rest.length > 1) {
    console.error(`unexpected arguments: ${rest.slice(1).join(" ")}\n${USAGE}`)
    process.exit(1)
  }
  return opts
}

function printError(err, src) {
  if (err instanceof ParseError && src !== undefined) {
    console.error(formatParseError(src, err))
    return
  }
  if (err instanceof CompileError || err?.name === "CompileError") {
    console.error(`tekl: ${err.message.replace(/^tekl: /, "")}`)
    return
  }
  console.error(err.message)
  if (err.cause?.message) console.error(`  cause: ${err.cause.message}`)
}

function printWarnings(warnings) {
  for (const w of warnings) console.error(`\x1b[33mwarning:\x1b[0m ${w}`)
}

async function loadSource(file) {
  return readFile(file, "utf8")
}

export async function run(argv) {
  const opts = parseArgs(argv)
  const propsPath = opts.output

  if (!opts.file) {
    try {
      await readFile(DEFAULT_LAYOUT_FILE, "utf8")
      opts.file = DEFAULT_LAYOUT_FILE
    } catch {
      console.error(`tekl: no input file and ${DEFAULT_LAYOUT_FILE} does not exist`)
      console.error(USAGE)
      process.exit(1)
    }
  }

  if (opts.watch) {
    console.log(`[termux-extrakey] watching ${opts.file} -> ${propsPath ?? "~/.termux/termux.properties"}`)
    console.log("[termux-extrakey] exits automatically once compilation succeeds")
    await jit(opts.file, {
      propsPath,
      apply: true,
      reload: opts.reload,
      onUpdate: (json, warnings) => {
        printWarnings(warnings)
        console.log(`[termux-extrakey] ${new Date().toLocaleTimeString()} updated ${json.length} row(s)`)
        console.log("[termux-extrakey] done, exiting")
        process.exit(0)
      },
      onError: async (err) => {
        let src
        try {
          src = await loadSource(opts.file)
        } catch {
          src = undefined
        }
        printError(err, src)
      },
    })
    return
  }

  try {
    const { src, warnings } = await compileFile(opts.file, {
      propsPath,
      apply: true,
      reload: opts.reload,
    })
    printWarnings(warnings)
    const where = propsPath ?? "~/.termux/termux.properties"
    console.log(`[termux-extrakey] written to ${where}${opts.reload ? ", settings reloaded" : ""}`)
  } catch (err) {
    let src
    try {
      src = await loadSource(opts.file)
    } catch {
      src = undefined
    }
    printError(err, src)
    process.exit(1)
  }
}

// ── worker 方法面 (原 tekl-worker.mjs handlers) + 插件声明 ─────────────
const keysHandlers = {
  compileSource,
  compileFile,
  updateProperties,
  reloadSettings,
  loadDoc,
  saveDoc,
  getDefaultProperties() {
    return DEFAULT_PROPERTIES
  },
}

export const keysPlugin = definePlugin({
  name: "keys",
  version: "1.0.0",
  deps: [],
  worker: fileURLToPath(import.meta.url),
})

if (!isMainThread) serve(keysHandlers)
