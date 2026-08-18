// ui 单文件插件 — TUI 行为层 (进程内)。
// 原 lib/tui/{plugin,plugin-loader,event-token,doc,matrix,actions,prims,mode,store}.mjs
// + lib/plugins/tui.mjs 合并为一个进程内插件;行为核心语言在 ./behavior.mjs,
// 键词汇表在 ./keyset.mjs,布局读写 (io) 在 ./keys.mjs (跨线程)。

import { definePlugin as kernelDefinePlugin } from "./kernel/kernel.mjs"
import {
  parseProgram,
  buildDispatchTable,
  checkBehaviorProgram,
  BUILTIN_OPS,
  parseRules,
} from "./behavior.mjs"
import { behaviorBaseContent } from "./user-dir.mjs"
import { NAMED_KEYS, SPECIAL_KEYS } from "./keyset.mjs"


// ══════════════════════════════════════════════════════════════════
// plugin kernel (原 lib/tui/plugin.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 插件内核 (通用) — 单元、生命周期、依赖排序、审计。
// 不感知业务: 不知道 actions/guards/ttoll 是什么。它只负责"托管插件集合"。
// 业务贡献点 (actions/guards/rules/hooks) 是 mode.mjs 与插件之间的集成契约。
// 纯数据层: 不 import opentui, 可 node --test。

export class PluginError extends Error {
  constructor(message) {
    super(`plugin: ${message}`)
  }
}

// 插件名 (包名空间): 字母/数字/下划线/连字符/点, 可含中文; 非空
export const NAME_RE = /^[\p{L}\p{N}_.\-]+$/u

// ==== 插件定义 (纯声明) ====
//   { name, version?, deps?: string[],
//     contribute?: { [能力]: 值 } }
// 一个插件 = 一份声明; 宿主负责装配/生命周期/回收。

export function definePlugin(def) {
  if (!def || typeof def !== "object") throw new PluginError("plugin 必须是对象")
  const { name, version = "0.0.0", deps = [], contribute = {} } = def
  if (!NAME_RE.test(name ?? "")) throw new PluginError(`非法插件名 '${name}'`)
  if (!Array.isArray(deps)) throw new PluginError(`插件 '${name}': deps 必须是数组`)
  if (!contribute || typeof contribute !== "object") {
    throw new PluginError(`插件 '${name}': contribute 必须是对象`)
  }
  return { name, version, deps, contribute }
}

// ==== 内核 ====

export function createPluginKernel() {
  const records = new Map() // name → { def, order }
  let counter = 0

  function assertRegistered(name) {
    if (!records.has(name)) throw new PluginError(`依赖缺失: '${name}' 未注册`)
  }

  // 拓扑排序 (Kahn): 保证 activate 顺序 = 依赖在前。返回依赖序的 name 数组。
  function topoOrder() {
    const deps = new Map()
    for (const [name, { def }] of records) deps.set(name, new Set(def.deps))
    const ready = [...deps].filter(([, d]) => d.size === 0).map(([n]) => n)
    const reversed = []
    while (ready.length) {
      const n = ready.shift()
      reversed.push(n)
      for (const [m, d] of deps) {
        if (d.delete(n) && d.size === 0) ready.push(m)
      }
    }
    if (reversed.length !== records.size) {
      const cycle = [...deps].filter(([, d]) => d.size > 0).map(([n]) => n)
      throw new PluginError(`插件依赖成环: ${cycle.join(", ")}`)
    }
    return reversed
  }

  // 注册插件: 拓扑排序 + 可达性校验。返回注销器 (逆序回收)。
  function register(def) {
    const p = definePlugin(def)
    if (records.has(p.name)) throw new PluginError(`插件重名: '${p.name}'`)
    for (const d of p.deps) assertRegistered(d)
    records.set(p.name, { def: p, order: ++counter })
    const order = topoOrder() // 触发环检测
    void order
    let dead = false
    return function unregister() {
      if (dead) return
      dead = true
      records.delete(p.name)
      topoOrder() // 删除后重新校验 (被依赖方不可删)
    }
  }

  // 按依赖序遍历 (activate 顺序)。返回 [{ name, def, order }]。
  function order() {
    return topoOrder().map((name) => ({ name, ...records.get(name) }))
  }

  // 聚合某贡献点: [{ plugin: name, value }] (依赖序)
  function contribute(capability) {
    const out = []
    for (const { name, def } of order()) {
      const value = def.contribute?.[capability]
      if (value !== undefined) out.push({ plugin: name, value })
    }
    return out
  }

  function has(name) {
    return records.has(name)
  }

  function names() {
    return new Set(records.keys())
  }

  function describe() {
    return order().map(({ name, def }) => ({
      name,
      version: def.version,
      deps: def.deps,
      capabilities: Object.keys(def.contribute),
    }))
  }

  return { register, order, contribute, has, names, describe }
}

// ==== 便捷: 合并同一贡献点的值 (同能力多个插件, 按依赖序组合) ====
// 若值为数组 → 拼接; 若为对象 → 浅合并 (后者覆盖前者); 否则取最后一个。
export function mergeContributions(kernel, capability) {
  const entries = kernel.contribute(capability)
  let out
  for (const { value } of entries) {
    if (Array.isArray(value)) {
      out = [...(out ?? []), ...value]
    } else if (value && typeof value === "object" && !Array.isArray(value)) {
      out = { ...(out ?? {}), ...value }
    } else {
      out = value
    }
  }
  return out
}
// ══════════════════════════════════════════════════════════════════
// plugin loader (原 lib/tui/plugin-loader.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 插件加载器 — 从目录发现并装配插件 (三方包接入点)。
// 约定: 扫描 <dir> 下所有 *.plugin.mjs, 每个模块 export default 一份插件声明
// (definePlugin 产物), 按文件名序注册进内核。加载顺序可被 deps 覆盖。
// 支持入口文件 <dir>/init.mjs (如 ~/.termux/termux-extrakey/id/init.mjs):
//   export default 单份声明 或 声明数组 (可另附元数据)。
// 纯 Node 层: 不 import opentui, 可 node --test。

