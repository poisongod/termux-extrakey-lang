// cli 插件 (进程内) — 命令入口。
// 提供能力: cli.main(argv) → 解析参数 → 装配三方插件 → 启动 render。
// 依赖 render (主进程渲染) + ui (注册三方插件) + keys (默认文件路径)。

import { definePlugin } from "../kernel/kernel.mjs"
import { loadPlugins } from "../ui.mjs"
import { provisionUserDir, pluginsDir, USER_ROOT } from "../user-dir.mjs"

export const cliPlugin = definePlugin({
  name: "cli",
  version: "1.0.0",
  deps: ["render", "ui", "keys"],
  activate(ctx) {
    return {
      "cli.main": (argv = []) => main(ctx, argv),
    }
  },
})

export function parseArgs(argv) {
  const out = { file: null, plugins: null, propsPath: null }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === "--plugins") {
      out.plugins = argv[++i] ?? null
    } else if (a === "--props") {
      out.propsPath = argv[++i] ?? null
    } else if (!a.startsWith("-")) {
      out.file = a
    }
  }
  return out
}

async function main(ctx, argv) {
  const { file, plugins, propsPath } = parseArgs(argv)

  // 主目录 (~/.termux/termux-extrakey): 首次运行补齐模板 + 装载 plugins 插件入口
  const created = provisionUserDir()
  if (created.length > 0) {
    console.log(`[termux-extrakey] 已初始化主目录 ${USER_ROOT}: ${created.map((f) => f.split("/").pop()).join(", ")}`)
  }
  const register = ctx.resolve("ui.registerPlugin")
  const idLoaded = await loadPlugins(pluginsDir(), { register })

  // 三方插件: 装配进 tui 行为层
  if (plugins) {
    const loaded = await loadPlugins(plugins, { register })
    if (loaded.length === 0) {
      console.error(`termux-extrakey: 未在 ${plugins} 发现 *.plugin.mjs 插件`)
    }
  }
  const pluginNames = [
    ...idLoaded.map((l) => l.def.name),
  ]
  if (pluginNames.length > 0) console.log(`[termux-extrakey] 已装配 plugins 插件: ${pluginNames.join(", ")}`)

  const start = ctx.resolve("render.start")
  if (!start) throw new Error("缺少 render.start 能力")
  await start(file, { propsPath })
}