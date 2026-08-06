import { test } from "node:test"
import assert from "node:assert/strict"
import { parse, ParseError } from "../lib/tekl/parser.mjs"
import { TokenizerError } from "../lib/tekl/tokenizer.mjs"
import { compile, CompileError, collectWarnings, toExtraKeysJson } from "../lib/tekl/compiler.mjs"

// EBNF (tekl.ebnf)
// N ::= 命名键 (ESC, TAB, ..., 或 [A-Za-z0-9_-]*)
// S ::= "CTRL"|"ALT"|"FN"|"SHIFT"
// A ::= N | S | A(A) | A:N
// M ::= A | A.A | M M | M'\n' | M';\n'   (;\n 是键盘行分隔;裸 \n 是代码换行)

const comp = (src) => compile(parse(src))

test("N: 命名单键默认无 display, 输出字符串", () => {
  assert.deepEqual(comp("ESC;"), [["ESC"]])
  assert.deepEqual(comp("F12;"), [["F12"]])
})

test("N: [A-Za-z0-9_]* 任意键名, 普通标识符保留原样", () => {
  assert.deepEqual(comp("A_1 my_key 2;"), [["A_1", "my_key", "2"]])
})

test("N: 命名键大小写不敏感规范为大写, 普通标识符保留原样", () => {
  assert.deepEqual(comp("ctrl alt esc pGdN;"), [["CTRL", "ALT", "ESC", "PGDN"]])
  assert.deepEqual(comp("KeyBoard;"), [["KEYBOARD"]])
  assert.deepEqual(comp("foo;"), [["foo"]])
  assert.deepEqual(comp("PASTE;"), [["PASTE"]])
  assert.deepEqual(comp("paste;"), [["PASTE"]])
})

test("N: 引号字面键不转大写", () => {
  assert.deepEqual(comp(`"my_key" 'Esc';`), [["my_key", "Esc"]])
})

test("N: 字面键保留大小写, 标识符键大写 (混合)", () => {
  assert.deepEqual(comp(`esc "esc" TAB 'tab';`), [["ESC", "esc", "TAB", "tab"]])
})

test("N: 引号字面键 (任意字符)", () => {
  assert.deepEqual(comp(`"/" "~" '-' 'x';`), [["/", "~", "-", "x"]])
})

test("N: 引号字面键 (Unicode 直接书写)", () => {
  assert.deepEqual(comp(`"中" "文" '😀';`), [["中", "文", "😀"]])
})

test("N: 引号字符互换嵌套", () => {
  assert.deepEqual(comp(`'"' "'";`), [['"', "'"]])
})

test("N: 字面键可作组合内键", () => {
  assert.deepEqual(comp(`CTRL("-");`), [[{ macro: "CTRL -" }]])
})

test("N: 字面键可作弹出键", () => {
  assert.deepEqual(comp(`ESC."~";`), [[{ key: "ESC", popup: "~" }]])
})

test("N: 未闭合引号报错", () => {
  assert.throws(() => parse('"/'), TokenizerError)
})

test("N: 连字符不在键名字符集", () => {
  assert.throws(() => parse("A-1;"), /unexpected character/)
})

test("S: 特殊修饰键作为单键按钮", () => {
  assert.deepEqual(comp("CTRL ALT FN SHIFT;"), [["CTRL", "ALT", "FN", "SHIFT"]])
})

test("A(A): 组合键 = 外部键修饰内部键 -> macro", () => {
  assert.deepEqual(comp("CTRL(C);"), [[{ macro: "CTRL C" }]])
})

test("A(A): 修饰键大写, 被修饰键保留原样, 默认无 display (label 才生成)", () => {
  assert.deepEqual(comp("Ctrl(d);"), [[{ macro: "CTRL d" }]])
  assert.deepEqual(comp("ctrl(alt(x));"), [[{ macro: "CTRL ALT x" }]])
  assert.deepEqual(comp("CTRL(\"x\");"), [[{ macro: "CTRL x" }]])
  assert.deepEqual(comp("Ctrl(C):Ctrl_C;"), [[{ macro: "CTRL C", display: "Ctrl_C" }]])
  assert.deepEqual(comp("Ctrl:Ctrl;"), [[{ key: "CTRL", display: "Ctrl" }]])
})

test("A(A): 外部可以是任意 A(不限于修饰键)", () => {
  assert.deepEqual(comp("HOME(UP);"), [[{ macro: "HOME UP" }]])
})

