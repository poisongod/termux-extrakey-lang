# 设计 (design)

> 本文档为 **最新完整设计**, 描述 target architecture (含已完成的单文件插件合并)。
> 命名一律按**实际用途** (弃用 tekl/tjek/ttoll/TUI 及项目标识; 见 §5 映射表)。
> 相关: `docs/tui-design.md` (界面交互细节), 行为语言文法 `docs/behavior.ebnf`
> (原 `docs/ttoll.ebnf`)。

## 1. 定位

Termux 扩展键盘 (`extra-keys`) 的 **键盘布局工具链 + 终端界面编辑器**。一切皆插件,
单一对外命令 `termux-extrakey`。运行时数据与用户配置以 **用户主目录** 为规范所在。

## 2. 命令面 (唯一命令 + 子命令参数)

唯一对外命令 `termux-extrakey`, 其余功能不单独开放, 全部经参数/子命令进入:

```
termux-extrakey                     # 默认: 启动终端界面编辑器 (TUI)
termux-extrakey tekl <f> [--output <props>] [--reload] [--watch]
                                    # 编译键盘布局 DSL → 写 termux.properties
termux-extrakey tjek <file | ->     # 校验 extra-keys JSON DSL (文件或 stdin)
termux-extrakey --help
```

- `tekl`/`tjek` 仅为内部子命令, 不设独立 bin; `package.json` bin 只暴露 `termux-extrakey`。
- `tekl` 失败默认输入 = 主目录键盘布局源; 缺文件时报错。
- `--watch`: 文件变更即重编译写盘, **首次成功后自动退出**。
- **FFI 自举**: opentui 需要 `--experimental-ffi`; bin 无法带 node 标志,
  `src/cli.mjs` 在 TUI 分支检测 `process.execArgv`, 缺则用 `spawnSync` 以该标志重执行。
  `tekl`/`tjek` 子命令不经过 FFI 分支。

## 3. 用户主目录 (~/.termux/termux-extrakey/)

工具链运行时的规范所在, 首次启动自动补齐模板 (仓库模板复制, **不覆盖已有**):

```
~/.termux/termux-extrakey/
├── keys.layout          # 键盘布局源 (原 termux.tekl) — 默认编辑/编译对象
├── behavior/
│   └── init.behavior    # 主核心: 行为规则程序 (规则段+行为段)
└── plugins/
    └── init.mjs         # 插件系统入口: 装配动作插件 (export default 声明或声明数组)
```

- `keys.layout`: TUI 默认加载/保存的布局源; `tekl` 子命令默认输入。
- `behavior/init.behavior`: 解释引擎的行为基座程序; 缺失时回退仓库内置模板。
- `plugins/init.mjs`: 用户插件的单一装配入口; 加载器先读它, 再扫描同目录 `*.plugin.mjs`。
- provision 只创建缺失项, 打印提示, 不动用户已有文件。

## 4. 模块布局 (合并后的单文件插件)

```
lib/
├── kernel/                  # 插件宿主 (唯一主机, 不合并): kernel/transport/shared/worker
│   ├── kernel.mjs           #   内核: 注册/依赖拓扑/activate/贡献聚合/call
│   ├── transport.mjs        #   线程 RPC: SAB 邮箱零拷贝, 降级 MessagePort
│   ├── shared.mjs           #   SharedArrayBuffer 邮箱原语
│   └── worker.mjs           #   线程侧协议 serve()
├── behavior.mjs             # 核心: 行为语言 (规则段+行为段 解析/静态检查/分发表) — 纯语言零依赖
├── ui.mjs                   # 单文件插件: 界面行为层 (解释器/原语集/内置守卫/编辑模型/状态/插件内核/加载器)
├── keys.mjs                 # 单文件插件: 键盘布局工具链 (词法/语法/编译/打印/序列化/读写 io/JIT/CLI)
├── check.mjs                # 单文件插件: JSON 校验 (错误报告/CLI)
├── json-dsl.mjs             # 共享: JSONTokener 兼容宽松 JSON 解析 (keys ∪ check)
├── validate.mjs             # 共享: extra-keys 四字段结构校验 (keys ∪ check)
├── keyset.mjs               # 共享: NAMED_KEYS/SPECIAL_KEYS 键词汇表 (keys tokenizer ∪ ui matrix)
├── user-dir.mjs             # 共享: 用户主目录 路径/模板/供给 (keys ∪ ui ∪ cli)
└── plugins/
    ├── render.mjs           # 渲染插件 (进程内; opentui FFI 不能进 worker)
    └── cli.mjs              # 命令插件 (进程内; 主目录装配 + 三方插件的装载)
```

**单文件插件原则**: 一个功能域收敛为一个 `.mjs`; 域内私有逻辑全部内联,
不再拆目录。**共享原则**: 跨功能域共同使用的逻辑 → 独立 `.mjs` (实例:
`json-dsl.mjs` / `validate.mjs` / `keyset.mjs` / `user-dir.mjs`; `error-report` 仅 check 用 → 留在 check)。
**子进程单文件守卫**: `keys.mjs` / `check.mjs` 用 `import { isMainThread }`
判断 — 作为 worker 被加载时 (`!isMainThread`) 调 `serve(handlers)`, 平时仅导出插件声明。

## 5. 命名规范 (实际用途优先)

弃用 tekl/tjek/ttoll/TUI 及项目标识命名, 以**实际职责/用途**为标准:

