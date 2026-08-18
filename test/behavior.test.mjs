import { test } from "node:test"
import assert from "node:assert/strict"
import { parse } from "../lib/keys.mjs"
import { printRows } from "../lib/keys.mjs"
import {
  parseProgram,
  parseRules,
  checkBehaviorProgram,
  tokenizeStatement,
  tokenizeActionBlock,
  BUILTIN_OPS,
  BehaviorError,
} from "../lib/behavior.mjs"
import { PRIM_NAMES } from "../lib/ui.mjs"
import {
  initialState,
  dispatch,
  registerPrim,
  registerAction,
  listPrims,
} from "../lib/ui.mjs"

const init = (src) => initialState(parse(src))
const text = (rows) => printRows(rows)

// ── 行为段解析 ───────────────────────────────────────────────────────

test("behavior: tokenizeActionBlock / tokenizeStatement", () => {
  assert.equal(tokenizeActionBlock("@移动焦点"), "移动焦点")
  assert.throws(() => tokenizeActionBlock("@ 移动"), BehaviorError)
  assert.deepEqual(tokenizeStatement("move"), { op: "move", arg: null })
  assert.deepEqual(tokenizeStatement("mode 1"), { op: "mode", arg: "1" })
  assert.deepEqual(tokenizeStatement("seterrors 组合键深度超过 3,已拒绝"), {
    op: "seterrors",
    arg: "组合键深度超过 3,已拒绝",
  })
  assert.equal(tokenizeStatement("(* 注释 *)"), null)
})

test("behavior: parseProgram 规则段+行为段并存", () => {
  const src = `
0.-.UP>移动焦点
@移动焦点
  move
0.-.ESC>退出
@退出
  exit
`
  const prims = new Set([...PRIM_NAMES, ...BUILTIN_OPS])
  const { rules, actions } = parseProgram(src, new Set(["sel"]), prims)
  assert.equal(rules.length, 2)
  assert.equal(rules[0].action, "移动焦点")
  assert.deepEqual([...actions.keys()], ["移动焦点", "退出"])
  assert.deepEqual(actions.get("移动焦点"), [{ op: "move", arg: null }])
})

test("behavior: parseProgram 未知原语报错 (静态检查)", () => {
  assert.throws(
    () => parseProgram("@移动焦点\n  moveX\n", new Set(), new Set()),
    /unknown statement op 'moveX'/,
  )
})

test("behavior: checkBehaviorProgram 规则必须引用已声明动作", () => {
  assert.throws(
    () => checkBehaviorProgram("0.-.UP>不存在的动作\n@别的\n  move\n", new Set(), new Set([...PRIM_NAMES])),
    /无行为段定义/,
  )
})

test("behavior: parseRules 跳过行为段 (仅取规则)", () => {
  const src = `
0.-.UP>移动焦点
@移动焦点
  move
0.-.DOWN>移动焦点
`
  const rules = parseRules(src)
  assert.equal(rules.length, 2)
})

// ── 解释器: 行为段驱动的 dispatch ────────────────────────────────────

test("behavior: dispatch 执行声明式动作 (行为段)", () => {
  const s = init("A B;")
  const { state } = dispatch(s, "right") // 移动焦点 = move
  assert.deepEqual(state.cursor, { r: 0, c: 1 })
})

test("behavior: 组合行为 (sel → swap → 生效)", () => {
  const s = init("A B;")
  const s1 = dispatch(s, "space").state
  const s2 = dispatch(s1, "right").state
  const { state, effects } = dispatch(s2, "space")
  assert.equal(text(state.rows), "B A;")
  assert.equal(state.selected, null)
  assert.equal(effects.length, 1)
})

test("behavior: mode 内置操作跳转模式", () => {
  const s = init("A;")
  assert.equal(dispatch(s, "alt").state.mode, "1")
  assert.equal(dispatch(dispatch(s, "alt").state, "alt").state.mode, "2")
})

// ── 原语扩展 (registerPrim) ─────────────────────────────────────────

test("behavior: registerPrim 扩展行为词汇表", () => {
  const off = registerPrim("audit", (s) => ({ state: { ...s, errors: ["审计"] }, effects: [] }))
  const offAction = registerAction("审计动作", (s) => ({ state: { ...s, errors: ["JS 兜底"] }, effects: [] }))
  try {
    assert.ok(listPrims().includes("audit"))
    // 内置原语 (13) + 新注册 = 14
    assert.equal(listPrims().length, 14)
  } finally {
    off()
    offAction()
  }
})
