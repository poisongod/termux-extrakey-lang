// TUI 组件 (设计 §5 components/) — createElement 无 JSX,零构建。
// 组件是纯展示: 从 store 快照读状态,不直接改 store。
// 现代风格: 圆角容器 + 背景高亮 chip (光标/选中用 bg 高亮, 不堆逐格边框)。

import { createElement } from "react"
import { useTerminalDimensions } from "@xincli/opentui-react"
import { printEntry } from "../lib/keys.mjs"
import { MATRIX, matrixCell } from "../lib/ui.mjs"

// ── 配色 (暗色终端友好) ──────────────────────────────────────────────
const ACCENT = "#00AAFF"
const ACCENT_DIM = "#0A3550"
const ACCENT_FG = "#A8E2FF"
const GOLD = "#FFB84D"
const GOLD_DIM = "#4A3410"
const GOLD_FG = "#FFD9A0"
const FG = "#E6E6E6"
const DIM = "#8A8A8A"
const PANEL_BORDER = "#3A3F47"
const ERROR = "#FF7A6B"

const ROW_MENU_ITEMS = ["行前新增行", "行后新增行", "删除当前行", "列前新增列", "列后新增列", "删除当前列"]
const MODE_NAMES = { "0": "键盘编辑", "1": "主键编辑", "2": "弹出键编辑" }
const MODES = ["0", "1", "2"]
const CELL_MIN_W = 3

// 比例分配: 宽度 ∝ 自然宽度, 最小 CELL_MIN_W, largest-remainder 补齐余额保证总和精确
function distributeWidths(naturals, total, minW) {
  const n = naturals.length
  if (n === 0) return []
  if (total <= 0) return naturals.map(() => 1)
  const base = Math.max(1, Math.floor(total / n))
  const min = Math.min(minW, base)
  const sum = naturals.reduce((a, b) => a + b, 0) || n
  const ex = naturals.map((w, i) => {
    const exact = (w * total) / sum
    return { i, lo: Math.floor(exact), frac: exact - Math.floor(exact) }
  })
  let used = ex.reduce((a, e) => a + e.lo, 0)
  const order = [...ex].sort((a, b) => b.frac - a.frac)
  let k = 0
  while (used < total) {
    ex[order[k % n].i].lo += 1
    used += 1
    k += 1
  }
  for (const e of ex) if (e.lo < min) e.lo = min
  return ex.map((e) => e.lo)
}

// 把内容居中填充到指定列宽 (超出则截断), 保证每个键位格精确占 col 列
function fitCell(content, col) {
  if (col <= 0) return ""
  if (content.length >= col) return content.slice(0, col)
  const left = Math.floor((col - content.length) / 2)
  return " ".repeat(left) + content + " ".repeat(col - content.length - left)
}

export function AppHeader() {
  return createElement(
    "box",
    { flexDirection: "column" },
    createElement("text", {
      content: "  TERMUX-EXTRAKEY  ·  键盘布局编辑器",
      fg: ACCENT,
    }),
    createElement("text", { content: "", fg: DIM }),
  )
}

export function ModeBar({ mode }) {
  const chips = MODES.map((m) => {
    const active = m === mode
    return createElement("text", {
      key: m,
      content: `  ${m} · ${MODE_NAMES[m]}  `,
      fg: active ? "#00222E" : DIM,
      bg: active ? ACCENT : undefined,
    })
  })
  return createElement("box", { flexDirection: "row", columnGap: 1 }, ...chips)
}

export function Grid({ state }) {
  // 比例自适应: 键位宽 ∝ 键位文本 (单个键位适配行比例), 行总宽 = 终端列宽减边框内边距 (键盘适配终端)
  const { width: cols = 80 } = useTerminalDimensions()
  const inner = Math.max(4, cols - 4)
  const rows = state.rows
  const lines = []
  if (rows.length === 0) {
    lines.push(createElement("text", { key: "empty", content: "  (空白键盘 — 用行列菜单添加按键)", fg: DIM }))
  }
  for (let r = 0; r < rows.length; r++) {
    const row = rows[r]
    const naturals = row.map((entry) => printEntry(entry).length + 2)
    const rowWidth = Math.max(0, inner - Math.max(0, row.length - 1))
    const widths = distributeWidths(naturals, rowWidth, CELL_MIN_W)
    const cells = row.map((entry, c) => {
      const isCursor = state.panel === "none" && state.cursor.r === r && state.cursor.c === c
      const isSelected = state.selected?.r === r && state.selected?.c === c
      const content = fitCell(printEntry(entry), widths[c])
      return createElement("text", {
        key: c,
        content,
        fg: isCursor ? ACCENT_FG : isSelected ? GOLD_FG : FG,
        bg: isCursor ? ACCENT_DIM : isSelected ? GOLD_DIM : undefined,
      })
    })
    lines.push(createElement("box", { key: `row${r}`, flexDirection: "row", columnGap: 1 }, ...cells))
  }
  return createElement(
    "box",
    {
      border: true,
      borderStyle: "rounded",
      borderColor: PANEL_BORDER,
      title: ` 键盘布局  ·  ${rows.length} 行  `,
      titleColor: ACCENT,
      padding: 1,
      flexDirection: "column",
      rowGap: 1,
    },
    ...lines,
  )
}

