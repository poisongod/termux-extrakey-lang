// 子进程 RPC 传输 (父进程侧) — 内核 ↔ worker 的并行计算通道。
// 用 Worker Threads (同进程 V8, 共享 SAB) 而非 child_process:
//   - 并行计算: 每个插件一个独立线程, 编译/校验不阻塞主线程事件循环
//   - 零拷贝: SharedArrayBuffer + Atomics 邮箱, 负载直接写共享内存
//   - 若环境不支持 SharedArrayBuffer, 自动降级到 MessagePort 结构化克隆
// 纯 Node: worker 也是 .mjs, 不引入第二编程语言。

import { Worker } from "node:worker_threads"
import { SharedMailbox, createSharedBuffer } from "./shared.mjs"

export class TransportError extends Error {
  constructor(message) {
    super(`transport: ${message}`)
  }
}

const HAS_SAB = typeof SharedArrayBuffer !== "undefined" && typeof Atomics?.waitAsync === "function"

// 启动 worker 线程 (一个 worker = 一个插件线程)。
// 返回 { call, dispose } — call 是 async (线程间通信天生异步)。
// 传输: SAB 邮箱 (零拷贝) 优先; 不支持时降级 MessagePort (structured clone)。
export function spawnWorker(workerFile) {
  if (HAS_SAB) return spawnShared(workerFile)
  return spawnPort(workerFile)
}

// ── SAB 邮箱传输 (零拷贝) ─────────────────────────────────────────────

function spawnShared(workerFile) {
  const mailbox = SharedMailbox.create()
  const worker = new Worker(workerFile, {
    // SAB 天然共享, 无需 transferList (放入即双方可读写同一块内存)
    workerData: { __kernelMailbox: mailbox.sab },
  })
  let closed = false
  worker.on("error", (err) => {
    // worker 异常退出, 未决请求会挂在 nextResponse 上; 主动释放
    closed = true
  })
  worker.on("exit", () => {
    closed = true
  })

  // call 排队: SAB 邮箱同一时刻只承载一个请求
  let chain = Promise.resolve()
  function call(method, ...args) {
    const run = chain.then(async () => {
      if (closed) throw new TransportError(`worker 已退出 (${workerFile})`)
      await mailbox.putRequest(method, args)
      return mailbox.nextResponse()
    })
    chain = run.catch(() => {})
    return run
  }

  function dispose() {
    try {
      mailbox.requestShutdown()
    } catch {
      /* ignore */
    }
    setTimeout(() => worker.terminate(), 20).unref()
  }

  return { call, dispose, mailbox, worker, transport: "shared" }
}

// ── MessagePort 结构化克隆传输 (降级) ─────────────────────────────────

function spawnPort(workerFile) {
  const worker = new Worker(workerFile, {
    workerData: { __kernelMailbox: null },
  })
  const pending = new Map()
  let seq = 0

  worker.on("message", (msg) => {
    const p = pending.get(msg?.id)
    if (!p) return
    pending.delete(msg.id)
    if (msg.error) p.reject(new TransportError(msg.error))
    else p.resolve(msg.result)
  })
  worker.on("error", (err) => failAll(new TransportError(`worker 崩溃: ${err.message}`)))
  worker.on("exit", (code) => failAll(new TransportError(`worker 已退出 (code=${code})`)))

  function failAll(err) {
    for (const p of pending.values()) p.reject(err)
    pending.clear()
  }

  function call(method, ...args) {
    return new Promise((resolve, reject) => {
      const id = ++seq
      pending.set(id, { resolve, reject })
      worker.postMessage({ id, method, args })
    })
  }

  function dispose() {
    setTimeout(() => worker.terminate(), 20).unref()
  }

  return { call, dispose, worker, transport: "port" }
}

export function supportsShared() {
  return HAS_SAB
}

export { createSharedBuffer }