// render 插件 (进程内) — TUI 渲染层。
// opentui FFI (createCliRenderer) 是主进程原生渲染器, 不能进 worker — 进程内。
// 依赖 ui + keys: 从内核取 dispatch/store/io 等能力, 注入 App。
// 提供能力: render.start(file, options) → 挂载渲染器 + App。

import { definePlugin } from "../kernel/kernel.mjs"
import { createCliRenderer } from "@opentui/core"
import { createRoot } from "@xincli/opentui-react"
import { createElement } from "react"
import { App } from "../../src/app.mjs"

export const renderPlugin = definePlugin({
  name: "render",
  version: "1.0.0",
  deps: ["ui", "keys"],
  activate(ctx) {
    // 从内核取 ui 能力, 装配 services 注入 App
    const ui = {
      dispatch: ctx.resolve("ui.dispatch"),
      initialState: ctx.resolve("ui.initialState"),
      eventToken: ctx.resolve("ui.eventToken"),
      store: ctx.resolve("ui.store"),
      loadDoc: ctx.resolve("ui.loadDoc"),
      saveDoc: ctx.resolve("ui.saveDoc"),
    }
    return {
      "render.start": (file, options = {}) => start(ui, file, options),
      "render.services": ui,
    }
  },
})

export async function start(ui, file, options = {}) {
  const { rows, file: docPath } = await ui.loadDoc(file)

  ui.store.setPaths(docPath ?? file, options.propsPath)
  ui.store.resetState(rows)

  const renderer = await createCliRenderer({ exitOnCtrlC: true })
  renderer.start()
  createRoot(renderer).render(createElement(App, { file, services: ui }))
}