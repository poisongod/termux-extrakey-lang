// 共享内存邮箱 (SharedArrayBuffer + Atomics) — 主线程 ↔ worker 的零拷贝通道。
// 核心求稳: 本模块不加载任何业务, 只有纯邮箱语义, 可 node --test (主线程测 waitAsync 路径)。
//
// 布局 (两个单向区, 各带控制槽, 避免读写重叠):
//   ctrl:  Int32Array[8]  (均按 4 字节对齐, 供 Atomics)
//     [0] req.state   0=空闲 1=请求就绪 (响应方消费后置回 0)
//     [1] req.flags   bit0=shutdown
//     [2] req.len     请求负载字节数
//     [3] req.seq     请求序号 (回显用)
//     [4] resp.state  0=空闲 1=应答就绪
//     [5] resp.status 0=成功 1=失败 (payload 为错误消息)
//     [6] resp.len    应答负载字节数
//     [7] resp.seq    回显请求序号
//   req 区:   负载 = JSON([method, ...args]) 的 UTF-8 字节
//   resp 区:  负载 = JSON(result) 或 JSON(err.message)
//
// 一方发请求 (主线程, Atomics.waitAsync), 一方阻塞等待 (worker, Atomics.wait)。
// 负载直接写入共享缓冲区, 无 postMessage 结构化克隆 — 零拷贝。
//
// 状态机 (request/response 各自单向):
//   请求方: 等 req.state==0 (空闲) → 写负载 → 置 req.state=1 → notify
//   响应方: 等 req.state==1 (就绪) → 读负载 → 置 req.state=0 (允许复用)
//           等 resp.state==0 (应答区空闲) → 写应答 → 置 resp.state=1 → notify
//   请求方: 等 resp.state==1 → 读应答 → 置 resp.state=0 (允许复用)

export class SharedMailboxError extends Error {
  constructor(message) {
    super(`shared-mailbox: ${message}`)
  }
}

export const FLAG_SHUTDOWN = 1
export const DEFAULT_CAP = 1 << 20 // 每方向 1 MiB, extra-keys 负载远小于此

const CTRL = 8 // 控制槽数量
const HEADER_BYTES = CTRL * 4

const encoder = new TextEncoder()
const decoder = new TextDecoder()

// ── 等待原语: 等到 ctrl[index] === value ──────────────────────────────

// 主线程版 (worker 线程内也可用, 不抛错)
async function waitAsyncUntil(ctrl, index, value) {
  const opposite = value === 0 ? 1 : 0
  return new Promise((resolve) => {
    const tick = () => {
      if (Atomics.load(ctrl, index) === value) return resolve()
      const w = Atomics.waitAsync(ctrl, index, opposite)
      if (w.async) w.value.then(tick)
      else tick()
    }
    tick()
  })
}

// 阻塞版 (worker 线程专用, 主线程会抛 "Atomics.wait cannot be called in this context")
function waitSyncUntil(ctrl, index, value) {
  const opposite = value === 0 ? 1 : 0
  while (Atomics.load(ctrl, index) !== value) {
    Atomics.wait(ctrl, index, opposite)
  }
}

export class SharedMailbox {
  constructor(sab, { reqCap = DEFAULT_CAP, respCap = DEFAULT_CAP } = {}) {
    this.sab = sab
    this.reqCap = reqCap
    this.respCap = respCap
    this.ctrl = new Int32Array(sab, 0, CTRL)
    this.req = new Uint8Array(sab, HEADER_BYTES, reqCap)
    this.resp = new Uint8Array(sab, HEADER_BYTES + reqCap, respCap)
    this.seq = 0
    // 常量索引 (对象固化, 热循环不查表)
    this.RQ = { state: 0, flags: 1, len: 2, seq: 3 }
    this.RS = { state: 4, status: 5, len: 6, seq: 7 }
  }

  static create({ reqCap = DEFAULT_CAP, respCap = DEFAULT_CAP } = {}) {
    const sab = new SharedArrayBuffer(HEADER_BYTES + reqCap + respCap)
    return new SharedMailbox(sab, { reqCap, respCap })
  }

  // ── 请求方 (主线程) ────────────────────────────────────────────────