import { readdirSync, existsSync } from "node:fs"
import { join, extname } from "node:path"
import { pathToFileURL } from "node:url"

const SUFFIX = ".plugin.mjs"
const DEFAULT_ENTRY = "init.mjs"

export function discoverPlugins(dir) {
  let entries
  try {
    entries = readdirSync(dir, { withFileTypes: true })
  } catch {
    return []
  }
  return entries
    .filter((e) => e.isFile() && e.name.endsWith(SUFFIX))
    .map((e) => join(dir, e.name))
    .sort()
}

// 装配: 对每个发现的文件 import + 校验 + 返回 { file, def }
export async function loadPluginModule(file) {
  const mod = await import(pathToFileURL(file).href)
  const def = mod.default
  if (def === undefined) {
    throw new PluginError(`加载插件失败: ${file} 缺少 export default (definePlugin 产物)`)
  }
  return { file, def: definePlugin(def) }
}

// 入口文件: export default = 单份声明 | 声明数组 | { plugins: [...], ...meta }
export async function loadEntryModule(file) {
  const mod = await import(pathToFileURL(file).href)
  const out = mod.default
  const list = Array.isArray(out) ? out : out?.plugins
  if (list) {
    if (!Array.isArray(list) || list.some((d) => d === undefined)) {
      throw new PluginError(`加载插件入口失败: ${file} plugins 必须是插件声明数组`)
    }
    return list.map((d) => definePlugin(d))
  }
  if (out === undefined) {
    throw new PluginError(`加载插件入口失败: ${file} 缺少 export default`)
  }
  return [definePlugin(out)]
}

// 全部装配: 先入口文件 (init.mjs, 若有), 再 *.plugin.mjs。返回 [{ file, def, unregister }]
export async function loadPlugins(dir, { register = defaultRegister, entry = DEFAULT_ENTRY } = {}) {
  const loaded = []
  const entryFile = entry ? join(dir, entry) : null
  if (entryFile && existsSync(entryFile)) {
    for (const def of await loadEntryModule(entryFile)) {
      const unregister = register(def)
      loaded.push({ file: entryFile, def, unregister })
    }
  }
  for (const file of discoverPlugins(dir)) {
    const { def } = await loadPluginModule(file)
    const unregister = register(def)
    loaded.push({ file, def, unregister })
  }
  return loaded
}

// 兜底: 未提供 register 时用无操作注册 (仅校验语法)
function defaultRegister(def) {
  void def
  return () => {}
}

export { extname }

// ══════════════════════════════════════════════════════════════════
// event token (原 lib/tui/event-token.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// opentui KeyEvent → mode.mjs 的事件令牌 (§6 操作集)。
// name: up/down/left/right/return/escape/space/ctrl/meta/option... → 归一化为 ttoll 键集。

export function eventToken(key) {
  const { name, ctrl, meta, option } = key
  if (ctrl) return "ctrl"
  // Termux 硬件键盘: meta/option 触发 ALT 组合 — 优先级高于裸方向键
  if (meta || option) return "alt"
  if (name === "up") return "up"
  if (name === "down") return "down"
  if (name === "left") return "left"
  if (name === "right") return "right"
  if (name === "return" || name === "enter") return "enter"
  if (name === "escape" || name === "esc") return "esc"
  if (name === "space") return "space"
  return null
}

// 名称输入框面板: 除 ESC 外所有字符键输入到 nameDraft
export function isTextInput(key) {
  const name = eventToken(key)
  if (!name) return key.sequence?.length === 1
  return name === "space" && key.sequence === " "
}
// ══════════════════════════════════════════════════════════════════
// doc transforms (原 lib/tui/doc.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// AST 结构化变换 (纯函数) — TUI 编辑模型。§4.3 / §4.5。
// 输入输出均为 parser.mjs 产出的 rows: Row[], Row = Entry[]。
// 不在 TUI 内复制编译/写入逻辑;仅做语法层变换,语义校验复用 lib/tekl/compiler.mjs。

export const DEFAULT_KEY = Object.freeze({ type: "key", key: "SPACE", kind: "named" })
export const COMBO_MAX_DEPTH = 3

// ── 读取 ──────────────────────────────────────────────────────────────

export function entryMain(entry) {
  return entry.type === "popup" ? entry.main : entry
}

export function entryPopup(entry) {
  return entry.type === "popup" ? entry.popupNode : null
}

// 组合键叶子 (按宏序) → key 原子数组;单键/标签取其一
export function collectLeaves(atom, out = []) {
  if (atom.type === "combo") {
    collectLeaves(atom.node, out)
    collectLeaves(atom.inner, out)
    return out
  }
  out.push(atom)
  return out
}

export function comboDepth(atom, depth = 1) {
  if (atom.type !== "combo") return depth
  return comboDepth(atom.inner, depth + 1)
}

// ── 行/列操作 (§4.3 M0 行列菜单) ──────────────────────────────────────

function cloneRows(rows) {
  return rows.map((row) => [...row])
}

export function insertColumn(rows, r, c, atom = DEFAULT_KEY) {
  const out = cloneRows(rows)
  out[r] = [...out[r]]
  out[r].splice(c, 0, atom)
  return out
}

export function insertRow(rows, r, atom = DEFAULT_KEY) {
  const out = cloneRows(rows)
  out.splice(r, 0, [atom])
  return out
}

export function deleteColumn(rows, r, c) {
  const out = cloneRows(rows)
  out[r] = [...out[r]]
  out[r].splice(c, 1)
  if (out[r].length === 0) out.splice(r, 1)
  return out
}

export function deleteRow(rows, r) {
  const out = cloneRows(rows)
  out.splice(r, 1)
  return out
}

