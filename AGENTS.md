# AGENTS.md

Termux extra-keys toolchain, developed **on Termux (Android)**: `tekl` DSL compiler + `tjek` loose-JSON validator. GPL-3.0. Requires Node 26.3+ (26.4 verified here).

## Layout

- `lib/kernel/` — 稳定核心 (不变, 一切皆插件的唯一主机): `kernel.mjs` (Unity 托管/依赖拓扑/贡献聚合/activate 生命周期), `transport.mjs` (Worker Threads RPC: SAB 邮箱零拷贝 / MessagePort 降级), `shared.mjs` (SharedArrayBuffer+Atomics 邮箱), `worker.mjs` (线程侧协议 `serve()`)
- 单文件插件 (全部 Node ESM, 无第二语言; 装配 = `createKernel().register(keys/check/ui/render/cli).boot()`):
  - `lib/keys.mjs` — 布局工具链 + TUI io + worker 自持 (原 `lib/tekl/*` + `lib/tui/io.mjs` 合并): tokenizer → parser (recursive descent per DSL/`keys.layout`) → compiler (AST → ExtraKeyButton JSON + warnings) → compiler-io (write properties) + stringify/printer/jit/cli + loadDoc/saveDoc。插件 `keysPlugin`, worker 即本文件 (`!isMainThread` 分支 `serve()`); 方法面 = compile/io/save/load。
  - `lib/check.mjs` — extra-keys JSON DSL 校验 (原 `lib/tjek/*` 合并): `formatFullReport`/`snippet`/`run`, 插件 `checkPlugin` (worker 自持)。
  - `lib/ui.mjs` — TUI 行为层, **进程内插件** (原 `lib/tui/*` 除 behavior/user-dir 其余合并): 动作 id 插件内核 (`plugin.mjs` 段) + loader + mode 解释器 + actions (内置守卫) + prims + doc + matrix + store + event-token; 插件 `uiPlugin`, caps `ui.*`。
  - `lib/behavior.mjs` — 行为核心语言 (**核心**, 原 `lib/tui/ttoll.mjs`): 规则段+行为段解析/静态检查/prim 词汇表, 纯数据层 (`node --test`-able, 无任何 import)。
- 共享层 (`lib/` 根): `lib/json-dsl.mjs` (Android JSONTokener 兼容解析, 前缀 `json-dsl:`), `lib/validate.mjs` (四字段校验), `lib/keyset.mjs` (NAMED_KEYS/SPECIAL_KEYS 键词汇表, tokenizer 与 matrix 共用), `lib/user-dir.mjs` (用户主目录: provisionUserDir/pluginsDir/behaviorBaseContent/defaultLayoutFile)。
- `lib/plugins/` — 仅渲染/装配: `render.mjs` (opentui FFI, 进程内, deps ui+keys) + `cli.mjs` (命令入口, deps render+ui+keys)。
- `src/` — TUI render layer: `cli.mjs` (启动引导 → kernel.boot; `createKernel().register(五插件).boot()`; 子命令 tekl/tjek 分发到 `lib/keys.mjs`/`lib/check.mjs` 的 `run`), `app.mjs` (App root, **services 由 render 插件注入**, 不直接 import ui 单例), `components.mjs` (Grid/ModeBar/MenuPanel/MatrixPanel/NameInput/Problems/StatusBar)
- `behavior.base` — 内置行为基座程序模板 (原 `tekl.ttoll`); `keys.layout` — 布局源码示例 (原 `keys.tekl`, source comments use `#`); `docs/behavior.ebnf` (原 `docs/ttoll.ebnf`) — 行为语言文法; `docs/design.md` — 权威设计 (含命名映射 §5 / 单文件插件 §6+); `docs/tui-design.md` — 界面交互细节
- Docs (READMEs, warnings) are in Chinese; `docs/behavior.ebnf` is the behavior-grammar source (comments are `(* *)`, placeholders written out)

## Commands

```bash
npm install && npm link   # provides termux-extrakey (唯一对外命令)
npm test                  # 193 tests (kernel/transport/shared + behavior/ui 纯数据层 + keys/check); single file: node --test test/tekl.test.mjs
npm start                 # node --experimental-ffi src/cli.mjs (TUI dev)
```

- CLI 单命令 `termux-extrakey`: 无子命令 → 默认启动 TUI; `tekl [file|default ~/.termux/termux-extrakey/keys.layout] [--output <props>] [--reload] [--watch]` 编译; `tjek <file|-|stdin>` 校验。tekl/tjek 不单设 bin (稳定子命令接口, 内部不改名)。`--watch` recompiles on change and **exits on first success**。
- Shebangs in `src/cli.mjs` are Termux absolute paths (`/data/data/...`) — run via `node` or `npm link` outside Termux. 子命令实现 = `lib/keys.mjs` / `lib/check.mjs` 导出的 `run(argv)`。

## Key quirks

