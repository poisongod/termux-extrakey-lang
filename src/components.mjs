// TUI 组件 (设计 §5 components/) — createElement 无 JSX,零构建。
// 组件是纯展示: 从 store 快照读状态,不直接改 store。

import { createElement } from "react"
import { printEntry } from "../lib/keys.mjs"
import { MATRIX, matrixCell } from "../lib/ui.mjs"

const ROW_MENU_ITEMS = ["行前新增行", "行后新增行", "删除当前行", "列前新增列", "列后新增列", "删除当前列"]

export function ModeBar({ mode }) {
  const names = { "0": "键盘编辑", "1": "主键编辑", "2": "弹出键编辑" }
  const modes = ["0", "1", "2"]
  const content = modes
    .map((m) => (m === mode ? `［${names[m]}］` : `[${names[m]}]`))
    .join("  ")
  return createElement("text", { content: `  ${content}  ` })
}

export function Grid({ state }) {
  const rows = state.rows
  const lines = []
  if (rows.length === 0) {
    lines.push(createElement("text", { key: "empty", content: "  (空白键盘 — 用行列菜单添加按键)" }))
  }
  for (let r = 0; r < rows.length; r++) {
    const row = rows[r]
    const cells = row.map((entry, c) => {
      const isCursor = state.panel === "none" && state.cursor.r === r && state.cursor.c === c
      const isSelected = state.selected?.r === r && state.selected?.c === c
      const cell = printEntry(entry)
      const border = isSelected ? "※" : isCursor ? "●" : " "
      return createElement("text", {
        key: c,
        content: `${border}${cell}${border}`,
      })
    })
    lines.push(createElement("text", { key: `row${r}`, content: `  ${cells.map(c => c.props.content).join(" ")}` }))
  }
  return createElement("box", { flexDirection: "column" }, lines)
}

export function MenuPanel({ state }) {
  const items = ROW_MENU_ITEMS.map((label, i) => ({
    content: `  ${i === state.menuHighlight ? "▸" : " "} ${label}`,
    fg: i === state.menuHighlight ? [255, 215, 135] : undefined,
  }))
  return createElement(
    "box",
    { border: true, title: "行列操作", flexDirection: "column" },
    items.map((it, i) => createElement("text", { key: i, content: it.content })),
    createElement("text", { content: "  ↑↓ 选择 · SPACE 执行 · ESC 关闭" }),
  )
}

export function MatrixPanel({ state }) {
  const sel = new Set(state.matrix.selected.map((a) => `${a.type}:${a.key}:${a.kind}`))
  const highlight = state.matrix.highlight
  const { cell: hcell } = matrixCell(highlight)
  const hkey = `${hcell.type}:${hcell.key}:${hcell.kind}`
  const selectedMark = sel.has(hkey) ? "✓" : " "
  const lines = []
  lines.push(createElement("text", {
    key: "hdr",
    content: `  已选: [${state.matrix.selected.map((a) => a.key).join(" ")}]  (Ctrl+↑↓←→ 累积)`,
  }))
  const idx = { i: 0 }
  const rows = [["CTRL", "ALT", "FN", "SHIFT"].map((k) => ({ key: k, kind: "special" }))]
  // 展平 MATRIX 为行 (按 matrixCell 的 row 分组)
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
      const content = `[${mark}${isHi ? "●" : " "}${cell.key}]`
      return createElement("text", { key: i, content })
    })
    lines.push(createElement("text", { key: `m${rowIdx}`, content: cells.map(c => c.props.content).join(" ") }))
  }
  lines.push(createElement("text", { key: "hint", content: "  ↑↓←→ 移动 · CTRL 选值 · ESC 完成" }))
  return createElement("box", { border: true, title: "预设键值", flexDirection: "column" }, lines)
}

export function NameInput({ state }) {
  const title = state.mode === "1" ? "主键名" : "弹出键名"
  return createElement(
    "box",
    { border: true, title, flexDirection: "column" },
    createElement("text", { content: `  [${state.nameDraft}_]` }),
    createElement("text", { content: "  输入任意文本 · ENTER 确认 · ESC 取消" }),
  )
}

export function Problems({ state, services }) {
  const store = services?.store
  const global = store?.getGlobalErrors ? store.getGlobalErrors() : []
  const msgs = [...(state.errors ?? []), ...global]
  if (msgs.length === 0) return createElement("text", { content: "" })
  return createElement(
    "box",
    { border: true, title: "问题", flexDirection: "column" },
    msgs.map((m, i) => createElement("text", { key: i, content: `  ⚠ ${m}` })),
  )
}

export function StatusBar({ state }) {
  const hint = {
    none: "←→↑↓ 移动 · ALT 模式 · SPACE 选中/矩阵 · CTRL 菜单 · ENTER 名称 · ESC 返回/退出",
    rowmenu: "↑↓ 选择 · SPACE 执行 · ESC 关闭",
    matrix: "↑↓←→ 移动 · CTRL 选值(可累积) · ESC 完成",
    name: "输入文本 · ENTER 确认 · ESC 取消",
  }[state.panel] ?? ""
  return createElement("text", { content: `  ${hint}` })
}
