import { test } from "node:test"
import assert from "node:assert/strict"
import { mkdirSync, writeFileSync, rmSync, existsSync } from "node:fs"
import { join } from "node:path"
import { discoverPlugins, loadPlugins, loadPluginModule, loadEntryModule } from "../lib/ui.mjs"
import { PluginError } from "../lib/ui.mjs"

const TMP = "/data/data/com.termux/files/usr/tmp/opencode/plugin-loader-test"
const p = (n) => join(TMP, n)

test("loader: 只发现 *.plugin.mjs", () => {
  mkdirSync(TMP, { recursive: true })
  writeFileSync(p("a.plugin.mjs"), "export default { name: 'a' }\n")
  writeFileSync(p("b.plugin.mjs"), "export default { name: 'b' }\n")
  writeFileSync(p("c.mjs"), "export default { name: 'c' }\n")
  assert.deepEqual(discoverPlugins(TMP).map((f) => f.split("/").pop()).sort(), [
    "a.plugin.mjs",
    "b.plugin.mjs",
  ])
})

test("loader: 缺 default 导出报错", async () => {
  writeFileSync(p("bad.plugin.mjs"), "export const x = 1\n")
  await assert.rejects(() => loadPluginModule(p("bad.plugin.mjs")), PluginError)
})

test("loader: loadPlugins 装配并注册 (含注销)", async () => {
  rmSync(TMP, { recursive: true, force: true }) // 隔离: 清掉上个测试的坏文件
  mkdirSync(TMP, { recursive: true })
  writeFileSync(
    p("one.plugin.mjs"),
    `import { definePlugin } from ${JSON.stringify(new URL("../lib/ui.mjs", import.meta.url).href)}\n` +
      `export default definePlugin({ name: "ext-one", contribute: { actions: { 一号: () => null } } })\n`,
  )
  const registered = []
  const loaded = await loadPlugins(TMP, { register: (def) => (registered.push(def.name), () => {}) })
  assert.deepEqual(loaded.map((l) => l.def.name), ["ext-one"])
  assert.deepEqual(registered, ["ext-one"])
})

test("loader: 入口 init.mjs 装配单声明 (含注销)", async () => {
  rmSync(TMP, { recursive: true, force: true })
  mkdirSync(TMP, { recursive: true })
  writeFileSync(
    p("init-one.mjs"),
    `import { definePlugin } from ${JSON.stringify(new URL("../lib/ui.mjs", import.meta.url).href)}\n` +
      `export default definePlugin({ name: "entry-one", contribute: { guards: { 入口守卫: () => true } } })\n`,
  )
  const registered = []
  const loaded = await loadPlugins(TMP, { entry: "init-one.mjs", register: (def) => (registered.push(def.name), () => {}) })
  assert.deepEqual(registered, ["entry-one"])
  assert.equal(loaded[0].def.contribute.guards["入口守卫"](), true) // 函数保持原样 (未被包装)
})

test("loader: 入口 init.mjs 支持声明数组", async () => {
  rmSync(TMP, { recursive: true, force: true })
  mkdirSync(TMP, { recursive: true })
  writeFileSync(
    p("init-two.mjs"),
    `import { definePlugin } from ${JSON.stringify(new URL("../lib/ui.mjs", import.meta.url).href)}\n` +
      `export default [definePlugin({ name: "a1" }), definePlugin({ name: "b2" })]\n`,
  )
  const registered = []
  await loadPlugins(TMP, { entry: "init-two.mjs", register: (def) => (registered.push(def.name), () => {}) })
  assert.deepEqual(registered, ["a1", "b2"])
})

test("loader: 入口 init.mjs 与 *.plugin.mjs 并存 (入口先行)", async () => {
  rmSync(TMP, { recursive: true, force: true })
  mkdirSync(TMP, { recursive: true })
  writeFileSync(
    p("init-three.mjs"),
    `import { definePlugin } from ${JSON.stringify(new URL("../lib/ui.mjs", import.meta.url).href)}\n` +
      `export default definePlugin({ name: "ent" })\n`,
  )
  writeFileSync(
    p("z.plugin.mjs"),
    `import { definePlugin } from ${JSON.stringify(new URL("../lib/ui.mjs", import.meta.url).href)}\n` +
      `export default definePlugin({ name: "zplug" })\n`,
  )
  const order = []
  await loadPlugins(TMP, { entry: "init-three.mjs", register: (def) => (order.push(def.name), () => {}) })
  assert.deepEqual(order, ["ent", "zplug"])
})

test("loader: loadEntryModule 缺 default 报错", async () => {
  rmSync(TMP, { recursive: true, force: true })
  mkdirSync(TMP, { recursive: true })
  writeFileSync(p("init-missing.mjs"), `export const x = 1\n`)
  await assert.rejects(() => loadEntryModule(p("init-missing.mjs")), PluginError)
})

test("loader: 目录不存在时静默返回空", async () => {
  const loaded = await loadPlugins("/nonexistent-dir-xyz")
  assert.deepEqual(loaded, [])
  assert.equal(existsSync("/nonexistent-dir-xyz/init.mjs"), false)
})

test("loader: 包络后 rm 临时目录", () => {
  rmSync(TMP, { recursive: true, force: true })
})