export function MenuPanel({ state }) {
  const items = ROW_MENU_ITEMS.map((label, i) => {
    const hi = i === state.menuHighlight
    return createElement("text", {
      key: i,
      content: `  ${hi ? "▸" : " "} ${label}  `,
      fg: hi ? GOLD_FG : FG,
      bg: hi ? GOLD_DIM : undefined,
    })
  })
  return createElement(
    "box",
    {
      border: true,
      borderStyle: "rounded",
      borderColor: PANEL_BORDER,
      title: " 行列操作 ",
      titleColor: ACCENT,
      padding: 1,
      flexDirection: "column",
      rowGap: 1,
    },
    ...items,
    createElement("text", { key: "hint", content: "  ↑↓ 选择 · SPACE 执行 · ESC 关闭", fg: DIM }),
  )
}

export function MatrixPanel({ state }) {
  const sel = new Set(state.matrix.selected.map((a) => `${a.type}:${a.key}:${a.kind}`))
  const highlight = state.matrix.highlight
  const { cell: hcell } = matrixCell(highlight)
  const hkey = `${hcell.type}:${hcell.key}:${hcell.kind}`
  const lines = []
  lines.push(createElement("text", {
    key: "hdr",
    content: `  已选: [${state.matrix.selected.map((a) => a.key).join(" ")}]`,
    fg: DIM,
  }))
  const byRow = new Map()
  for (let i = 0; i < MATRIX.length; i++) {
    const { row } = matrixCell(i)
    if (!byRow.has(row)) byRow.set(row, [])
    byRow.get(row).push(i)
  }
  for (const [rowIdx, idxs] of [...byRow.entries()]) {
    const cells = idxs.map((i) => {
      const cell = MATRIX[i]
      const key = `${cell.type}:${cell.key}:${cell.kind}`
      const isHi = i === highlight
      const mark = sel.has(key) ? "✓" : " "
      const content = `${mark} ${cell.key}`
      return createElement("text", {
        key: i,
        content: `  ${content}  `,
        fg: sel.has(key) ? GOLD_FG : isHi ? ACCENT_FG : FG,
        bg: isHi ? ACCENT_DIM : sel.has(key) ? GOLD_DIM : undefined,
      })
    })
    lines.push(createElement("box", { key: `m${rowIdx}`, flexDirection: "row", columnGap: 1 }, ...cells))
  }
  lines.push(createElement("text", { key: "hint", content: "  ↑↓←→ 移动 · CTRL 选值(可累积) · ESC 完成", fg: DIM }))
  return createElement(
    "box",
    {
      border: true,
      borderStyle: "rounded",
      borderColor: PANEL_BORDER,
      title: " 预设键值 ",
      titleColor: ACCENT,
      padding: 1,
      flexDirection: "column",
      rowGap: 1,
    },
    ...lines,
  )
}

export function NameInput({ state }) {
  const title = state.mode === "1" ? " 主键名 " : " 弹出键名 "
  return createElement(
    "box",
    {
      border: true,
      borderStyle: "rounded",
      borderColor: PANEL_BORDER,
      title,
      titleColor: ACCENT,
      padding: 1,
      flexDirection: "column",
      rowGap: 1,
    },
    createElement("text", { content: `  [${state.nameDraft}_]`, fg: ACCENT_FG }),
    createElement("text", { content: "  输入任意文本 · ENTER 确认 · ESC 取消", fg: DIM }),
  )
}

export function Problems({ state, services }) {
  const store = services?.store
  const global = store?.getGlobalErrors ? store.getGlobalErrors() : []
  const msgs = [...(state.errors ?? []), ...global]
  if (msgs.length === 0) return createElement("text", { content: "" })
  return createElement(
    "box",
    {
      border: true,
      borderStyle: "rounded",
      borderColor: ERROR,
      title: " 问题 ",
      titleColor: ERROR,
      padding: 1,
      flexDirection: "column",
    },
    msgs.map((m, i) => createElement("text", { key: i, content: `  ⚠ ${m}`, fg: ERROR })),
  )
}

export function StatusBar({ state }) {
  const hint = {
    none: "←→↑↓ 移动 · ALT 模式 · SPACE 选中/矩阵 · CTRL 菜单 · ENTER 名称 · ESC 返回/退出",
    rowmenu: "↑↓ 选择 · SPACE 执行 · ESC 关闭",
    matrix: "↑↓←→ 移动 · CTRL 选值(可累积) · ESC 完成",
    name: "输入文本 · ENTER 确认 · ESC 取消",
  }[state.panel] ?? ""
  return createElement("text", { content: `  ${hint}`, fg: DIM })
}
