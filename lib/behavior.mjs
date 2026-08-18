// ttoll 行为核心语言 — 解析 + 静态检查 + 行动脚本 (行为全部声明化)。
// 单一程序 = 规则段 (声明"谁触发谁") + 行为段 (声明"动作做什么"),
// 运行时由通用解释器 (mode.mjs) 按程序驱动, 不存在硬编码的 JS 状态机。
//
// ── 规则段 (无空格 DSL, 每条规则换行结束) ─────────────────────────────
//   M.P.K[?G]...>id       : 模式+面板+键(链式守卫) → 动作 id
//   K = 'ESC'|'ALT'|'CTRL'|'SPACE'|'ENTER'
//   D = 'UP'|'DOWN'|'LEFT'|'RIGHT'
//   M = '0'|'1'|'2'
//   P = '-'|'R'|'M'|'N'                        (无/行列菜单/矩阵/名称框)
//   G = 'sel'|'nosel'|'same'|'diff'|'insel'|'noinsel'|'haspopup'|'nopopup'
//   同一 (M,P,键) 可声明多条规则,靠守卫区分; 运行时按声明序首条命中。
//
// ── 行为段 (动作定义, '@'id 换行后跟语句行) ───────────────────────────
//   @移动焦点
//     cursor.move
//   @保持原值
//     matrix.reset
//     escpanel
//     unsel
//     cancel
//   语句 = 内置操作 (mode/panel/sel/unsel/apply/cancel/exit/...)
//         或 插件贡献的原语名 (cursor.move/matrix.reset/menu.exec/...)。
//   程序 = 基座 tekl.ttoll 的规则+行为段 + 各插件贡献的 rules 拼接。

export class BehaviorError extends Error {
  constructor(message) {
    super(`behavior: ${message}`)
  }
}

export const MODES = /* #__PURE__ */ new Set(["0", "1", "2"])
export const FUNC_KEYS = /* #__PURE__ */ new Set(["ESC", "ALT", "CTRL", "SPACE", "ENTER"])
export const DIR_KEYS = /* #__PURE__ */ new Set(["UP", "DOWN", "LEFT", "RIGHT"])
export const KEYS = /* #__PURE__ */ new Set([...FUNC_KEYS, ...DIR_KEYS])
export const PANELS = /* #__PURE__ */ new Set(["-", "R", "M", "N"])
export const GUARDS = /* #__PURE__ */ new Set([
  "sel", "nosel", "same", "diff", "insel", "noinsel", "haspopup", "nopopup",
])

// 文法头行 (K/D/M/P/G/C = ...) 与注释混排在规则文件里,统一跳过
const GRAMMAR_HEADER = /^[A-Z]\s*=/

// ── 动作定义 (行为段) 的内置操作 ──────────────────────────────────────
// 运行时解释/操作见 mode.mjs;这里只声明合法名与参数形态。
export const BUILTIN_OPS = /* #__PURE__ */ new Set([
  "mode", "escpanel", "sel", "unsel", "apply", "cancel", "exit", "seterrors",
])

export function tokenizeRule(line) {
  const s = line.trim()
  if (s === "" || s.startsWith("(*")) return null
  if (GRAMMAR_HEADER.test(s)) return null
  // M.P.K[?G]...>id — 支持链式守卫 (?G?G 均须命中)
  const m = s.match(/^([0-2])\.([-RMN])\.([A-Z]+)((?:\?[\p{L}\p{N}_\-]+)*)>(.*)$/u)
  if (!m) throw new BehaviorError(`invalid rule: ${JSON.stringify(line)}`)
  const [, mode, panel, key, guardPart, action] = m
  const guards = guardPart ? [...guardPart.matchAll(/\?([\p{L}\p{N}_\-]+)/gu)].map((g) => g[1]) : []
  return { mode, panel, key, guard: guards.length ? guards : null, action: action.trim() }
}

// ── 行为段解析: '@id 换行后跟语句行' → { id, stmts } ─────────────────
export function tokenizeActionBlock(header) {
  const m = (header ?? "").trim().match(/^@([\p{L}\p{N}_.\-]+)$/u)
  if (!m) throw new BehaviorError(`invalid action block: ${JSON.stringify(header)}`)
  return m[1]
}

// 语句行: `op` 或 `op arg` (无空格 DSL 外的唯一允许空白是单个分隔空格)
export function tokenizeStatement(line) {
  const s = line.trim()
  if (s === "" || s.startsWith("(*")) return null
  const m = s.match(/^([\p{L}\p{N}_.\-]+)(?:\s+(.*))?$/u)
  if (!m) throw new BehaviorError(`invalid statement: ${JSON.stringify(line)}`)
  const [, op, arg] = m
  return { op, arg: (arg ?? "").trim() || null }
}

