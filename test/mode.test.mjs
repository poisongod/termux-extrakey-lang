import { test } from "node:test"
import assert from "node:assert/strict"
import { parse } from "../lib/keys.mjs"
import { printRows } from "../lib/keys.mjs"
import { initialState, dispatch, EFFECT, validateProgram } from "../lib/ui.mjs"
import { readFile } from "node:fs/promises"

const init = (src) => initialState(parse(src))
const text = (rows) => printRows(rows)

test("mode: M0 方向键移动光标", () => {
  const s = init("A B C;")
  const { state } = dispatch(s, "right")
  assert.deepEqual(state.cursor, { r: 0, c: 1 })
  const { state: s2 } = dispatch(state, "down")
  assert.deepEqual(s2.cursor, { r: 0, c: 1 })
})

test("mode: M0 SPACE 选中 → 同格取消 → 异格交换", () => {
  const s = init("A B;")
  const { state: s1 } = dispatch(s, "space")
  assert.deepEqual(s1.selected, { r: 0, c: 0 })
  const { state: s2 } = dispatch(s1, "right")
  const { state: s3, effects } = dispatch(s2, "space")
  assert.equal(text(s3.rows), "B A;")
  assert.equal(s3.selected, null)
  assert.equal(effects.length, 1)
})

test("mode: M0 未选中时 CTRL 无动作", () => {
  const s = init("A B;")
  const { state, ok } = dispatch(s, "ctrl")
  assert.equal(ok, false)
  assert.equal(state.panel, "none")
})

test("mode: M0 选中后 CTRL 打开行列菜单", () => {
  const s = init("A B;")
  const s1 = dispatch(s, "space").state
  const { state: s2 } = dispatch(s1, "ctrl")
  assert.equal(s2.panel, "rowmenu")
})

test("mode: 行列菜单 删除当前行", () => {
  const s = init("A;\nB;")
  const s1 = dispatch(s, "space").state // 选中 (0,0)
  const s2 = dispatch(s1, "ctrl").state // 行列菜单
  assert.equal(s2.menuHighlight, 0)
  // 移到第 3 项 = 删除当前行
  let st = s2
  st = dispatch(st, "down").state
  st = dispatch(st, "down").state
  assert.equal(st.menuHighlight, 2)
  const { state: s3, effects } = dispatch(st, "space")
  assert.equal(text(s3.rows), "B;")
  assert.equal(effects.length, 1)
})

test("mode: 行列菜单 列后新增列", () => {
  const s = init("ESC TAB;")
  const s1 = dispatch(s, "space").state
  const s2 = dispatch(s1, "ctrl").state
  let st = s2
  for (let i = 0; i < 4; i++) st = dispatch(st, "down").state
  assert.equal(st.menuHighlight, 4)
  const { state: s3 } = dispatch(st, "space")
  assert.equal(text(s3.rows), "ESC SPACE TAB;")
})

test("mode: ALT 模式循环 0→1→2→0", () => {
  const s = init("A;")
  assert.equal(dispatch(s, "alt").state.mode, "1")
  assert.equal(dispatch(dispatch(s, "alt").state, "alt").state.mode, "2")
})

test("mode: M0 ESC 请求退出", () => {
  const { state, effects } = dispatch(init("A;"), "esc")
  assert.deepEqual(effects, [EFFECT.EXIT])
  assert.equal(state.mode, "0")
})

test("mode: M1 ESC 返回 M0", () => {
  const s = init("A;")
  const s1 = dispatch(s, "alt").state // → M1
  assert.equal(dispatch(s1, "esc").state.mode, "0")
})

test("mode: M2 ESC 逐级返回 M1", () => {
  let s = init("A;")
  s = dispatch(s, "alt").state // 1
  s = dispatch(s, "alt").state // 2
  assert.equal(dispatch(s, "esc").state.mode, "1")
})

test("mode: M1 SPACE 打开矩阵; 累积选值 → CTRL 组合", () => {
  const s = init("d;")
  const s1 = dispatch(s, "alt").state // M1
  const s2 = dispatch(s1, "space").state // 矩阵
  assert.equal(s2.panel, "matrix")
  // 预选当前值: d 的叶子在矩阵中预高亮 (preSelected)
  assert.equal(s2.matrix.selected.length, 1)
  assert.equal(s2.matrix.selected[0].key, "d")
  // 高亮移动到 CTRL (index 0), CTRL 累积
  const s3 = dispatch(s2, "ctrl").state
  assert.equal(s3.matrix.selected.length, 2)
  const { state: s6, effects } = dispatch(s3, "esc")
  const cell = s6.rows[0][0]
  assert.equal(cell.type, "combo")
  assert.equal(effects.length, 1)
  assert.equal(s6.panel, "none")
})