| 旧标识 | 旧位置 | 新名 (用途) | 新位置 |
| --- | --- | --- | --- |
| tekl 工具链 | `lib/tekl/*` | 键盘布局工具链 | `lib/keys.mjs` |
| tjek 工具链 | `lib/tjek/*` | JSON 校验 | `lib/check.mjs` (+ json-dsl/validate) |
| ttoll 行为语言 | `lib/tui/ttoll.mjs` | 行为语言 (**核心**) | `lib/behavior.mjs` |
| tui 行为层 | `lib/tui/*` | 界面行为层 | `lib/ui.mjs` |
| json-dsl | `lib/tjek/json-dsl.mjs` | 宽松 JSON 解析 (共享) | `lib/json-dsl.mjs` |
| validate-json | `lib/tjek/validate-json.mjs` | 四字段校验 (共享) | `lib/validate.mjs` |
| user-dir | `lib/tui/user-dir.mjs` | 用户主目录 (共享) | `lib/user-dir.mjs` |
| keyset | `lib/tekl/tokenizer.mjs` 内 | 键词汇表 NAMED_KEYS/SPECIAL_KEYS (共享) | `lib/keyset.mjs` |
| plugins/tekl*.mjs, tjek*.mjs, tui.mjs | `lib/plugins/` | 并入 keys/check/ui 单文件 | — |
| tekl plugin/cap | `ctx.call("tekl", …)` | 插件名 `keys`, cap `keys.*` | 并入 `lib/keys.mjs` |
| tjek plugin/cap | — | 插件名 `check`, cap `check.*` | 并入 `lib/check.mjs` |
| tui plugin/cap | `tui.dispatch` … | 插件名 `ui`, cap `ui.*` | 并入 `lib/ui.mjs` |
| tekl.ttoll (仓库基座) | `tekl.ttoll`, `keys.tekl` | 主核心模板 / 布局示例 | → `behavior/init.behavior`, `keys.layout` |

- 插件注册名与 capability 前缀同步改名 (`kernel.call("keys", …)` / `ctx.resolve("ui.*")`)。
- 对外命令与子命令名 (`tekl`/`tjek`) 保留为稳定接口, 不随内部改名。
- 函数/导出命名同样以用途为准 (如错误前缀 `ttoll:` → `behavior:`, `tjek:` → `check:`)。

## 6. 插件宿主 (lib/kernel/)

- **一切皆插件**: 单元 = `definePlugin({ name, deps, capabilities?, worker?, activate? })`。
- **host 不知业务**: 只托管 (注册/依赖拓扑/审计), capability 合并/路由不感知语义。
- **进程内** = `capabilities` 直接贡献值/函数 (渲染/界面层, 热路径要求同步)。
- **子进程** = `worker` 指向脚本, 方法经 `transport.mjs` RPC
  (SharedArrayBuffer+Atomics 邮箱零拷贝, 降级 MessagePort), 线程侧 `serve(handlers)`。
- **热路径 (dispatch) 必须在进程内**, 跨线程只做编译/校验/io。
- `activate(ctx)` 按依赖序调用, 返回动态贡献 (优先于静态)。

## 7. 行为核心语言 (lib/behavior.mjs)

- **程序** = 规则段 (谁触发谁) + 行为段 (动作做什么), 全部声明式。
- 规则: `M . P . 键 [?守卫]… > 动作 id`; 守卫链按声明序求值, 首条命中执行。
- 行为段: `@id` 换行后语句行; 语句 = 内置操作 (`mode/escpanel/sel/unsel/apply/cancel/exit/seterrors`)
  或 原语 (词汇表, 可按 `registerPrim` 扩展)。
- 静态检查: 键/守卫/原语合法性, 动作必须有行为段定义, 模式 0 可达 (BFS)。
- 运行时解释器在 ui.mjs (`mode` 域): 合并贡献 → 解析 → 查表 → 守卫 → 执行脚本 → hooks。
- 行为程序源: 主目录 `behavior/init.behavior` (或缺省仓库模板) + 插件贡献 rules 拼接。
- **核心文件零依赖**, 是纯语言层; 解释/原语在 ui.mjs。

## 8. 数据流 (界面)

```
opentui KeyEvent ─→ eventToken (ctrl/meta 优先) ─→ ui.dispatch (行为程序驱动)
   → 查表 → 守卫 → 行为脚本 (内置操作 + 原语) → 新状态 + 效果
   → store.setState → React 渲染 → 效果 apply/cancel/exit → 写盘 (keys worker)
```

- 名称输入框是唯一自由文本例外 (不经过行为机)。
- `apply`/`exit` 才写盘 (`saveDoc`); `cancel` 仅回滚预览。

## 9. 共享模块职责

| 模块 | 职责 | 消费方 |
| --- | --- | --- |
| `json-dsl.mjs` | Android `JSONTokener` 兼容解析 (宽松 JSON) | keys (round-trip), check (解析), ui (校验前置) |
| `validate.mjs` | extra-keys 四字段结构校验 (key/macro/display/popup) | keys (编译写盘前), check (CLI) |
| `user-dir.mjs` | 主目录路径常量 + 模板供给 + 首次 provision | keys (默认布局源), ui (行为基座/插件目录), cli (provision+id 装载) |

## 10. 校验与错误处理

- 编译路径双校验: `validateTermuxJson` (keys 编译期) + `validate` (共享四字段)。
- round-trip 合同: 序列化后经 `json-dsl` 再解析, 须与语义等价 JSON 一致才写盘。
- 写盘原子性: `.bak` 改名 + 失败回滚 + 读回校验。
- 错误报告: 语法/校验错误带源码片段、行列定位、修复提示。

## 11. 测试

- `node --test` (无外部框架), 纯数据层 (kernel/ui/behavior/keys 非渲染部分) 可单测。
- 渲染/交互层无 CTS, 靠设备手工冒烟。
- 守护: round-trip 一致、行为程序解析/解释/原语扩展、插件装配与注销。