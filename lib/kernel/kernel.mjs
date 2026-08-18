// 插件内核 (稳定核心) — 一切皆插件的唯一主机。
// 内核不感知任何业务: 不知道 actions/tekl/tui/render 为何物。它只做:
//   1. 托管插件单元 (声明/生命周期/依赖拓扑/审计)
//   2. 贡献点聚合 (capability 字符串 → 值, 内核不知其义)
//   3. 子进程路由 (worker 经 transport.mjs IPC, 内核只转发方法名+参数)
// 纯 Node: 子进程插件也是 Node.js (ESM .mjs), 不引入第二编程语言。
// 不 import opentui, 可 node --test。

import { spawnWorker } from "./transport.mjs"

export class KernelError extends Error {
  constructor(message) {
    super(`kernel: ${message}`)
  }
}

// 插件名 (包名空间): 字母/数字/下划线/连字符/点, 可含中文; 非空
export const NAME_RE = /^[\p{L}\p{N}_.\-]+$/u

// 插件单元 — 纯声明:
//   { name, version?, deps?: string[],
//     capabilities?: { [能力]: 值 },          // 进程内贡献
//     worker?: string,                        // 子进程插件: worker 脚本路径
//     activate?: async (ctx) => void }        // 生命周期: boot 时按依赖序调用
// 进程内 + 子进程可共存: capabilities 做进程内贡献, worker 暴露方法。
export function definePlugin(def) {
  if (!def || typeof def !== "object") throw new KernelError("插件必须是对象")
  const { name, version = "0.0.0", deps = [], capabilities = {}, worker = null, activate = null } = def
  if (!NAME_RE.test(name ?? "")) throw new KernelError(`非法插件名 '${name}'`)
  if (!Array.isArray(deps)) throw new KernelError(`插件 '${name}': deps 必须是数组`)
  if (!capabilities || typeof capabilities !== "object") {
    throw new KernelError(`插件 '${name}': capabilities 必须是对象`)
  }
  if (worker !== null && typeof worker !== "string") {
    throw new KernelError(`插件 '${name}': worker 必须是脚本路径字符串`)
  }
  if (activate !== null && typeof activate !== "function") {
    throw new KernelError(`插件 '${name}': activate 必须是函数`)
  }
  return { name, version, deps, capabilities, worker, activate }
}

export class Kernel {
  #records = new Map() // name → { def, order }
  #procs = new Map() // name → spawnWorker 结果
  #order = null // 拓扑序缓存
  #counter = 0