export function swap(rows, r1, c1, r2, c2) {
  if (r1 === r2 && c1 === c2) return rows
  const out = cloneRows(rows)
  const a = out[r1][c1]
  out[r1][c1] = out[r2][c2]
  out[r2][c2] = a
  return out
}

// ── 值/标签变换 (§4.3 M1/M2) ──────────────────────────────────────────

// 组合链: 多值 → 嵌套 combo (宏序 = 选择序)。1 值 = 单原子;0 值 = null。
// A(A) 语义: CTRL(ALT(X)) = combo{node: CTRL, inner: combo{node: ALT, inner: X}}
export function makeCombo(atoms) {
  if (atoms.length === 0) return null
  let node = atoms[atoms.length - 1]
  for (let i = atoms.length - 2; i >= 0; i--) {
    node = { type: "combo", node: atoms[i], inner: node }
  }
  return node
}

// 以新原子替换某 entry 的主键值: label 包裹保留, popup 包裹保留
export function replaceMain(entry, atom) {
  const inner = (e) => (e.type === "label" ? { ...e, node: atom } : atom)
  if (entry.type === "popup") return { ...entry, main: inner(entry.main) }
  return inner(entry)
}

// 以新原子替换弹出键值;无弹出键则创建 A.A
export function replacePopup(entry, atom) {
  if (entry.type === "popup") return { ...entry, popupNode: atom }
  return { type: "popup", main: entry, popupNode: atom }
}

// 移除弹出键 (A.A → A);原本无弹出键则为 no-op
export function removePopup(entry) {
  if (entry.type !== "popup") return entry
  return entry.main
}

// 标签 wrap/unwrap (A → A:N)。空字符串 = 清除标签 (A:N → A)。
function wrapLabel(atom, label) {
  if (label === "") {
    return atom.type === "label" ? atom.node : atom
  }
  if (atom.type === "label") return { ...atom, label }
  return { type: "label", node: atom, label }
}

// 设/清主键标签 (M1 名称框)
export function setMainLabel(entry, label) {
  if (entry.type === "popup") return { ...entry, main: wrapLabel(entry.main, label) }
  return wrapLabel(entry, label)
}

// 设/清弹出键标签 (M2 名称框);无弹出键时为 no-op
export function setPopupLabel(entry, label) {
  if (entry.type !== "popup") return entry
  return { ...entry, popupNode: wrapLabel(entry.popupNode, label) }
}

// ══════════════════════════════════════════════════════════════════
// matrix (原 lib/tui/matrix.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 预设键值矩阵 (§6.5) — 定义与导航。纯数据层,不 import opentui。
// 分区: 修饰键 / 命名键 / 功能键 / 字面量池。
// 每格 = { type:"key", key, kind } (kind ∈ named/special/literal), 即 tekl AST 的 key 原子。


const NAMED_EXCLUDED = new Set(["KEYBOARD", "DRAWER", "SCROLL", "SPACE", "PASTE"])
const LITERALS = ["~", "|", "\\", "%", "$", "#", "@", "^", ">", "<", "{", "}", "(", ")", "[", "]", "+", "=", "?"]

function keyAtom(key, kind = "named") {
  return { type: "key", key, kind }
}

export const MODIFIER_ROW = [...SPECIAL_KEYS].map((k) => keyAtom(k, "special"))

export const NAMED_ROWS = chunk(
  [...NAMED_KEYS].filter((k) => !NAMED_EXCLUDED.has(k) && !SPECIAL_KEYS.has(k)).map((k) => keyAtom(k)),
  8,
)

export const LITERAL_ROW = LITERALS.map((k) => keyAtom(k, "literal"))

function chunk(arr, n) {
  const out = []
  for (let i = 0; i < arr.length; i += n) out.push(arr.slice(i, i + n))
  return out
}

// 展平全部矩阵格 (行序): [row][col] 访问
export const MATRIX = [...MODIFIER_ROW, ...NAMED_ROWS.flat(), ...LITERAL_ROW]

// 行宽 (列数),用于左右环绕
export const MATRIX_COLS = Math.max(MODIFIER_ROW.length, ...NAMED_ROWS.map((r) => r.length), LITERAL_ROW.length)

export function toMatrixIndex(rowIdx, colIdx) {
  const rows = [MODIFIER_ROW, ...NAMED_ROWS, LITERAL_ROW]
  let offset = 0
  for (let r = 0; r < rowIdx; r++) offset += rows[r].length
  return offset + colIdx
}

// 线性 index → {row, col}; 行内越界按行宽夹取
export function matrixCell(index) {
  const rows = [MODIFIER_ROW, ...NAMED_ROWS, LITERAL_ROW]
  for (const [r, row] of rows.entries()) {
    if (index < row.length) return { row: r, col: index, cell: row[index] }
    index -= row.length
  }
  return { row: rows.length - 1, col: rows.at(-1).length - 1, cell: rows.at(-1).at(-1) }
}

// 方向移动 (带环绕): 返回新线性 index
export function moveIndex(index, dir, cols = MATRIX_COLS) {
  const rows = [MODIFIER_ROW, ...NAMED_ROWS, LITERAL_ROW]
  const total = rows.reduce((n, r) => n + r.length, 0)
  const flat = rows.flat()
  const pos = Math.max(0, Math.min(index, flat.length - 1))
  const { row, col } = matrixCell(pos)

  let nr = row
  let nc = col
  if (dir === "up") {
    nr = row === 0 ? rows.length - 1 : row - 1
    nc = Math.min(col, rows[nr].length - 1)
  } else if (dir === "down") {
    nr = row === rows.length - 1 ? 0 : row + 1
    nc = Math.min(col, rows[nr].length - 1)
  } else if (dir === "left") {
    nc = col === 0 ? rows[row].length - 1 : col - 1
  } else if (dir === "right") {
    nc = col === rows[row].length - 1 ? 0 : col + 1
  }
  return toMatrixIndex(nr, nc)
}

