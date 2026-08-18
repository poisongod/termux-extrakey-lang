// Worker 协议 (线程侧) — 子进程插件的唯一入口。
// 用法: worker.mjs 里
//   import { serve } from "../kernel/worker.mjs"
//   serve({ methodName: handler, ... })
// 传输由 workerData 决定:
//   - __kernelMailbox (SharedArrayBuffer) → Atomics 邮箱 (零拷贝)
//   - 否则 → parentPort 结构化克隆 (降级)
// 协议与 transport.mjs 对称: 请求 [method, ...args], 应答 {result|error}。

import { parentPort, workerData } from "node:worker_threads"
import { SharedMailbox } from "./shared.mjs"

// 线程入口: 自动选择传输。返回 Promise (在线程生命周期内运行)。
export function serve(handlers) {
  const sab = workerData?.__kernelMailbox
  if (sab instanceof SharedArrayBuffer) return serveShared(handlers, sab)
  return servePort(handlers)
}

// SAB 邮箱: 零拷贝, worker 阻塞轮询 (Atomics.wait)
export function serveShared(handlers, sab) {
  const mailbox = new SharedMailbox(sab)
  return new Promise((resolve) => {
    // 热循环: 单态方法表, 避免每次查找抖 shape
    const lookup = handlers
    const loop = async () => {
      for (;;) {
        mailbox.waitRequest()
        const { flags: f, method, args } = mailbox.takeRequest()
        if (f & 1 /* FLAG_SHUTDOWN */) {
          resolve()
          return
        }
        const fn = lookup[method]
        if (typeof fn !== "function") {
          mailbox.putResponse(`unknown method '${method}'`, true)
          continue
        }
        try {
          const result = await fn(...args)
          mailbox.putResponse(result, false)
        } catch (err) {
          mailbox.putResponse(err, true)
        }
      }
    }
    void loop()
  })
}

// MessagePort 结构化克隆 (降级)
export function servePort(handlers) {
  if (!parentPort) throw new Error("serve 需要 worker_threads (或传 __kernelMailbox)")
  parentPort.on("message", async (msg) => {
    if (!msg || typeof msg !== "object") return
    const { id, method, args = [] } = msg
    const fn = handlers[method]
    if (typeof fn !== "function") {
      parentPort.postMessage({ id, method, error: `unknown method '${method}'` })
      return
    }
    try {
      const result = await fn(...args)
      parentPort.postMessage({ id, method, result })
    } catch (err) {
      parentPort.postMessage({ id, method, error: err instanceof Error ? err.message : String(err) })
    }
  })
}

// 兼容旧 child_process 用法 (进程通信, 一般不再需要)
export function serveProcess(handlers) {
  process.on("message", async (msg) => {
    if (!msg || typeof msg !== "object") return
    if (msg.shutdown) {
      process.exit(0)
      return
    }
    const { id, method, args = [] } = msg
    const fn = handlers[method]
    if (typeof fn !== "function") {
      process.send({ id, method, error: `unknown method '${method}'` })
      return
    }
    try {
      const result = await fn(...args)
      process.send({ id, method, result })
    } catch (err) {
      process.send({ id, method, error: err instanceof Error ? err.message : String(err) })
    }
  })
}