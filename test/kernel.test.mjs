// lib/kernel: 插件内核 + 共享邮箱 + 线程传输。
// 只测纯数据层 + 真实线程 (tekl/tjek worker 经 SAB 邮箱 round-trip)。

import { test } from "node:test"
import assert from "node:assert/strict"
import { createKernel, definePlugin, KernelError } from "../lib/kernel/kernel.mjs"
import { SharedMailbox } from "../lib/kernel/shared.mjs"
import { spawnWorker } from "../lib/kernel/transport.mjs"
import { keysPlugin } from "../lib/keys.mjs"
import { checkPlugin } from "../lib/check.mjs"

test("kernel: definePlugin 校验清单", () => {
  assert.throws(() => definePlugin({}), /非法插件名/)
  assert.throws(() => definePlugin({ name: "a", deps: "x" }), /deps/)
  assert.throws(() => definePlugin({ name: "a", capabilities: null }), /capabilities/)
  assert.throws(() => definePlugin({ name: "a", worker: 42 }), /worker/)
  assert.throws(() => definePlugin({ name: "a", activate: "no" }), /activate/)
  const p = definePlugin({ name: "x.y", deps: [], capabilities: { k: 1 } })
  assert.equal(p.version, "0.0.0")
})

test("kernel: 注册/依赖序/重名/缺依赖/注销", () => {
  const k = createKernel()
  k.register({ name: "a", capabilities: { v: 1 } })
  const offB = k.register({ name: "b", deps: ["a"], capabilities: { v: 2 } })
  assert.deepEqual([...k.names()].sort(), ["a", "b"])
  assert.deepEqual(k.order().map((p) => p.name), ["a", "b"])
  assert.throws(() => k.register({ name: "a" }), /重名/)
  assert.throws(() => k.register({ name: "c", deps: ["nope"] }), /依赖缺失/)
  offB()
  assert.equal(k.has("b"), false)
})

test("kernel: 依赖成环/悬垂检测 (unregister 破坏依赖序)", () => {
  const k = createKernel()
  k.register({ name: "a" })
  k.register({ name: "b", deps: ["a"] })
  // 注销被依赖方 → 拓扑校验失败
  assert.throws(() => k.unregister("a"), /成环|依赖/)
})

test("kernel: contribute/resolve 数组拼接 + 对象合并 + 动态覆盖静态", async () => {
  const k = createKernel()
  k.register({ name: "a", capabilities: { list: [1], map: { a: 1 }, scalar: "a" } })
  k.register({
    name: "b",
    deps: ["a"],
    capabilities: { list: [2], map: { b: 2 }, scalar: "b" },
    activate: () => ({ scalar: "dyn" }),
  })
  await k.boot()
  assert.deepEqual(k.contribute("list"), [1, 2])
  assert.deepEqual(k.contribute("map"), { a: 1, b: 2 })
  assert.equal(k.resolve("scalar"), "dyn")
})

test("kernel: call 进程内方法 + 未知方法报错", async () => {
  const k = createKernel()
  k.register({ name: "svc", capabilities: { add: (a, b) => a + b } })
  assert.equal(await k.call("svc", "add", 1, 2), 3)
  await assert.rejects(() => k.call("svc", "nope"), /无方法/)
  await assert.rejects(() => k.call("ghost", "x"), /未知插件/)
})

// ── SharedMailbox (纯邮箱, 主线程用 waitAsync 路径) ───────────────────

test("shared-mailbox: 请求/应答 round-trip + 复用 + 错误", async () => {
  const mb = SharedMailbox.create()
  const responder = new SharedMailbox(mb.sab)

  // 模拟 worker: 手动驱动 (主线程不能用 Atomics.wait)
  let done = Promise.resolve()
  const drive = () => {
    let resolveDone
    done = new Promise((r) => (resolveDone = r))
    ;(async () => {
      for (;;) {
        await waitUntil(responder, responder.RQ.state, 1)
        const { flags, method, args } = responder.takeRequest()
        if (flags & 1) break
        if (method === "fail") responder.putResponse("boom", true)
        else responder.putResponse(method + args.join(""))
      }
      resolveDone()
    })()
    return done
  }
  drive()

  assert.equal(await mb.request("echo", "hello"), "echohello")
  assert.equal(await mb.request("echo", "world"), "echoworld")
  await assert.rejects(() => mb.request("fail"), /boom/)
  mb.requestShutdown()
  await done
})

function waitUntil(mb, index, value) {
  const opposite = value === 0 ? 1 : 0
  return new Promise((resolve) => {
    const tick = () => {
      if (Atomics.load(mb.ctrl, index) === value) return resolve()
      const w = Atomics.waitAsync(mb.ctrl, index, opposite)
      if (w.async) w.value.then(tick)
      else tick()
    }
    tick()
  })
}

// ── 线程传输: tekl/tjek 经 SAB 邮箱真实 round-trip ────────────────────

test("transport: spawnWorker SAB 邮箱调用 tekl 编译", async () => {
  const client = spawnWorker(keysPlugin.worker)
  const src = "ESC TAB;\nCTRL(C) HOME:Start;"
  const { json, warnings } = await client.call("compileSource", src)
  assert.ok(Array.isArray(json))
  assert.ok(json.length >= 2)
  assert.ok(Array.isArray(warnings))
  client.dispose()
})

test("transport: tjek 校验通过/失败经邮箱返回错误", async () => {
  const client = spawnWorker(checkPlugin.worker)
  const ok = await client.call("validate", "[[{key:ESC}]]")
  assert.deepEqual(ok, [[{ key: "ESC" }]])
  await assert.rejects(() => client.call("validate", "[[{key:ESC}], [oops:1]]"), /expected|must|invalid|tjek/i)
  client.dispose()
})

// ── 全装配: 插件经内核调用 tekl (线程) ────────────────────────────────

test("kernel: tekl 插件经 call 编译 + dispose 优雅停止", async () => {
  const k = createKernel()
  k.register(keysPlugin)
  k.register(checkPlugin)
  await k.boot()
  const { json } = await k.call("keys", "compileSource", "ESC;")
  assert.ok(Array.isArray(json))
  await k.dispose()
})