  register(def) {
    const p = definePlugin(def)
    if (this.#records.has(p.name)) throw new KernelError(`插件重名: '${p.name}'`)
    for (const d of p.deps) {
      if (!this.#records.has(d)) throw new KernelError(`依赖缺失: '${d}' (需先注册)`)
    }
    this.#records.set(p.name, { def: p, order: ++this.#counter })
    this.#topo() // 触发环检测
    return () => this.unregister(p.name)
  }

  has(name) {
    return this.#records.has(name)
  }

  names() {
    return new Set(this.#records.keys())
  }

  unregister(name) {
    if (!this.#records.delete(name)) return false
    this.#topo()
    return true
  }

  // 依赖序 (activate 顺序, 依赖在前)。Kahn 拓扑 + 环检测。
  #topo() {
    const deps = new Map()
    for (const [name, { def }] of this.#records) deps.set(name, new Set(def.deps))
    const ready = [...deps].filter(([, d]) => d.size === 0).map(([n]) => n)
    const order = []
    while (ready.length) {
      const n = ready.shift()
      order.push(n)
      for (const [m, d] of deps) {
        if (d.delete(n) && d.size === 0) ready.push(m)
      }
    }
    if (order.length !== this.#records.size) {
      const cycle = [...deps].filter(([, d]) => d.size > 0).map(([n]) => n)
      throw new KernelError(`插件依赖成环: ${cycle.join(", ")}`)
    }
    this.#order = order
    return order
  }

  order() {
    if (!this.#order) this.#topo()
    return this.#order.map((name) => ({ name, def: this.#records.get(name).def }))
  }

  // 按注册序列出 (审计用)
  describe() {
    return this.order().map(({ name, def }) => ({
      name,
      version: def.version,
      deps: def.deps,
      capabilities: Object.keys(def.capabilities),
      worker: def.worker ?? null,
    }))
  }

  // ── 启动生命周期 ─────────────────────────────────────────────────────

  // boot: 按依赖序调用各插件的 activate(ctx)。ctx 提供调用其它插件的能力。
  // activate 可返回动态贡献对象, 记入该插件名下 (动态优先于静态贡献)。
  async boot() {
    let ctx = null
    for (const { name } of this.order()) {
      const { def } = this.#records.get(name)
      if (!def.activate) continue
      if (!ctx) ctx = this.#ctx()
      const extra = await def.activate(ctx)
      if (extra && typeof extra === "object") {
        this.#dyn.set(name, { ...(this.#dyn.get(name) ?? {}), ...extra })
      }
    }
    return this
  }

  // 上下文: 插件在 activate 中经它调用/解析其它插件
  #ctx() {
    return {
      kernel: this,
      call: (name, method, ...args) => this.call(name, method, ...args),
      resolve: (capability) => this.resolve(capability),
      contribute: (capability) => this.contribute(capability),
    }
  }

  // ── 贡献点 ──────────────────────────────────────────────────────────

  #dyn = new Map() // name → activate 返回的动态贡献

  // 单插件的合并面 (静态 + 动态, 动态优先)
  #surface(name) {
    const rec = this.#records.get(name)
    if (!rec) return {}
    return { ...rec.def.capabilities, ...(this.#dyn.get(name) ?? {}) }
  }

  // 聚合某 capability (依赖序): 数组拼接 / 对象合并 / 否则取最后一个。
  // 静态 (capabilities) + 动态 (activate 返回) 合并, 动态优先。
  contribute(capability) {
    let out
    const merge = (value) => {
      if (Array.isArray(value)) out = [...(out ?? []), ...value]
      else if (value && typeof value === "object") out = { ...(out ?? {}), ...value }
      else out = value
    }
    for (const { name } of this.order()) {
      for (const kind of ["static", "dynamic"]) {
        const cap = kind === "static" ? this.#records.get(name).def.capabilities : this.#dyn.get(name)
        const value = cap?.[capability]
        if (value !== undefined) merge(value)
      }
    }
    return out
  }

  // 解析某 capability (最后一个贡献者; 动态优先于静态)
  resolve(capability) {
    let out
    for (const { name } of this.order()) {
      const value = this.#surface(name)[capability]
      if (value !== undefined) out = value
    }
    return out
  }

  // ── 调用 ────────────────────────────────────────────────────────────

  // 调用插件能力:
  //   - worker 插件: client.call(method, ...args) (线程间, async)
  //   - 进程内插件: 合并面 (动态+静态) 上的方法函数直接调用
  // 统一返回 Promise。
  async call(name, method, ...args) {
    const rec = this.#records.get(name)
    if (!rec) throw new KernelError(`未知插件 '${name}'`)
    const { def } = rec
    if (def.worker) {
      const client = await this.#worker(name, def.worker)
      return client.call(method, ...args)
    }
    const fn = this.#surface(name)[method]
    if (typeof fn !== "function") {
      throw new KernelError(`插件 '${name}' 无方法 '${method}'`)
    }
    return fn(...args)
  }

  async #worker(name, file) {
    let client = this.#procs.get(name)
    if (!client) {
      client = spawnWorker(file)
      this.#procs.set(name, client)
    }
    return client
  }

  // 停止: 结束所有子进程插件
  async dispose() {
    for (const [name, client] of this.#procs) {
      client.dispose()
      this.#procs.delete(name)
    }
  }
}

export function createKernel() {
  return new Kernel()
}