- **Round-trip contract**: `compiler-io.mjs` (keys.mjs 内) serializes DSL then **re-parses it with the shared json-dsl parser** and requires identical JSON before writing. Changing `stringifyDsl` (keys.mjs) or `lib/json-dsl.mjs` can break this invariant — run `npm test`.
- Compile path (`compileSource`) runs **two** validations before writing: `validateTermuxJson` (compiler) then `validateExtraKeyJson` (four-field `key`/`macro`/`display`/`popup`, `lib/validate.mjs`).
- Write path (`updateProperties`): replaces/appends `extra-keys = ...` in `~/.termux/termux.properties`, atomic via `.tekl.bak` rename (restored on write failure), verifies read-back.
- Case rules (`keyName` in keys.mjs): named keys (ESC, F1…) and modifiers (CTRL/ALT/FN/SHIFT) normalize to uppercase (case-insensitive input); plain combo identifiers and quoted literals keep original case. Combos default to **no** `display` field.
- Modifiers may not repeat within a row; `key`/`macro` are mutually exclusive; `popup` single level only; no rename/popup inside `()`.
- Warnings (`collectWarnings`, non-fatal, yellow, Chinese): empty field values, uneven row lengths, missing `DRAWER`/`KEYBOARD`.
- Source comments: layout DSL uses `#` to end of line (tokenizer); `(* *)` comments only appear inside `*.ebnf` grammar files.
- check CLI prints only the formatted report; set `CHECK_DEBUG=1` to also dump the full error object (named after old `TJEK_DEBUG`).
- TUI data flow: `event-token` (ui.mjs) normalizes opentui KeyEvent (ctrl/meta checked **before** bare arrow names so chord-level tokens win) → mode dispatch (纯解释器, behavior 程序驱动) → effects `apply`/`cancel`/`exit` (`saveDoc` only on apply/exit, 走 keys worker).
- **动作 id 插件系统** (ui.mjs 内, plugin.mjs 段 + mode wiring): 插件 = **声明式单元** `definePlugin({ name, deps, contribute: { actions, guards, rules, hooks, prims } })`; 内核只负责托管, 不知道业务贡献点为何物。内置 `builtinPlugin` (actions 段) = 第一个注册者, 只贡献 `guards`。注册 = `registerPlugin` / `registerAction` / `registerGuard` / `registerActionWithRules`(行为+键位一次交付) / `registerPrim`(原语词汇表扩展) / `wrapAction`; 均返回注销器, 注销时**重建 dispatch** (即时生效)。
- **behavior 行为核心语言** (`lib/behavior.mjs`): 程序 = 规则段 (declares 谁触发谁) + 行为段 `@id` (declares 动作做什么, 内在语句 = 内置操作 `mode/escpanel/sel/unsel/apply/cancel/exit/seterrors` + 原语调用 prims)。`parseProgram` 静态检查: 键/守卫/原语/动作定义/模式可达。解释器 `buildDispatch` (ui.mjs): 合并贡献 → parseProgram → 查表 → 守卫 → 编译脚本执行 → hooks; **无硬编码 JS 状态机**。显式注册的 JS 动作优先覆盖行为段 (wrapAction 兼容)。`behavior.base` = 内置动作全部声明化的基座程序 (模板复制到主目录 `behavior/init.behavior`)。`test/behavior.test.mjs` 守护行为段解析/解释/原语扩展。
- **内核插件系统** (`lib/kernel/`): 插件声明 `{ name, deps, capabilities, worker?, activate? }`; **子进程插件** = 独立 worker 线程 (keys/check 的 worker 即自身文件, 经 `worker.mjs` `serve()` 暴露方法, `transport.mjs` 用 **SharedArrayBuffer+Atomics 邮箱** 零拷贝, 降级 MessagePort); **进程内插件** = `capabilities` 直接贡献值/函数 (ui/render/cli)。`activate(ctx)` 按依赖序跑, 可返回动态贡献 (覆盖静态)。`kernel.call(name, method, ...args)` 统一代理线程/进程内。`boot()` = 全插件 activate。**热路径 (dispatch) 必须在进程内** — 跨线程只做编译/校验/io (ui 的 loadDoc/saveDoc 经 `ctx.call("keys", …)` 到 keys worker)。
- Commas: printer must be in sync with tokenizer/parser — extend both together (`test/printer.test.mjs` round-trip).
- The opentui native FFI (`createCliRenderer`) has no CTS for this repo — render/interaction layers verified only via `node --test` on pure data modules + manual smoke on device.
- All ESM `.mjs`, no external test framework (`node --test`). Commit style: conventional commits with `keys`/`check`/`ui`/`behavior`/`kernel` scope, messages in Chinese.

## Stale docs

- Root `README.md` says "53 用例"; actual is 193 (`docs/keys.md`/`docs/check.md` are **pre-merge legacy docs** and count 76 — treat `npm test` tally as truth).
- Layout DSL grammar (`keys.layout`/`keys.mjs`): `NAMED_KEYS` in `lib/keyset.mjs` is canonical (includes `SCROLL` and `PASTE`); keep any EBNF `N` list in sync. The keyset lives in `lib/keyset.mjs` and is shared with `lib/ui.mjs` matrix.