// 给定 key 原子 (单键),返回其在线性矩阵中的 index;未找到返回 -1
export function findIndex(atom) {
  return MATRIX.findIndex(
    (c) => c.type === atom.type && c.key === atom.key && c.kind === atom.kind,
  )
}

// 预高亮: 组合键/单键 → 组成叶子在矩阵中的 index 集
export function preHighlight(atom) {
  if (!atom) return []
  if (atom.type === "combo") {
    const out = []
    for (const leaf of flattenCombo(atom)) out.push(findIndex(leaf))
    return out
  }
  const i = findIndex(atom)
  return i >= 0 ? [i] : []
}

function flattenCombo(node, out = []) {
  if (node.type === "combo") {
    flattenCombo(node.node, out)
    flattenCombo(node.inner, out)
    return out
  }
  out.push(node)
  return out
}

// 已选序列 → AST 原子 (1 值单键 / ≥2 组合链);复用 doc.makeCombo

export { makeCombo as selectionToAtom }
// ══════════════════════════════════════════════════════════════════
// actions (原 lib/tui/actions.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 内置守卫与文档助手 (纯支持层) — 不再承载动作实现。
// 动作语义已全部声明化到 tekl.ttoll 行为段, 由 mode.mjs 解释器按程序执行。
// 本文件只保留: EFFECT 常量 + 内置守卫谓词 + 插件可复用的 doc 变换 facade。
// 纯数据层: 不 import opentui, 可 node --test。


// 生效副作用 (§7 生效点): APPLY=校验+写盘+reload, EXIT=退出, CANCEL=回滚预览
export const EFFECT = {
  APPLY: { type: "apply" },
  EXIT: { type: "exit" },
  CANCEL: { type: "cancel" },
}

const entryAt = (s) => s.rows[s.cursor.r]?.[s.cursor.c]
const mainAtom = (e) => (e.type === "popup" ? e.main : e)
const popupAtom = (e) => (e.type === "popup" ? e.popupNode : null)

function sameAtom(a, b) {
  return a.type === b.type && a.key === b.key && a.kind === b.kind
}

function matrixAtomAtHighlight(s) {
  const idx = Math.min(Math.max(s.matrix.highlight, 0), MATRIX.length - 1)
  return MATRIX[idx]
}

function replaceEntry(rows, { r, c }, entry) {
  const out = rows.map((row) => [...row])
  out[r][c] = entry
  return out
}

function clampCursor(s) {
  const row = s.rows[s.cursor.r]
  return { r: s.cursor.r, c: Math.min(s.cursor.c, row ? row.length - 1 : 0) }
}

// ── 内置守卫: name → predicate(state) ────────────────────────────────
export const builtinGuards = Object.freeze({
  sel: (s) => s.selected !== null,
  nosel: (s) => s.selected === null,
  same: (s) => s.selected && s.selected.r === s.cursor.r && s.selected.c === s.cursor.c,
  diff: (s) => s.selected && !(s.selected.r === s.cursor.r && s.selected.c === s.cursor.c),
  insel: (s) => s.matrix.selected.some((a) => sameAtom(a, matrixAtomAtHighlight(s))),
  noinsel: (s) => !s.matrix.selected.some((a) => sameAtom(a, matrixAtomAtHighlight(s))),
  haspopup: (s) => popupAtom(entryAt(s)) !== null,
  nopopup: (s) => popupAtom(entryAt(s)) === null,
})

// 供插件复用的 doc 变换 (公开的只读 facade)
export const docHelpers = Object.freeze({
  entryAt,
  mainAtom,
  popupAtom,
  sameAtom,
  matrixAtomAtHighlight,
  replaceEntry,
  clampCursor,
})

// 内置插件声明 (第一方 plugin): 贡献 guards。
// 行为 (actions) 不再由 JS 贡献 — 全部来自 tekl.ttoll 行为段。
export const builtinPlugin = {
  name: "builtin",
  version: "1.0.0",
  deps: [],
  contribute: {
    guards: builtinGuards,
  },
}

// ══════════════════════════════════════════════════════════════════
// prims (原 lib/tui/prims.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 动作原语集 (交互原子) — ttoll 行为段里被解释器调用的单一操作单位。
// 每个原语 = 纯状态变换 (state → { state, effects }), 无副作用。
// 与 ttoll 的关系: 行为段声明"怎么组合", 原语声明"每种原子动作的语义"。
// 扩展点: 第三方插件可注册原语 (见 mode.mjs registerPrim), 行为段引用其名。
// 纯数据层: 不 import opentui, 可 node --test。


const dirVec = {
  UP: { dr: -1, dc: 0 },
  DOWN: { dr: 1, dc: 0 },
  LEFT: { dr: 0, dc: -1 },
  RIGHT: { dr: 0, dc: 1 },
}

const ok = (state, effects = []) => ({ state, effects })
const setPanel = (s, panel, extra = {}) => ok({ ...s, panel, ...extra })

// ── 网格/矩阵/菜单 焦点移动 (panel 感知) ─────────────────────────────

const move = {
  name: "move",
  desc: "移动焦点: 按当前面板移动网格光标/矩阵高亮/菜单高亮",
  run(state, key) {
    if (state.panel === PANEL.MATRIX) {
      return ok({
        ...state,
        matrix: { ...state.matrix, highlight: moveIndex(state.matrix.highlight, key.toLowerCase()) },
      })
    }
    if (state.panel === PANEL.ROWMENU) {
      const step = key === "DOWN" ? 1 : -1
      return ok({ ...state, menuHighlight: (state.menuHighlight + step + 6) % 6 })
    }
    const { dr, dc } = dirVec[key] ?? { dr: 0, dc: 0 }
    const R = state.rows.length
    if (R === 0) return ok(state)
    let { r, c } = state.cursor
    const C = state.rows[r].length
    if (dr !== 0) r = (r + dr + R) % R
    if (dc !== 0) c = (c + dc + C) % C
    return ok({ ...state, cursor: { r, c } })
  },
}

