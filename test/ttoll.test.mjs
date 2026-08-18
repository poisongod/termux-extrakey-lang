import { test } from "node:test"
import assert from "node:assert/strict"
import {
  parseRules,
  buildTable,
  checkProgram,
  reachability,
  tokenizeRule,
  BehaviorError,
} from "../lib/behavior.mjs"
import { readFile } from "node:fs/promises"

const MINIMAL = `
0.-.UP>移动焦点
0.-.SPACE?nosel>选中焦点键位
0.-.ALT>1
1.-.SPACE>弹出预设键值矩阵
1.-.ALT>2
2.-.ESC>返回主键模式
2.-.ALT>0
`

test("ttoll: tokenizeRule 基本规则", () => {
  const r = tokenizeRule("0.-.UP>移动焦点")
  assert.deepEqual(r, { mode: "0", panel: "-", key: "UP", guard: null, action: "移动焦点" })
})

test("ttoll: tokenizeRule 带守卫 (链式守卫为数组)", () => {
  const r = tokenizeRule("0.-.SPACE?nosel>选中焦点键位")
  assert.deepEqual(r, { mode: "0", panel: "-", key: "SPACE", guard: ["nosel"], action: "选中焦点键位" })
  const chain = tokenizeRule("2.M.ESC?matselno?haspopup>移除弹出键")
  assert.deepEqual(chain.guard, ["matselno", "haspopup"])
})

test("ttoll: tokenizeRule 跳过程序头/注释/空行", () => {
  assert.equal(tokenizeRule(""), null)
  assert.equal(tokenizeRule("(* 注释 *)"), null)
  assert.equal(tokenizeRule("K = 'ESC' | 'CTRL'"), null)
})

test("ttoll: tokenizeRule 非法规则报错", () => {
  assert.throws(() => tokenizeRule("0X.-.UP>foo"), BehaviorError)
  assert.throws(() => tokenizeRule("0.up>foo"), BehaviorError)
})

test("ttoll: 未知键/守卫报错", () => {
  assert.throws(() => parseRules("0.-.FOO>bar\n"), BehaviorError)
  assert.throws(() => parseRules("0.-.UP?weird>bar\n"), BehaviorError)
})

test("ttoll: parseRules 完整规则程序", async () => {
  const src = await readFile("behavior.base", "utf8")
  const rules = parseRules(src)
  assert.ok(rules.length >= 30)
  assert.ok(rules.every((r) => ["0", "1", "2"].includes(r.mode)))
  assert.ok(rules.every((r) => ["-", "R", "M", "N"].includes(r.panel)))
})

test("ttoll: buildTable 按模式分组", () => {
  const table = buildTable(parseRules(MINIMAL))
  assert.deepEqual([...table.keys()].sort(), ["0", "1", "2"])
  assert.equal(table.get("0").length, 3)
})

test("ttoll: reachability 三个模式从 0 可达", () => {
  const reachable = reachability(buildTable(parseRules(MINIMAL)))
  assert.deepEqual([...reachable].sort(), ["0", "1", "2"])
})

test("ttoll: checkProgram 动作白名单 + 可达性", () => {
  const ids = new Set(["移动焦点", "选中焦点键位", "弹出预设键值矩阵", "返回主键模式", "0", "1", "2"])
  const { table } = checkProgram(MINIMAL, ids)
  assert.equal(table.has("0"), true)
})

test("ttoll: checkProgram 未知动作报错", () => {
  assert.throws(() => checkProgram("0.-.UP>未知动作\n", new Set(["移动焦点"])), /unknown action id/)
})

test("ttoll: 不可达模式报错", () => {
  // 模式 2 无任何数值跳转入边
  assert.throws(
    () => checkProgram("0.-.UP>移动焦点\n2.-.ESC>foo\n", new Set(["移动焦点", "foo"])),
    /unreachable/,
  )
})

test("ttoll: checkProgram 实际规则程序通过 (动作白名单含所有 id)", async () => {
  const src = await readFile("behavior.base", "utf8")
  const rules = parseRules(src)
  const all = new Set(rules.map((r) => r.action))
  const { rules: out } = checkProgram(src, all)
  assert.equal(out.length, rules.length)
})