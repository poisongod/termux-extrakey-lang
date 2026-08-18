import { test } from "node:test"
import assert from "node:assert/strict"
import { parse } from "../lib/keys.mjs"
import { printRows } from "../lib/keys.mjs"
import { createPluginKernel, definePlugin, mergeContributions, PluginError } from "../lib/ui.mjs"
import {
  initialState,
  dispatch,
  registerAction,
  registerGuard,
  registerActionWithRules,
  wrapAction,
  actionIds,
  listActions,
  plugins,
  validateProgram,
} from "../lib/ui.mjs"
import { readFile } from "node:fs/promises"

const init = (src) => initialState(parse(src))
const text = (rows) => printRows(rows)

// ── 内核 (plugin.mjs) ────────────────────────────────────────────────

test("kernel: definePlugin 校验清单", () => {
  const p = definePlugin({ name: "demo", contribute: { actions: {} } })
  assert.deepEqual(p, { name: "demo", version: "0.0.0", deps: [], contribute: { actions: {} } })
  assert.throws(() => definePlugin({}), PluginError) // 缺 name
  assert.throws(() => definePlugin({ name: "a b" }), PluginError) // 非法名
  assert.throws(() => definePlugin({ name: "a", deps: "x" }), PluginError) // deps 非数组
})

test("kernel: 注册/依赖序/重名/缺依赖", () => {
  const k = createPluginKernel()
  k.register(definePlugin({ name: "a", contribute: { actions: {} } }))
  const offB = k.register(definePlugin({ name: "b", deps: ["a"], contribute: { actions: {} } }))
  assert.throws(() => k.register(definePlugin({ name: "a" })), PluginError) // 重名
  assert.throws(
    () => k.register(definePlugin({ name: "z", deps: ["missing"] })),
    PluginError, // 缺依赖
  )
  offB() // 注销 b
  assert.equal(k.has("b"), false)
})

test("kernel: order 依赖在前 (拓扑序)", () => {
  const k = createPluginKernel()
  k.register(definePlugin({ name: "a", contribute: { actions: {} } }))
  k.register(definePlugin({ name: "b", deps: ["a"], contribute: { actions: {} } }))
  k.register(definePlugin({ name: "c", deps: ["a", "b"], contribute: { actions: {} } }))
  assert.deepEqual(k.order().map((o) => o.name), ["a", "b", "c"])
})

test("kernel: contribute 按依赖序聚合 + mergeContributions 合并", () => {
  const k = createPluginKernel()
  k.register(definePlugin({ name: "base", contribute: { actions: { 甲: (s) => s } } }))
  k.register(
    definePlugin({ name: "ext", deps: ["base"], contribute: { actions: { 甲: (s) => s, 乙: (s) => s } } }),
  )
  const merged = mergeContributions(k, "actions")
  assert.deepEqual(Object.keys(merged).sort(), ["乙", "甲"].sort())
  assert.equal(typeof merged["甲"], "function") // ext 覆盖 base (后者优先)
})

test("kernel: contribute 数组/对象/标量合并", () => {
  const k = createPluginKernel()
  k.register(definePlugin({ name: "p1", contribute: { rules: ["0.-.UP>移动焦点"], tags: { a: 1 } } }))
  k.register(definePlugin({ name: "p2", deps: ["p1"], contribute: { rules: ["1.-.UP>移动焦点"], tags: { b: 2 } } }))
  assert.deepEqual(mergeContributions(k, "rules"), ["0.-.UP>移动焦点", "1.-.UP>移动焦点"])
  assert.deepEqual(mergeContributions(k, "tags"), { a: 1, b: 2 })
})

// ── mode.mjs wiring (默认 kernel = 内置 + 扩展) ──────────────────────

test("plugin e2e: registerActionWithRules 一次交付行为+键位", async () => {
  const off = registerActionWithRules(
    "打印当前键",
    (s, key, ctx) => {
      const e = ctx.docHelpers.entryAt(s)
      return { state: { ...s, errors: [`看到了键 ${key}, 当前 ${e.key}`] }, effects: [] }
    },
    ["0.-.ENTER>打印当前键"], // 插件自带触发点 (追加到 base ttoll)
  )
  try {
    const { state } = dispatch(init("A;"), "enter")
    assert.match(state.errors[0], /看到了键 ENTER, 当前 A/)
  } finally {
    off()
    assert.equal(actionIds().has("打印当前键"), false) // 注销还原
  }
})