// ── 选中/交换 (M0) ───────────────────────────────────────────────────

const swapKeys = {
  name: "swap",
  desc: "交换选中键与光标键",
  run(s) {
    const a = s.selected
    const b = s.cursor
    return ok({ ...s, rows: swap(s.rows, a.r, a.c, b.r, b.c), selected: null }, [EFFECT.APPLY])
  },
}

// ── 行列菜单 (M0 R 面板) ─────────────────────────────────────────────

const openRowMenu = {
  name: "menu.open",
  desc: "打开行列操作菜单",
  run: (s) => setPanel(s, PANEL.ROWMENU, { menuHighlight: 0 }),
}

const execRowMenu = {
  name: "menu.exec",
  desc: "执行菜单当前高亮项 (行/列增删)",
  run(s) {
    const { r, c } = s.cursor
    let outRows = s.rows
    let cursor = s.cursor
    switch (s.menuHighlight) {
      case 0:
        outRows = insertRow(outRows, r)
        cursor = { r: Math.max(0, r - 1), c: 0 }
        break
      case 1:
        outRows = insertRow(outRows, r + 1)
        cursor = { r: Math.min(r + 1, outRows.length - 1), c: 0 }
        break
      case 2:
        outRows = deleteRow(outRows, r)
        cursor = { r: Math.min(r, Math.max(outRows.length - 1, 0)), c: Math.min(c, outRows[r]?.length - 1 ?? 0) }
        break
      case 3:
        outRows = insertColumn(outRows, r, c)
        cursor = { r, c: c + 1 }
        break
      case 4:
        outRows = insertColumn(outRows, r, c + 1)
        cursor = { r, c }
        break
      case 5:
        outRows = deleteColumn(outRows, r, c)
        cursor = { r, c: Math.min(c, outRows[r]?.length - 1 ?? 0) }
        break
    }
    return ok(
      { ...s, rows: outRows, cursor, panel: PANEL.NONE, selected: null, menuHighlight: 0 },
      [EFFECT.APPLY],
    )
  },
}

// ── 矩阵编辑 (M1/M2 M 面板) ─────────────────────────────────────────

const openMatrix = {
  name: "matrix.open",
  desc: "打开预设键值矩阵并按当前值预选",
  run(s) {
    const e = s.rows[s.cursor.r]?.[s.cursor.c]
    const atom = s.mode === "1" ? (e?.type === "popup" ? e.main : e) : e?.type === "popup" ? e.popupNode : null
    const selected = atom ? collectLeaves(atom) : []
    return setPanel(s, PANEL.MATRIX, { matrix: { highlight: 0, selected } })
  },
}

const matrixAdd = {
  name: "matrix.add",
  desc: "将高亮格加入已选序列",
  run(s) {
    const cell = matrixAtomAtHighlight(s)
    if (s.matrix.selected.some((a) => sameAtom(a, cell))) return ok(s)
    return ok({ ...s, matrix: { ...s.matrix, selected: [...s.matrix.selected, cell] } })
  },
}

const matrixDel = {
  name: "matrix.del",
  desc: "将高亮格从已选序列移除",
  run(s) {
    const cell = matrixAtomAtHighlight(s)
    return ok({
      ...s,
      matrix: { ...s.matrix, selected: s.matrix.selected.filter((a) => !sameAtom(a, cell)) },
    })
  },
}

const matrixReset = {
  name: "matrix.reset",
  desc: "清空矩阵高亮与已选序列",
  run: (s) => ok({ ...s, matrix: { highlight: 0, selected: [] } }),
}

const finishMatrix = {
  name: "matrix.finish",
  desc: "矩阵完成: 按当前模式 M1=替换主键值 / M2=替换或移除弹出键, 并应用",
  run(s) {
    const forPopup = s.mode === "2"
    const atoms = s.matrix.selected
    const e = entryAt(s)
    let action = EFFECT.APPLY
    let entry = e

    if (atoms.length === 0) {
      // 空序列语义 (§4.5 规则 3): M2 且有弹出 = 移除弹出; 否则取消
      if (forPopup && e?.type === "popup") {
        entry = removePopup(e)
      } else {
        action = EFFECT.CANCEL
      }
    } else {
      const atom = makeCombo(atoms)
      if (comboDepth(atom) > COMBO_MAX_DEPTH) {
        return ok({ ...s, errors: ["组合键深度超过 3,已拒绝"] })
      }
      entry = forPopup ? replacePopup(e, atom) : replaceMain(e, atom)
    }

    const rows = atoms.length > 0 || (forPopup && e?.type === "popup")
      ? replaceEntry(s.rows, s.cursor, entry)
      : s.rows
    return ok(
      {
        ...s,
        rows,
        panel: PANEL.NONE,
        selected: null,
        matrix: { highlight: 0, selected: [] },
      },
      [action],
    )
  },
}

// ── 名称框 (M1/M2 N 面板) ───────────────────────────────────────────

function openNameInput(s, atom) {
  const draft = atom?.type === "label" ? atom.label : ""
  return setPanel(s, PANEL.NAME_INPUT, { nameDraft: draft })
}

const openMainName = {
  name: "name.openMain",
  desc: "打开主键名输入框",
  run(s) {
    const e = entryAt(s)
    const atom = e?.type === "popup" ? e.main : e
    return openNameInput(s, atom)
  },
}

const openPopupName = {
  name: "name.openPopup",
  desc: "打开弹出键名输入框 (无弹出键提示)",
  run(s) {
    const e = entryAt(s)
    if (e?.type !== "popup") {
      return ok({ ...s, errors: ["请先选择弹出键值,再编辑弹出键名"] })
    }
    return openNameInput(s, e.popupNode)
  },
}

