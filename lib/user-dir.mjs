// 用户主目录 (~/.termux/termux-extrakey) — 工具链运行时的规范所在。
// 存放: keys.layout 布局源码 / behavior/init.behavior 主核心 (基座程序) / plugins/init.mjs 插件系统入口。
// 仓库内文件只是"出厂模板", 首次运行 provisionUserDir() 复制到主目录, 用户自行迭代。
// 纯数据层: 不 import opentui, 可 node --test。

import { existsSync, mkdirSync, copyFileSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { fileURLToPath } from "node:url"

export const USER_ROOT = `${process.env.HOME}/.termux/termux-extrakey`

export const defaultLayoutFile = () => join(USER_ROOT, "keys.layout")
export const behaviorBaseFile = () => join(USER_ROOT, "behavior", "init.behavior")
export const pluginsDir = () => join(USER_ROOT, "plugins")
export const pluginsEntryFile = () => join(pluginsDir(), "init.mjs")

// 出厂模板 (仓库内): behavior.base / keys.layout → 主目录
const BUNDLED_BEHAVIOR = fileURLToPath(new URL("../behavior.base", import.meta.url))
const BUNDLED_LAYOUT = fileURLToPath(new URL("../keys.layout", import.meta.url))

// 基座程序: 主目录 init.behavior 有则用之, 否则用仓库模板
export function behaviorBaseContent() {
  const f = behaviorBaseFile()
  return readFileSync(existsSync(f) ? f : BUNDLED_BEHAVIOR, "utf8")
}

// 首次运行: 补齐主目录结构 (只创建缺失项, 不覆盖已有)。返回新建的路径列表。
export function provisionUserDir() {
  mkdirSync(join(USER_ROOT, "behavior"), { recursive: true })
  mkdirSync(pluginsDir(), { recursive: true })
  const created = []
  if (!existsSync(behaviorBaseFile())) {
    copyFileSync(BUNDLED_BEHAVIOR, behaviorBaseFile())
    created.push(behaviorBaseFile())
  }
  if (!existsSync(defaultLayoutFile())) {
    copyFileSync(BUNDLED_LAYOUT, defaultLayoutFile())
    created.push(defaultLayoutFile())
  }
  if (!existsSync(pluginsEntryFile())) {
    writeFileSync(pluginsEntryFile(), PLUGINS_INIT_SAMPLE)
    created.push(pluginsEntryFile())
  }
  return created
}

// plugins/init.mjs 出厂样品: 在此装配动作插件 (单一入口, 可 export default 数组)。
const PLUGINS_INIT_SAMPLE = `// 插件系统入口 — 动作插件的装配点。
// 约定: export default 插件声明 (definePlugin) 或声明数组。
//   贡献点 contribute: { actions, guards, rules, hooks, prims } (见仓库 lib/ui.mjs)
import { definePlugin } from "termux-extrakey/lib/ui.mjs"

export default definePlugin({
  name: "my-actions",
  deps: [],
  contribute: {
    actions: {},
    guards: {},
    rules: [],
    prims: {},
  },
})
`