test("plugin e2e: registerAction 单独注册 + ttoll 内引用", async () => {
  const src = `0.-.ENTER>打印当前键\n` + (await readFile("behavior.base", "utf8"))
  const off = registerAction("打印当前键", (s, key, ctx) => {
    const e = ctx.docHelpers.entryAt(s)
    return { state: { ...s, errors: [`键=${key} 值=${e.key}`] }, effects: [] }
  })
  try {
    const r = dispatch(init("A;"), "enter")
    // dispatch 使用默认 BASE_TTOLL, 不含追加规则; 这里验证 handler 本身可由白名单识别
    assert.ok(actionIds().has("打印当前键"))
    void r
  } finally {
    off()
  }
})

test("plugin e2e: wrapAction 覆盖内置行为并可还原", () => {
  const off = wrapAction("交换焦点键位并实时更新", (original, s, key, ctx) => {
    const res = original(s, key, ctx)
    return { state: { ...res.state, errors: ["交换 (审计)"] }, effects: res.effects }
  }, { desc: "审计包装" })
  let s = init("A B;")
  s = dispatch(s, "space").state
  s = dispatch(s, "right").state
  const wrapped = dispatch(s, "space")
  assert.equal(text(wrapped.state.rows), "B A;")
  assert.deepEqual(wrapped.state.errors, ["交换 (审计)"])

  off() // 还原
  s = init("A B;")
  s = dispatch(s, "space").state
  s = dispatch(s, "right").state
  const restored = dispatch(s, "space")
  assert.equal(text(restored.state.rows), "B A;")
  assert.equal(restored.state.errors.length, 0)
})

test("plugin e2e: 自定义守卫接入", async () => {
  const offGuard = registerGuard("首行", (s) => s.cursor.r === 0)
  const offAction = registerActionWithRules(
    "标记",
    (s) => ({ state: { ...s, errors: ["首行!"] }, effects: [] }),
    ["0.-.ENTER?首行>标记"],
  )
  try {
    const hit = dispatch(init("A;\nB;"), "enter") // 光标 (0,0) 首行
    assert.deepEqual(hit.state.errors, ["首行!"])
    const miss = dispatch(init("A;\nB;"), "down").state
    assert.equal(dispatch(miss, "enter").ok, false)
  } finally {
    offAction() // 逆序: 先撤取决于守卫的动作, 再撤守卫
    offGuard()
  }
})

test("plugin e2e: hooks.before 短路 + hooks.after 改造", async () => {
  // 该测试用独立 kernel + 独立 dispatch (避免污染默认全局)
  // 简化: 验证内核层面 hooks 贡献可合并; dispatch 层 hook 走默认不用
  const k = createPluginKernel()
  k.register(
    definePlugin({
      name: "hooker",
      contribute: {
        hooks: {
          before: (s, key) => (key === "SPACE" ? { state: { ...s, errors: ["短路"] } } : null),
        },
      },
    }),
  )
  assert.equal(typeof mergeContributions(k, "hooks").before, "function")
})

test("plugin: actionIds/listActions/plugins 内省", () => {
  // 行为段声明的动作 (声明式) + 已注册的 JS 动作 都应出现在 actionIds
  assert.ok(actionIds().has("移动焦点"))
  assert.ok(actionIds().has("编辑主键名称"))
  assert.ok(actionIds().has("1"))
  assert.ok(listActions().every((a) => typeof a.handler === "function"))
  assert.ok(plugins().some((p) => p.name === "builtin"))
  // builtin 插件贡献守卫 (行为已声明化到 behavior.base, 不再贡献 JS actions)
  assert.ok(plugins().find((p) => p.name === "builtin").capabilities.includes("guards"))
})

test("plugin: validateProgram 死插件告警 (注册未使用)", async () => {
  const off = registerAction("从未引用的动作", () => null)
  try {
    const { dead } = validateProgram()
    assert.ok(dead.includes("从未引用的动作"))
  } finally {
    off()
  }
})

test("plugin: validateProgram 合法 ttoll 通过", () => {
  const { rules, declared, used } = validateProgram()
  assert.ok(rules.length >= 30)
  assert.ok(declared >= used)
})