test("A(A): 嵌套 -> 展平为空格宏", () => {
  assert.deepEqual(comp("CTRL(ALT(X));"), [[{ macro: "CTRL ALT X" }]])
  assert.deepEqual(comp("CTRL(f(d));"), [[{ macro: "CTRL f d" }]])
})

test("A:N: 标签不用引号, 引号标签报错", () => {
  assert.throws(() => comp("ESC:\"esc\";"), /label must not be quoted/)
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
    [{ macro: "CTRL C", popup: "F5" }],
  ])
})

test("A.A: 弹出值可为主键", () => {
  assert.deepEqual(comp("ESC.CTRL(A);"), [
    [{ key: "ESC", popup: { macro: "CTRL A" } }],
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

test("重复特殊键大小写不敏感 (Ctrl/CTRL 视为同一键)", () => {
  assert.throws(() => parse("ESC CTRL ctrl;"), /\bctrl\b/i)
  assert.throws(() => parse("esc Shift shift;"), /\bshift\b/i)
})

test("组合键只含重命名外层不可编译", () => {
  assert.throws(() => comp("CTRL:C(A);"), CompileError)
})

test("警告: 空 label 产生空 display 值", () => {
  assert.deepEqual(comp("ESC:;"), [[{ key: "ESC", display: "" }]])
  const warnings = collectWarnings(comp("ESC:;"))
  assert.ok(warnings.some((w) => /display.*值为空/.test(w)))
})

test("警告: 空 popup 值", () => {
  const ws = collectWarnings(comp("ESC.A:;"))
  assert.ok(ws.some((w) => /popup.*display.*值为空/.test(w)))
})

test("警告: 键盘行键位数不一致", () => {
  const warnings = collectWarnings(comp("ESC TAB;\nCTRL ALT FN;"))
  assert.ok(warnings.some((w) => /键位数不同/.test(w)))
})

test("警告: 缺少应用抽屉键 (DRAWER)", () => {
  const warnings = collectWarnings(comp("ESC TAB;"))
  assert.ok(warnings.some((w) => /DRAWER/.test(w)))
})

test("警告: 缺少键盘切换键 (KEYBOARD)", () => {
  const warnings = collectWarnings(comp("DRAWER ESC;"))
  assert.ok(warnings.some((w) => /KEYBOARD/.test(w)))
})

test("无警告: 含 DRAWER/KEYBOARD 且行长一致且非空", () => {
  const warnings = collectWarnings(comp("DRAWER KEYBOARD ESC;\nTAB CTRL ALT;"))
  assert.deepEqual(warnings.filter((w) => !/DRAWER|KEYBOARD/.test(w)), [])
})

test("无警告: DRAWER/KEYBOARD 出现在 popup 中", () => {
  const warnings = collectWarnings(comp("ESC.DRAWER;"))
  assert.ok(!warnings.some((w) => /DRAWER/.test(w)))
  const ws = collectWarnings(comp("KEYBOARD.CTRL(d);"))
  assert.ok(!ws.some((w) => /KEYBOARD/.test(w)))
})

test("宽松 JSON 产物: 字段名与标识符值不引号", () => {
  assert.equal(
    toExtraKeysJson("ESC.DRAWER;"),
    `[[{key: ESC, popup: DRAWER}]]`,
  )
})

test("宽松 JSON 产物: 组合键 popup 保留被修饰键原样", () => {
  assert.equal(
    toExtraKeysJson("KEYBOARD.Ctrl(d);"),
    `[[{key: KEYBOARD, popup: {macro: "CTRL d"}}]]`,
  )
})

test("宽松 JSON 产物: 符号键/空格宏才引号, 裸 display 不引号", () => {
  assert.equal(
    toExtraKeysJson(`ESC TAB "~" "/" '-';`),
    `[[ESC, TAB, "~", "/", "-"]]`,
  )
  assert.equal(
    toExtraKeysJson("ESC:esc;"),
    `[[{key: ESC, display: esc}]]`,
  )
  assert.equal(
    toExtraKeysJson("Ctrl(C):Ctrl_C;"),
    `[[{macro: "CTRL C", display: Ctrl_C}]]`,
  )
})

test("宽松 JSON 产物: 引号标签被拒 (标签不用引号)", () => {
  assert.throws(() => toExtraKeysJson("ESC:\"esc\";"), /label must not be quoted/)
})