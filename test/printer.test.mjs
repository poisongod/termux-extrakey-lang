import { test } from "node:test"
import assert from "node:assert/strict"
import { parse } from "../lib/keys.mjs"
import { printRows, printEntry, printLiteral } from "../lib/keys.mjs"

// 逐条覆盖 tekl.ebnf 产生式的打印 + round-trip
const rt = (src) => {
  const ast = parse(src)
  return printRows(ast)
}

test("printer: N 命名单键", () => {
  assert.equal(rt("ESC TAB CTRL;"), "ESC TAB CTRL;")
  assert.equal(rt("A_1 my_key 2;"), "A_1 my_key 2;")
})

test("printer: 命名键保留原大小写", () => {
  assert.equal(rt("esc KeyBoard;"), "esc KeyBoard;")
})

test("printer: 引号字面键回补引号 (双引号优先)", () => {
  assert.equal(rt('"~" "/" \'-\';'), '"~" "/" "-";')
  assert.equal(printLiteral('"'), "'\"'")
  assert.equal(printLiteral("~"), '"~"')
})

test("printer: 组合键递归打印", () => {
  assert.equal(rt("CTRL(C);"), "CTRL(C);")
  assert.equal(rt("CTRL(ALT(X));"), "CTRL(ALT(X));")
})

test("printer: 标签打印 node:label", () => {
  assert.equal(rt("ESC:quit;"), "ESC:quit;")
  assert.equal(rt("CTRL(C):copy;"), "CTRL(C):copy;")
})

test("printer: 弹出键打印 main.popupNode", () => {
  assert.equal(rt("ESC.F1;"), "ESC.F1;")
  assert.equal(rt("CTRL(C).F5;"), "CTRL(C).F5;")
  assert.equal(rt("ESC.CTRL(A);"), "ESC.CTRL(A);")
  assert.equal(rt("ESC.A:B;"), "ESC.A:B;")
})

test("printer: 行以分号结尾, 行间换行", () => {
  assert.equal(rt("ESC;\nTAB;\nCTRL;"), "ESC;\nTAB;\nCTRL;")
  assert.equal(rt("ESC TAB;\nCTRL ALT FN;"), "ESC TAB;\nCTRL ALT FN;")
})

test("printer: round-trip 逐节点相等 (多产生式混合)", () => {
  const cases = [
    "ESC TAB CTRL ALT;",
    "CTRL(C) ALT(A) HOME:Start ESC.F1;",
    "DRAWER KEYBOARD SCROLL SPACE ENTER\n  BACKSLASH QUOTE APOSTROPHE;",
    "KEYBOARD.CTRL(d);",
    "ESC.CTRL(A);",
    'CTRL("-");',
    "ESC.\"~\";",
  ]
  for (const src of cases) {
    const ast = parse(src)
    const out = printRows(ast)
    const ast2 = parse(out)
    assert.deepEqual(ast2, ast, `round-trip failed for ${JSON.stringify(src)} -> ${out}`)
  }
})

test("printer: printEntry 单条目打印", () => {
  const [row] = parse("CTRL(C):copy ESC.F1;")
  assert.equal(printEntry(row[0]), "CTRL(C):copy")
  assert.equal(printEntry(row[1]), "ESC.F1")
})