const applyName = {
  name: "name.apply",
  desc: "确认名称框: 写入主键/弹出键名并应用",
  run(s) {
    const e = entryAt(s)
    if (s.mode === "2") {
      if (e?.type !== "popup") {
        return ok({ ...s, errors: ["请先选择弹出键值,再编辑弹出键名"] })
      }
      const rows = replaceEntry(s.rows, s.cursor, setPopupLabel(e, s.nameDraft))
      return ok({ ...s, rows, panel: PANEL.NONE, nameDraft: "" }, [EFFECT.APPLY])
    }
    const rows = replaceEntry(s.rows, s.cursor, setMainLabel(e, s.nameDraft))
    return ok({ ...s, rows, panel: PANEL.NONE, nameDraft: "" }, [EFFECT.APPLY])
  },
}

const discardName = {
  name: "name.discard",
  desc: "丢弃名称输入",
  run: (s) => setPanel(s, PANEL.NONE, { nameDraft: "" }),
}

// ── 注册表 ───────────────────────────────────────────────────────────
// 仅含"原子变换"原语; 效果类操作 (apply/cancel/exit/mode/sel/unsel/escpanel)
// 是内置操作, 由解释器直接处理 (BUILTIN_OPS)。

export const PRIMS = {
  move: move.run,
  swap: swapKeys.run,
  "menu.open": openRowMenu.run,
  "menu.exec": execRowMenu.run,
  "matrix.open": openMatrix.run,
  "matrix.add": matrixAdd.run,
  "matrix.del": matrixDel.run,
  "matrix.reset": matrixReset.run,
  "matrix.finish": finishMatrix.run,
  "name.openMain": openMainName.run,
  "name.openPopup": openPopupName.run,
  "name.apply": applyName.run,
  "name.discard": discardName.run,
}

export const PRIM_DESCRIPTIONS = Object.fromEntries(
  Object.values({
    move, swapKeys, openRowMenu, execRowMenu,
    openMatrix, matrixAdd, matrixDel, matrixReset, finishMatrix,
    openMainName, openPopupName, applyName, discardName,
  }).map((p) => [p.name, p.desc]),
)

// 原语名集合 (静态检查用)
export const PRIM_NAMES = new Set(Object.keys(PRIMS))
// ══════════════════════════════════════════════════════════════════
// mode interpreter (原 lib/tui/mode.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 行为解释器 — 由 ttoll 程序 (规则段+行为段) 数据驱动, 不再硬编码动作实现。
// 职责:
//   1. 内置插件 (actions.mjs) 作为第一个"插件"注册进 kernel (贡献守卫)
//   2. 程序 = 基座 tekl.ttoll (规则+行为) + 各插件贡献的 rules (按依赖序拼接)
//   3. parseProgram 解析: 规则表 + 动作→语句脚本; 未知原语/守卫/动作构建期报错
//   4. dispatch = 查表 → 守卫 (插件扩展) → 执行动作脚本 (内置操作+原语) → hooks
//   5. registerAction/wrapAction/registerPrim 即时热更新 (重建 dispatch)
// 纯函数层: 不 import opentui, 可 node --test。


// ── 内核 + 内置插件 ───────────────────────────────────────────────────

const kernel = createPluginKernel()
kernel.register(builtinPlugin)

export { kernel }

// 动态派发闭包 (registerAction 等会重建它)
let dispatchFn = null

// ── dispatch 构建: 聚合贡献 → 校验 → 编译脚本 → 闭包 ─────────────────

let seq = 0
function uniqueName(prefix) {
  return `${prefix}.${++seq}`
}

// 内置操作执行器: BUILTIN_OPS 中的语句由解释器直接处理
function execBuiltinOp(state, op, arg) {
  switch (op) {
    case "mode":
      return { state: { ...state, mode: arg, selected: null }, effects: [] }
    case "escpanel":
      return { state: { ...state, panel: PANEL.NONE, selected: null }, effects: [] }
    case "sel":
      return { state: { ...state, selected: { ...state.cursor } }, effects: [] }
    case "unsel":
      return { state: { ...state, selected: null }, effects: [] }
    case "apply":
      return { state, effects: [EFFECT.APPLY] }
    case "cancel":
      return { state, effects: [EFFECT.CANCEL] }
    case "exit":
      return { state, effects: [EFFECT.EXIT] }
    case "seterrors":
      return { state: { ...state, errors: [arg] }, effects: [] }
    default:
      throw new Error(`未知内置操作 '${op}'`)
  }
}

// 动作脚本 → 执行函数 (state, key, ctx) => { state, effects }
function compileScript(stmts, prims) {
  return function runScript(state, key, ctx) {
    let s = state
    const effects = []
    for (const st of stmts) {
      let r
      if (BUILTIN_OPS.has(st.op)) {
        r = execBuiltinOp(s, st.op, st.arg)
      } else {
        const prim = prims[st.op]
        if (!prim) throw new Error(`缺少原语 '${st.op}'`)
        r = prim(s, key, ctx)
      }
      s = r.state
      if (r.effects) effects.push(...r.effects)
    }
    return { state: s, effects, ok: effects.length > 0 || s !== state }
  }
}