export function parseRules(source, guardNames = GUARDS) {
  const rules = []
  let inBlock = false
  for (const line of source.split("\n")) {
    const s = line.trim()
    if (s.startsWith("@")) {
      inBlock = true
      continue
    }
    if (/^[0-2]\./.test(s)) {
      inBlock = false // 规则行 (可后置于行为段)
      const t = tokenizeRule(line)
      if (!t) continue
      if (!KEYS.has(t.key)) throw new BehaviorError(`unknown key '${t.key}'`)
      for (const g of t.guard ?? []) {
        if (!guardNames.has(g)) throw new BehaviorError(`unknown guard '${g}'`)
      }
      rules.push(t)
      continue
    }
    if (inBlock) continue // 行为段语句不属于规则
  }
  return rules
}

// 解析整个程序 (规则段 + 行为段)。返回 { rules, actions: Map(id→stmts) }
// 行分类: '@' = 行为段头; '^[0-2]\.' = 规则行; 其余 (在行为段内) = 语句。
// 规则行可穿插在行为段之后 (插件贡献的 rules 追加在基座程序尾部)。
export function parseProgram(source, guardNames = GUARDS, primNames = new Set()) {
  const rules = []
  const actions = new Map()
  let id = null
  let stmts = []
  const flush = () => {
    if (id !== null) actions.set(id, stmts)
    id = null
    stmts = []
  }
  for (const line of source.split("\n")) {
    const s = line.trim()
    if (s.startsWith("@")) {
      flush()
      id = tokenizeActionBlock(s)
      continue
    }
    if (/^[0-2]\./.test(s)) {
      // 规则行: 结束上一个行为段后再解析 (layout 上可后置)
      flush()
      const t = tokenizeRule(line)
      if (!t) continue
      if (!KEYS.has(t.key)) throw new BehaviorError(`unknown key '${t.key}'`)
      for (const g of t.guard ?? []) {
        if (!guardNames.has(g)) throw new BehaviorError(`unknown guard '${g}'`)
      }
      rules.push(t)
      continue
    }
    if (id !== null) {
      const st = tokenizeStatement(s)
      if (st) {
        if (!BUILTIN_OPS.has(st.op) && !primNames.has(st.op)) {
          throw new BehaviorError(`unknown statement op '${st.op}' (action '${id}')`)
        }
        stmts.push(st)
      }
    }
  }
  flush()
  return { rules, actions }
}

// 分发表: (M,P,键) → 规则列表 (声明序,守卫未求值)
export function buildDispatchTable(rules) {
  const table = new Map()
  for (const r of rules) {
    const k = dispatchKey(r)
    if (!table.has(k)) table.set(k, [])
    table.get(k).push(r)
  }
  return table
}

export function dispatchKey(r) {
  return `${r.mode}|${r.panel}|${r.key}`
}

// 按模式分组 (兼容 / 审计用)
export function buildTable(rules) {
  const table = new Map()
  for (const r of rules) {
    if (!table.has(r.mode)) table.set(r.mode, [])
    table.get(r.mode).push(r)
  }
  return table
}

// 静态检查 (§6.10): 键合法性 / 守卫合法性 / 动作在白名单 / 模式可达
export function checkProgram(source, actionIds, guardNames = GUARDS) {
  const rules = parseRules(source, guardNames)
  const table = buildTable(rules)
  const reachable = reachability(table)

  for (const r of rules) {
    if (actionIds && !actionIds.has(r.action)) {
      throw new BehaviorError(`unknown action id '${r.action}'`)
    }
    if (!reachable.has(r.mode)) {
      throw new BehaviorError(`mode '${r.mode}' unreachable from mode 0`)
    }
  }
  return { rules, table }
}

// 行为程序静态检查: 规则动作都必须有对应行为段定义 (解释器按声明驱动)。
// 返回 { rules, actions, table }
export function checkBehaviorProgram(source, guardNames = GUARDS, primNames = new Set()) {
  const { rules, actions } = parseProgram(source, guardNames, primNames)
  const table = buildTable(rules)
  const reachable = reachability(table)

  for (const r of rules) {
    if (!actions.has(r.action)) {
      throw new BehaviorError(`unknown action id '${r.action}' (无行为段定义)`)
    }
    if (!reachable.has(r.mode)) {
      throw new BehaviorError(`mode '${r.mode}' unreachable from mode 0`)
    }
  }
  return { rules, actions, table }
}

// 从模式 0 出发,经动作 `0`/`1`/`2` (模式跳转) BFS 可达模式集
export function reachability(table) {
  const edges = new Map([...table.keys()].map((m) => [m, new Set()]))
  for (const r of [...table.values()].flat()) {
    if (/^[0-2]$/.test(r.action)) {
      edges.get(r.mode)?.add(r.action)
    }
  }
  const seen = new Set()
  const queue = ["0"]
  for (; queue.length; ) {
    const cur = queue.shift()
    if (seen.has(cur)) continue
    seen.add(cur)
    for (const next of edges.get(cur) ?? []) {
      if (!seen.has(next)) queue.push(next)
    }
  }
  return seen
}