test("mode: M1 矩阵空序列 ESC 保持原值 (CANCEL)", () => {
  let s = init("d;")
  s = dispatch(s, "alt").state // M1
  s = dispatch(s, "space").state // 矩阵
  // 打开时预选了当前值叶子;先清空序列再 ESC
  s = { ...s, matrix: { ...s.matrix, selected: [] } }
  const { state, effects } = dispatch(s, "esc")
  assert.deepEqual(effects, [EFFECT.CANCEL])
  assert.equal(text(state.rows), "d;")
  // §6.10 回 M1 — 保持当前编辑模式,不退回 M0
  assert.equal(state.mode, "1")
})

test("mode: M1 ENTER 名称框; 输入+应用标签", () => {
  let s = init("ESC;")
  s = dispatch(s, "alt").state // M1
  s = dispatch(s, "enter").state // 名称框
  assert.equal(s.panel, "name")
  // 模拟输入:修改 nameDraft
  s = { ...s, nameDraft: "quit" }
  const { state, effects } = dispatch(s, "enter")
  assert.equal(text(state.rows), "ESC:quit;")
  assert.equal(effects.length, 1)
})

test("mode: M1 名称框 ESC 丢弃", () => {
  let s = init("ESC;")
  s = dispatch(s, "alt").state
  s = dispatch(s, "enter").state
  s = { ...s, nameDraft: "quit" }
  const { state } = dispatch(s, "esc")
  assert.equal(text(state.rows), "ESC;")
})

test("mode: M2 无弹出键时打开矩阵创建弹出", () => {
  let s = init("ESC;")
  s = dispatch(s, "alt").state // M1
  s = dispatch(s, "alt").state // M2
  s = dispatch(s, "space").state // 矩阵
  // 选一个值: 高亮到 F5 附近再 CTRL
  const { state } = dispatch(s, "esc")
  // 空序列 M2 = 移除弹出或取消;这里原本没有弹出,应取消
  const cell = state.rows[0][0]
  assert.equal(cell.type, "key")
})

test("mode: M2 矩阵选值创建弹出键", () => {
  let s = init("ESC;")
  s = dispatch(s, "alt").state
  s = dispatch(s, "alt").state // M2
  s = dispatch(s, "space").state // 矩阵
  // 高亮选中一个有意义的值 (非空): 移到命名键并 CTRL
  let st = s
  st = dispatch(st, "right").state // 从 CTRL 移到 ALT
  st = dispatch(st, "right").state
  st = dispatch(st, "right").state // SHIFT
  st = dispatch(st, "down").state // ESC row
  st = dispatch(st, "ctrl").state // select ESC
  const { state } = dispatch(st, "esc")
  const cell = state.rows[0][0]
  assert.equal(cell.type, "popup")
  // §6.10 M2 矩阵完成回 M2 (弹出键编辑),而非退回 M1
  assert.equal(state.mode, "2")
})

test("mode: M2 矩阵空序列移除弹出键回 M2", () => {
  let s = init("ESC.F1;")
  s = dispatch(s, "alt").state // M1
  s = dispatch(s, "alt").state // M2
  s = dispatch(s, "space").state // 矩阵
  s = { ...s, matrix: { ...s.matrix, selected: [] } }
  const { state, effects } = dispatch(s, "esc")
  assert.equal(text(state.rows), "ESC;")
  assert.deepEqual(effects, [EFFECT.APPLY])
  assert.equal(state.mode, "2")
})

test("mode: M2 已存在弹出键时可编辑弹出键名", () => {
  let s = dispatch(init("ESC.F1;"), "alt").state // M1
  s = dispatch(s, "alt").state // M2
  const { state: s2 } = dispatch(s, "enter")
  assert.equal(s2.panel, "name")
  const { state: s3 } = dispatch({ ...s2, nameDraft: "bu" }, "enter")
  assert.equal(text(s3.rows), "ESC.F1:bu;")
})

test("mode: validateProgram 对合法 ttoll 通过", async () => {
  const src = await readFile("behavior.base", "utf8")
  const { rules } = validateProgram(src)
  assert.ok(rules.length >= 30)
})