function buildDispatch() {
  const jsActions = mergeContributions(kernel, "actions") ?? {} // 插件 JS 动作 (兼容面)
  const guards = mergeContributions(kernel, "guards") // {name: pred}
  const prims = { ...PRIMS, ...(mergeContributions(kernel, "prims") ?? {}) }
  const rulesLines = mergeContributions(kernel, "rules") ?? []
  const source = [BASE_TTOLL, ...(Array.isArray(rulesLines) ? rulesLines : [rulesLines])]
    .filter(Boolean)
    .join("\n")

  const guardNames = new Set(Object.keys(guards))
  const primNames = new Set(Object.keys(prims))
  const { rules, actions } = parseProgram(source, guardNames, primNames)
  const table = normalizeTable(buildDispatchTable(rules))

  // 动作解析: 显式注册的 JS 动作 (registerAction/wrapAction) 优先覆盖;
  // 否则用行为段脚本 (声明式行为)。
  const exec = (id) => {
    const a = jsActions[id]
    if (typeof a === "function") return a
    if (a?.handler) return a.handler
    const stmts = actions.get(id)
    if (stmts) return compileScript(stmts, prims)
    return null
  }
  const hooks = mergeContributions(kernel, "hooks") // { before?, after? }

  return function dispatch(state, key) {
    const token = KEY_TO_TTOLL[key] ?? key
    const ctx = { docHelpers }

    if (hooks?.before) {
      const r = hooks.before(state, token, ctx)
      if (r != null) return { state: r.state, effects: r.effects ?? [], ok: true }
    }

    const list = table.get(`${state.mode}|${state.panel}|${token}`)
    let out = { state, effects: [], ok: false }
    if (list) {
      for (const rule of list) {
        const guardsOk = !rule.guard || rule.guard.every((g) => evalGuard(guards, g, state))
        if (!guardsOk) continue
        const handler = exec(rule.action)
        if (!handler) continue
        const res = handler(state, token, { ...ctx, id: rule.action })
        if (res === null || res === undefined) continue
        out = { state: res.state, effects: res.effects ?? [], ok: res.ok ?? true }
        break
      }
    }

    if (hooks?.after) {
      const r = hooks.after(out, state, token, ctx)
      if (r != null) out = { state: r.state, effects: r.effects ?? [], ok: r.ok ?? out.ok }
    }
    return out
  }
}

// ttoll 面板代码 (-/R/M/N) → 运行时面板值, 使表键与 state.panel 一致
function normalizeTable(table) {
  const out = new Map()
  for (const [k, rules] of table) {
    const [mode, panel, key] = k.split("|")
    out.set(`${mode}|${PANEL_FROM_TTOLL[panel] ?? panel}|${key}`, rules)
  }
  return out
}

function evalGuard(guards, name, state) {
  const pred = guards[name]
  return pred ? pred(state) : true
}

// ── 插件 API (即时生效) ───────────────────────────────────────────────

// 注册一个动作插件: 贡献 actions/guards/rules/hooks/prims 中任一能力, 即重建 dispatch。
export function registerPlugin(def) {
  const off = kernel.register(def)
  dispatchFn = buildDispatch()
  return off // 注销器也会重建
}

export function registerAction(id, handler, meta = {}) {
  const off = registerPlugin(
    definePlugin({
      name: uniqueName(`action.${id}`),
      contribute: { actions: { [id]: { handler, meta } } },
    }),
  )
  return wrapOff(off)
}

// 注册带自身按键绑定的插件 (行为 + 触发点一次交付)
export function registerActionWithRules(id, handler, ttollLines, meta = {}) {
  const lines = Array.isArray(ttollLines) ? ttollLines : [ttollLines]
  const off = registerPlugin(
    definePlugin({
      name: uniqueName(`action.${id}`),
      contribute: { actions: { [id]: { handler, meta } }, rules: lines },
    }),
  )
  return wrapOff(off)
}

// 注册行为原语: 供 tekl.ttoll 行为段引用 (扩展行为词汇表)
export function registerPrim(name, fn, desc = "") {
  const off = registerPlugin(
    definePlugin({ name: uniqueName(`prim.${name}`), contribute: { prims: { [name]: fn } } }),
  )
  return wrapOff(off)
}

// 包装已有动作: wrapper(original, state, key, ctx)。依赖序保证包装者后于原实现。
export function wrapAction(id, wrapper, meta = {}) {
  const current = execSynced(id)
  const off = registerPlugin(
    definePlugin({
      name: uniqueName(`wrap.${id}`),
      contribute: {
        actions: {
          [id]: {
            handler: (s, k, ctx) => wrapper(current, s, k, ctx),
            meta: { ...meta, wrapped: true },
          },
        },
      },
    }),
  )
  return wrapOff(off)
}

export function registerGuard(name, pred) {
  const off = registerPlugin(
    definePlugin({ name: uniqueName(`guard.${name}`), contribute: { guards: { [name]: pred } } }),
  )
  return wrapOff(off)
}

function execSynced(id) {
  // 优先已注册 JS 动作, 其次行为段脚本
  const jsActions = mergeContributions(kernel, "actions") ?? {}
  const a = jsActions[id]
  if (typeof a === "function") return a
  if (a?.handler) return a.handler
  const stmts = currentActions().get(id)
  if (stmts) return compileScript(stmts, { ...PRIMS, ...(mergeContributions(kernel, "prims") ?? {}) })
  return null
}

// 注销器: 注销内核 + 重建 dispatch, 并保证幂等
function wrapOff(off) {
  let dead = false
  return function remove() {
    if (dead) return
    dead = true
    off()
    dispatchFn = buildDispatch()
  }
}

// ── 内省 ──────────────────────────────────────────────────────────────

function currentActions() {
  const rulesLines = mergeContributions(kernel, "rules") ?? []
  const source = [BASE_TTOLL, ...(Array.isArray(rulesLines) ? rulesLines : [rulesLines])]
    .filter(Boolean)
    .join("\n")
  const guards = mergeContributions(kernel, "guards")
  const prims = { ...PRIMS, ...(mergeContributions(kernel, "prims") ?? {}) }
  const { actions } = parseProgram(
    source,
    new Set(Object.keys(guards)),
    new Set(Object.keys(prims)),
  )
  return actions
}

export const actionIds = () => {
  const ids = new Set(currentActions().keys())
  for (const id of Object.keys(mergeContributions(kernel, "actions") ?? {})) ids.add(id)
  return ids
}

