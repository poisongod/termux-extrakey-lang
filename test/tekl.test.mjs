import { test } from "node:test"
import assert from "node:assert/strict"
import { parse, ParseError } from "../src/tekl/parser.mjs"
import { compile, CompileError } from "../src/tekl/compiler.mjs"

// EBNF (tekl.ebnf)
// N ::= 命名键 (ESC, TAB, ..., 或 [A-Za-z0-9_-]*)
// S ::= "CTRL"|"ALT"|"FN"|"SHIFT"
// A ::= N | S | A(A) | A:N
// M ::= A | A.A | M M | M'\n' | M';\n'   (;\n 是键盘行分隔;裸 \n 是代码换行)

const comp = (src) => compile(parse(src))

test("N: 命名单键输出紧凑字符串", () => {
  assert.deepEqual(comp("ESC;"), [["ESC"]])
  assert.deepEqual(comp("F12;"), [["F12"]])
})

test("N: [A-Za-z0-9_]* 任意键名", () => {
  assert.deepEqual(comp("A_1 my_key 2;"), [["A_1", "my_key", "2"]])
})

test("N: 连字符不在键名字符集", () => {
  assert.throws(() => parse("A-1;"), /unexpected character/)
})

test("S: 特殊修饰键作为单键按钮", () => {
  assert.deepEqual(comp("CTRL ALT FN SHIFT;"), [["CTRL", "ALT", "FN", "SHIFT"]])
})

test("A(A): 组合键 = 外部键修饰内部键 -> macro", () => {
  assert.deepEqual(comp("CTRL(C);"), [[{ macro: "CTRL C", display: "CTRL C" }]])
})

test("A(A): 外部可以是任意 A(不限于修饰键)", () => {
  assert.deepEqual(comp("HOME(UP);"), [[{ macro: "HOME UP", display: "HOME UP" }]])
})

test("A(A): 嵌套 -> 展平为空格宏", () => {
  assert.deepEqual(comp("CTRL(ALT(X));"), [[{ macro: "CTRL ALT X", display: "CTRL ALT X" }]])
  assert.deepEqual(comp("CTRL(f(d));"), [[{ macro: "CTRL f d", display: "CTRL f d" }]])
})

test("A:N: 重命名 = 显示文本", () => {
  assert.deepEqual(comp("ESC:esc;"), [[{ key: "ESC", display: "esc" }]])
})

test("A:A 的 A 可为任意 A:组合键后重命名", () => {
  assert.deepEqual(comp("CTRL(C):copy;"), [[{ macro: "CTRL C", display: "copy" }]])
})

test("A.A: 弹出键", () => {
  assert.deepEqual(comp("ESC.F1;"), [[{ key: "ESC", popup: "F1" }]])
})

test("A.A: 主键可为组合键", () => {
  assert.deepEqual(comp("CTRL(C).F5;"), [
    [{ macro: "CTRL C", display: "CTRL C", popup: "F5" }],
  ])
})

test("A.A: 弹出值可为主键", () => {
  assert.deepEqual(comp("ESC.CTRL(A);"), [
    [{ key: "ESC", popup: { macro: "CTRL A", display: "CTRL A" } }],
  ])
})

test("A.A: 弹出值可为重命名键", () => {
  assert.deepEqual(comp("ESC.A:B;"), [[{ key: "ESC", popup: { key: "A", display: "B" } }]])
})

test("M M: 空格序列排成一行", () => {
  assert.deepEqual(comp("ESC TAB CTRL;"), [["ESC", "TAB", "CTRL"]])
})

test("M'\\n': 裸换行只是代码换行, 不产生新行", () => {
  assert.deepEqual(comp("ESC\nTAB\nCTRL;"), [["ESC", "TAB", "CTRL"]])
})

test("M';\\n': 分号换行分隔键盘行", () => {
  assert.deepEqual(comp("ESC;\nTAB;\nCTRL;"), [["ESC"], ["TAB"], ["CTRL"]])
})

test("解析错误: 组合键必须闭合", () => {
  assert.throws(() => parse("CTRL(C;"), ParseError)
})

test("解析错误: 弹出键少右侧", () => {
  assert.throws(() => parse("ESC.;"), ParseError)
})

test("编译错误: 内部带非键节点(重命名)不可放进 ()", () => {
  assert.throws(() => comp("CTRL(ALT:X);"), CompileError)
})

test("重复特殊键在同一行被拒绝", () => {
  assert.throws(() => parse("ESC CTRL CTRL;"), /\bCTRL\b/)
})

test("组合键只含重命名外层不可编译", () => {
  assert.throws(() => comp("CTRL:C(A);"), CompileError)
})