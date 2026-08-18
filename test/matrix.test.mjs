import { test } from "node:test"
import assert from "node:assert/strict"
import {
  MODIFIER_ROW,
  NAMED_ROWS,
  LITERAL_ROW,
  MATRIX,
  matrixCell,
  moveIndex,
  findIndex,
  preHighlight,
  selectionToAtom,
} from "../lib/ui.mjs"

const named = (k) => ({ type: "key", key: k, kind: "named" })
const special = (k) => ({ type: "key", key: k, kind: "special" })
const literal = (k) => ({ type: "key", key: k, kind: "literal" })

test("matrix: 分区行定义", () => {
  assert.deepEqual(MODIFIER_ROW.map((c) => c.key).sort(), ["ALT", "CTRL", "FN", "SHIFT"])
  assert.ok(NAMED_ROWS.flat().some((c) => c.key === "ESC"))
  assert.ok(NAMED_ROWS.flat().some((c) => c.key === "F1"))
  assert.ok(NAMED_ROWS.flat().every((c) => c.kind === "named"))
  assert.ok(LITERAL_ROW.every((c) => c.kind === "literal"))
  assert.ok(LITERAL_ROW.some((c) => c.key === "~"))
})

test("matrix: 不含特殊键/排除键混入命名区", () => {
  const keys = NAMED_ROWS.flat().map((c) => c.key)
  for (const k of ["CTRL", "DRAWER", "KEYBOARD"]) assert.ok(!keys.includes(k), k)
})

test("matrix: findIndex 按 kind 区分同名", () => {
  assert.equal(findIndex(special("CTRL")), 0)
  assert.equal(findIndex(named("F1")) >= 0, true)
  assert.equal(findIndex(literal("~")) >= 0, true)
  assert.equal(findIndex(named("CTRL")), -1)
})

test("matrix: matrixCell 线性↔二维一致", () => {
  for (let i = 0; i < MATRIX.length; i++) {
    const { cell, row, col } = matrixCell(i)
    assert.equal(cell.key, MATRIX[i].key)
    assert.ok(row >= 0 && col >= 0)
  }
})

test("matrix: moveIndex 方向环绕", () => {
  // CTRL(0) 向左环绕到本行末 SHIFT
  const left = moveIndex(0, "left")
  assert.equal(matrixCell(left).cell.key, "SHIFT")
  // 上方向从修饰行到末行
  const up = moveIndex(0, "up")
  assert.ok(matrixCell(up).row > 0)
})

test("matrix: preHighlight 组合键叶子预高亮", () => {
  const idx = preHighlight({
    type: "combo",
    node: special("CTRL"),
    inner: { type: "combo", node: special("ALT"), inner: named("X") },
  })
  assert.deepEqual(idx, [findIndex(special("CTRL")), findIndex(special("ALT")), -1])
})

test("matrix: preHighlight 单键", () => {
  assert.deepEqual(preHighlight(named("F1")), [findIndex(named("F1"))])
  assert.deepEqual(preHighlight(null), [])
})

test("matrix: selectionToAtom 单值/多值", () => {
  assert.deepEqual(selectionToAtom([named("A")]), named("A"))
  const combo = selectionToAtom([special("CTRL"), named("d")])
  assert.equal(combo.type, "combo")
  assert.deepEqual(combo.node, special("CTRL"))
  assert.deepEqual(combo.inner, named("d"))
})