export const listActions = () => {
  const js = mergeContributions(kernel, "actions") ?? {}
  const out = []
  for (const [id, stmts] of currentActions()) {
    out.push({ id, handler: compileScript(stmts, { ...PRIMS, ...(mergeContributions(kernel, "prims") ?? {}) }), meta: { declared: true } })
  }
  for (const [id, spec] of Object.entries(js)) {
    out.push({ id, handler: typeof spec === "function" ? spec : spec.handler, meta: typeof spec === "function" ? {} : spec.meta })
  }
  return out
}

export const listPrims = () => Object.keys({ ...PRIMS, ...(mergeContributions(kernel, "prims") ?? {}) })

export const resolveGuard = (name) => mergeContributions(kernel, "guards")[name] ?? null
export const plugins = () => kernel.describe()

// ── 面板/令牌映射 (纯常量) ────────────────────────────────────────────

export const PANEL = {
  NONE: "none",
  ROWMENU: "rowmenu",
  MATRIX: "matrix",
  NAME_INPUT: "name",
}

export const PANEL_FROM_TTOLL = {
  "-": PANEL.NONE,
  "R": PANEL.ROWMENU,
  "M": PANEL.MATRIX,
  "N": PANEL.NAME_INPUT,
}

export const KEY_TO_TTOLL = {
  up: "UP",
  down: "DOWN",
  left: "LEFT",
  right: "RIGHT",
  enter: "ENTER",
  esc: "ESC",
  space: "SPACE",
  ctrl: "CTRL",
  alt: "ALT",
}

// ── 状态 ──────────────────────────────────────────────────────────────

export function initialState(rows) {
  return {
    mode: "0",
    rows,
    cursor: { r: 0, c: 0 },
    selected: null,
    panel: PANEL.NONE,
    matrix: { highlight: 0, selected: [] },
    menuHighlight: 0,
    nameDraft: "",
    errors: [],
  }
}

export function withErrors(state, errors) {
  return { state: { ...state, errors }, effects: [], ok: true }
}

// 公开分发器 (热更新: 内部引用 dispatchFn, registerAction 后即换新闭包)
export function dispatch(state, key) {
  if (!dispatchFn) dispatchFn = buildDispatch()
  return dispatchFn(state, key)
}

// 静态校验入口: 合并插件贡献后校验规则程序 (+ 死插件报告: 注册未用)
export function validateProgram(source = BASE_TTOLL) {
  const guards = mergeContributions(kernel, "guards")
  const prims = { ...PRIMS, ...(mergeContributions(kernel, "prims") ?? {}) }
  const rulesLines = mergeContributions(kernel, "rules") ?? []
  const full = [source, ...(Array.isArray(rulesLines) ? rulesLines : [rulesLines])]
    .filter(Boolean)
    .join("\n")
  const { rules } = checkBehaviorProgram(
    full,
    new Set(Object.keys(guards)),
    new Set(Object.keys(prims)),
  )
  const jsActions = mergeContributions(kernel, "actions") ?? {}
  const used = new Set(rules.map((r) => r.action))
  const declared = new Set([...currentActions().keys(), ...Object.keys(jsActions)])
  const dead = [...declared].filter((id) => !used.has(id))
  return { rules, declared: declared.size, used: used.size, dead }
}

export function dispatchTableFromRules(source) {
  return buildDispatchTable(parseRules(source))
}

// 基座规则程序: 主目录 ttoll/init.ttoll (主核心), 缺省回退仓库 tekl.ttoll

const BASE_TTOLL = behaviorBaseContent()

// ══════════════════════════════════════════════════════════════════
// store (原 lib/tui/store.mjs) — 合并进单文件插件后保留原有的分层节。
// ══════════════════════════════════════════════════════════════════
// 单例状态 (设计 §5 store.mjs) — 纯数据层,不 import opentui。
// 承载: ast(rows), mode, 焦点, 选中标记, 面板状态, errors/warnings。
// 由 mode.dispatch 驱动: dispatch 返回新状态后调 setState。


let _state = initialState([])
const listeners = new Set()

export function getState() {
  return _state
}

export function setState(next) {
  _state = next
  for (const fn of listeners) fn(_state)
}

export function subscribe(fn) {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

export function resetState(rows) {
  setState(initialState(rows))
}

let _docPath = null
let _propsPath = null
let _errors = []

export function setPaths(docPath, propsPath) {
  _docPath = docPath
  _propsPath = propsPath
}

export function getPaths() {
  return { docPath: _docPath, propsPath: _propsPath }
}

export function setGlobalErrors(errors) {
  _errors = errors
}

export function getGlobalErrors() {
  return _errors
}
// 单例状态门面 (原 plugins/tui.mjs 的 `import * as store` 等效): 渲染层经 store 对象取读写。
export const store = {
  getState,
  setState,
  subscribe,
  resetState,
  setPaths,
  getPaths,
  setGlobalErrors,
  getGlobalErrors,
}

// ── ui 插件声明 (进程内; io 走 keys 子进程) ──────────────────────────
export const uiPlugin = kernelDefinePlugin({
  name: "ui",
  version: "1.0.0",
  deps: ["keys"],
  capabilities: {
    "ui.dispatch": dispatch,
    "ui.initialState": initialState,
    "ui.withErrors": withErrors,
    "ui.eventToken": eventToken,
    "ui.validateProgram": validateProgram,
    "ui.actionIds": actionIds,
    "ui.listActions": listActions,
    "ui.listPrims": listPrims,
    "ui.registerPlugin": registerPlugin,
    "ui.registerAction": registerAction,
    "ui.registerActionWithRules": registerActionWithRules,
    "ui.wrapAction": wrapAction,
    "ui.registerGuard": registerGuard,
    "ui.registerPrim": registerPrim,
    "ui.store": store,
  },
  activate(ctx) {
    return {
      "ui.loadDoc": (file) => ctx.call("keys", "loadDoc", file),
      "ui.saveDoc": (rows, options = {}) => ctx.call("keys", "saveDoc", rows, options),
    }
  },
})