  async putRequest(method, args = []) {
    const payload = JSON.stringify([method, ...args])
    const bytes = encoder.encode(payload)
    if (bytes.length > this.reqCap) {
      throw new SharedMailboxError(`请求负载 ${bytes.length}B 超过上限 ${this.reqCap}B`)
    }
    // 等上一请求被消费 (响应方已置回 0) 再复用请求区
    await waitAsyncUntil(this.ctrl, this.RQ.state, 0)
    this.req.set(bytes)
    const seq = ++this.seq
    Atomics.store(this.ctrl, this.RQ.seq, seq)
    Atomics.store(this.ctrl, this.RQ.len, bytes.length)
    Atomics.store(this.ctrl, this.RQ.flags, 0)
    Atomics.store(this.ctrl, this.RQ.state, 1)
    Atomics.notify(this.ctrl, this.RQ.state, 1)
    return seq
  }

  async nextResponse() {
    const ctrl = this.ctrl
    await waitAsyncUntil(ctrl, this.RS.state, 1)
    const status = Atomics.load(ctrl, this.RS.status)
    const len = Atomics.load(ctrl, this.RS.len)
    const bytes = this.resp.subarray(0, len)
    const json = decoder.decode(bytes)
    const result = JSON.parse(json)
    // 消费应答 → 置空, 让响应方复用应答区
    Atomics.store(ctrl, this.RS.len, 0)
    Atomics.store(ctrl, this.RS.state, 0)
    if (status === 1) {
      throw new SharedMailboxError(String(result))
    }
    return result
  }

  async request(method, ...args) {
    await this.putRequest(method, args)
    return this.nextResponse()
  }

  requestShutdown() {
    Atomics.store(this.ctrl, this.RQ.len, 0)
    Atomics.store(this.ctrl, this.RQ.flags, FLAG_SHUTDOWN)
    Atomics.store(this.ctrl, this.RQ.state, 1)
    Atomics.notify(this.ctrl, this.RQ.state, 1)
  }

  // ── 响应方 (worker 线程) ────────────────────────────────────────────

  // 阻塞等请求 (worker 专用, 主线程会抛错)
  waitRequest() {
    waitSyncUntil(this.ctrl, this.RQ.state, 1)
    return Atomics.load(this.ctrl, this.RQ.flags)
  }

  takeRequest() {
    const flags = Atomics.load(this.ctrl, this.RQ.flags)
    if (flags & FLAG_SHUTDOWN) {
      // 关停信号: 无负载
      Atomics.store(this.ctrl, this.RQ.len, 0)
      Atomics.store(this.ctrl, this.RQ.state, 0)
      return { flags, method: "__shutdown__", args: [] }
    }
    const len = Atomics.load(this.ctrl, this.RQ.len)
    const json = decoder.decode(this.req.subarray(0, len))
    const [method, ...args] = JSON.parse(json)
    // 标记消费 → 请求方可重写请求区
    Atomics.store(this.ctrl, this.RQ.len, 0)
    Atomics.store(this.ctrl, this.RQ.state, 0)
    return { flags, method, args }
  }

  putResponse(result, error = false) {
    let payload
    if (error) {
      payload = result instanceof Error ? result.message : String(result)
    } else {
      payload = result
    }
    const bytes = encoder.encode(JSON.stringify(payload))
    if (bytes.length > this.respCap) {
      throw new SharedMailboxError(`应答负载 ${bytes.length}B 超过上限 ${this.respCap}B`)
    }
    // 等上一应答被消费 (置回 0) 再复用应答区
    waitSyncUntil(this.ctrl, this.RS.state, 0)
    this.resp.set(bytes)
    Atomics.store(this.ctrl, this.RS.status, error ? 1 : 0)
    Atomics.store(this.ctrl, this.RS.len, bytes.length)
    Atomics.store(this.ctrl, this.RS.state, 1)
    Atomics.notify(this.ctrl, this.RS.state, 1)
  }
}

// 便捷: 主线程侧拿到一块共享缓冲 (可 transfer 给 Worker)
export function createSharedBuffer({ reqCap = DEFAULT_CAP, respCap = DEFAULT_CAP } = {}) {
  return new SharedArrayBuffer(HEADER_BYTES + reqCap + respCap)
}