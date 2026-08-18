#!/data/data/com.termux/files/usr/bin/env node
// termux-extrakey — 唯一命令入口 (设计 §2/§5): 一切皆插件。
// 无子命令默认启动 TUI (组装内核 → boot → 交给 cli 插件)。
// 子命令参数: tekl 编译 | tjek 校验 (内部工具链, 不单独对外开放)。
// 用法: termux-extrakey                 → TUI (默认)
//       termux-extrakey tekl <f> [...]  → tekl 编译 (--output/--reload/--watch)
//       termux-extrakey tjek <file|->   → tjek 校验 (可 stdin)

// ── FFI 自举: opentui 需要 --experimental-ffi, 否则重执行 (bin 链接无法带 node 标志) ──
import { spawnSync } from "node:child_process"

function ensureFFI(argv) {
  // 仅 TUI 分支需要 FFI (tekl/tjek 子命令不依赖)
  if (argv[0] === "tekl" || argv[0] === "tjek" || argv[0] === "--help" || argv[0] === "-h") return
  if (process.execArgv.includes("--experimental-ffi")) return
  const res = spawnSync(
    process.execPath,
    ["--experimental-ffi", import.meta.filename, ...argv],
    { stdio: "inherit" },
  )
  process.exit(res.status ?? 1)
}

const USAGE = `usage: termux-extrakey [subcommand] [args...]
  (no subcommand)                         start the TUI (default)
  tekl <file.tekl> [--output ...] ...     compile tekl into termux.properties
  tjek <file | ->                         validate an extra-keys JSON DSL`

async function runTui(argv) {
  const [{ createKernel }, { keysPlugin }, { checkPlugin }, { uiPlugin }, { renderPlugin }, { cliPlugin }] =
    await Promise.all([
      import("../lib/kernel/kernel.mjs"),
      import("../lib/keys.mjs"),
      import("../lib/check.mjs"),
      import("../lib/ui.mjs"),
      import("../lib/plugins/render.mjs"),
      import("../lib/plugins/cli.mjs"),
    ])
  const kernel = createKernel()
  kernel.register(keysPlugin)
  kernel.register(checkPlugin)
  kernel.register(uiPlugin)
  kernel.register(renderPlugin)
  kernel.register(cliPlugin)
  await kernel.boot()

  const cliMain = kernel.resolve("cli.main")
  if (!cliMain) throw new Error("缺少 cli.main 能力")
  await cliMain(argv)
}

main().catch((err) => {
  console.error(`termux-extrakey: ${err.message}`)
  process.exit(1)
})

async function main() {
  const argv = process.argv.slice(2)
  ensureFFI(argv)
  const cmd = argv[0]
  switch (cmd) {
    case "tekl":
      return await (await import("../lib/keys.mjs")).run(argv.slice(1))
    case "tjek":
      return await (await import("../lib/check.mjs")).run(argv.slice(1))
    default:
      if (cmd === "--help" || cmd === "-h") {
        console.log(USAGE)
        return
      }
      return await runTui(argv)
  }
}