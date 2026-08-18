import { test } from "node:test"
import assert from "node:assert/strict"
import { parse } from "../lib/keys.mjs"
import { printRows } from "../lib/keys.mjs"
import {
  entryMain,
  entryPopup,
  collectLeaves,
  comboDepth,
  insertColumn,
  insertRow,
  deleteColumn,
  deleteRow,
  swap,
  makeCombo,
  replaceMain,
  replacePopup,
  removePopup,
  setMainLabel,
  setPopupLabel,
} from "../lib/ui.mjs"

const rows = (src) => parse(src)
const text = (r) => printRows(r)

test("doc: insertColumn 列后新增默认 SPACE 键", () => {
  const out = insertColumn(rows("ESC TAB;"), 0, 1)
  assert.equal(text(out), "ESC SPACE TAB;")
})

test("doc: insertColumn 列前新增", () => {
  const out = insertColumn(rows("ESC TAB;"), 0, 0)
  assert.equal(text(out), "SPACE ESC TAB;")
})

test("doc: insertRow 行前/行后新增", () => {
  assert.equal(text(insertRow(rows("ESC;"), 0)), "SPACE;\nESC;")
  assert.equal(text(insertRow(rows("ESC;"), 1)), "ESC;\nSPACE;")
})

test("doc: deleteColumn 删空行自动移除该行", () => {
  const out = deleteColumn(rows("ESC;"), 0, 0)
  assert.equal(out.length, 0)
})

test("doc: deleteColumn 普通删除", () => {
  const out = deleteColumn(rows("ESC TAB;"), 0, 0)
  assert.equal(text(out), "TAB;")
})

test("doc: deleteRow", () => {
  const out = deleteRow(rows("ESC;\nTAB;"), 0)
  assert.equal(text(out), "TAB;")
})

test("doc: swap 交换两键", () => {
  const out = swap(rows("A B;"), 0, 0, 0, 1)
  assert.equal(text(out), "B A;")
})

test("doc: swap 同键为 no-op", () => {
  const r = rows("A B;")
  assert.equal(swap(r, 0, 0, 0, 0), r)
})

test("doc: makeCombo 单值 = 原子, 多值 = 嵌套链 (宏序 = 选择序)", () => {
  const k = (name, kind = "named") => ({ type: "key", key: name, kind })
  assert.deepEqual(makeCombo([k("CTRL", "special")]), k("CTRL", "special"))
  const combo = makeCombo([k("CTRL", "special"), k("ALT", "special"), k("X")])
  assert.equal(combo.type, "combo")
  assert.equal(text([[combo]]), "CTRL(ALT(X));")
})

test("doc: collectLeaves 组合键叶子按宏序", () => {
  const [[combo]] = rows("CTRL(ALT(X));")
  assert.deepEqual(collectLeaves(combo).map((n) => n.key), ["CTRL", "ALT", "X"])
})

test("doc: comboDepth 限制深度 ≤3", () => {
  const [[c2]] = rows("CTRL(ALT(X));")
  const [[c1]] = rows("CTRL(X);")
  assert.equal(comboDepth(c1), 2)
  assert.equal(comboDepth(c2), 3)
  assert.ok(comboDepth(c2) <= 3)
})

test("doc: replaceMain 替换主键值, 标签包裹保留", () => {
  const out = replaceMain(rows("HOME:Start;")[0][0], { type: "key", key: "END", kind: "named" })
  assert.equal(text([[out]]), "END:Start;")
})

test("doc: replaceMain popup 主键替换, 弹出保留", () => {
  const out = replaceMain(rows("ESC.F1;")[0][0], { type: "key", key: "TAB", kind: "named" })
  assert.equal(text([[out]]), "TAB.F1;")
})

test("doc: replacePopup 无弹出键 → 创建 A.A", () => {
  const out = replacePopup(rows("ESC;")[0][0], { type: "key", key: "F5", kind: "named" })
  assert.equal(text([[out]]), "ESC.F5;")
})

test("doc: replacePopup 已有弹出 → 替换", () => {
  const out = replacePopup(rows("ESC.F1;")[0][0], { type: "key", key: "F5", kind: "named" })
  assert.equal(text([[out]]), "ESC.F5;")
})

test("doc: removePopup 解除 A.A; 无弹出为 no-op", () => {
  assert.equal(text([[removePopup(rows("ESC.F1;")[0][0])]]), "ESC;")
  const bare = rows("ESC;")[0][0]
  assert.equal(removePopup(bare), bare)
})

test("doc: setMainLabel 加标签 / 清空标签", () => {
  const added = setMainLabel(rows("ESC;")[0][0], "quit")
  assert.equal(text([[added]]), "ESC:quit;")
  const cleared = setMainLabel(rows("ESC:quit;")[0][0], "")
  assert.equal(text([[cleared]]), "ESC;")
})

test("doc: setPopupLabel 无弹出键为 no-op", () => {
  const bare = rows("ESC;")[0][0]
  assert.equal(setPopupLabel(bare, "x"), bare)
})

test("doc: entryMain/entryPopup 访问器", () => {
  const popup = rows("ESC.F1;")[0][0]
  assert.equal(entryMain(popup).key, "ESC")
  assert.equal(entryPopup(popup).key, "F1")
  const bare = rows("ESC;")[0][0]
  assert.equal(entryMain(bare), bare)
  assert.equal(entryPopup(bare), null)
})
