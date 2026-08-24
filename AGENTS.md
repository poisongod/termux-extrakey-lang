# AGENTS.md

tekl — Termux 键盘布局 DSL + TUI 工具链。**Rust Core + Rhai ID 插件**，GPL-3.0。
权威文档：`README.md`（架构与目录地图）、`tekl-PRD.md`（产品需求）、`ebnf/*.ebnf`（文法）。

## 布局

- `src/main.rs` — 入口；子命令 `edit`（默认 TUI）/ `reload`（编译布局并写盘）
- `src/lib/` — 共享核心：
  - `keys.rs` tekl 词法/语法/编译/打印（round-trip 契约）
  - `behavior.rs` ttoll 解析 + `dispatch`/`dispatch_chord` + 静态检查
  - `id.rs` `IdRegistry`：内置清单（include_str!）+ 用户 `~/.termux/id/<ID>.rhai` 扫描
  - `check.rs` `validate.rs` `json_dsl.rs` `keyset.rs` `user_dir.rs`
- `src/tui/` — `app.rs`（布局 TUI）+ `editor.rs`（双分区行为编辑器）+ mode/keymap/preview/render
- `src/id/{tui,editor}/<ID>.rhai` — 内置 ID 插件，**一 ID 一文件，文件名即 ID**
- `src/worker/script_runtime.rs` — Rhai 引擎；宿主函数调用记录为 `HostOp` 序列，消费者回放
- `src/compiler/generator.rs` + `src/termux/writer.rs` — reload 子命令的编译与原子写盘
- 数据模板：`keys.layout`（布局样例）、`behavior.base`（TUI 规则基座，首次运行复制到主目录）

## 命令

```bash
cargo test                                    # 全部单元测试
cargo build --release
cp target/release/tekl $PREFIX/bin/tekl       # Termux 安装
```

> Termux 坑：`~/.cargo/bin` 的 rustup 空壳会遮蔽系统 cargo/rustc。
> 用 `PATH=/data/data/com.termux/files/usr/bin:$PATH cargo …`。

运行：

```bash
tekl edit <file>   # .tekl → 布局 TUI；.ttoll → 行为编辑器（左 ID 表｜右内容）
tekl reload        # 编译 ~/.termux/termux.tekl → termux.properties → 热更新
```

## 关键不变量

- **无硬编码操作逻辑**：按键→ID 在 `.ttoll`，ID→实现在 `.rhai` 的 `run` 闭包；
  Core 只做事件归一化 / dispatch / 宿主函数。改行为 = 改数据文件，不改源码。
- **文法冻结**：`ebnf/ttoll.ebnf` 键词汇固定 K∪D 共 9 个（ESC/ALT/CTRL/SPACE/ENTER + 四方向）。
  改文法 = 破坏性变更，须同步 behavior.rs 与全部 .ttoll 及测试。
- **一 ID 一文件**：`<0xHH>.rhai` 只含该 ID 的 `desc` + `run`；内置经清单嵌入，
  用户 `~/.termux/id/` 只增不覆盖。
- **注册池按消费者隔离**：TUI↔`tui.rhai`+`tekl.ttoll`，编辑器↔`editor.rhai`+`editor.ttoll`；
  `0x00` 全局等同（退出），其余各自从 0x01 起。
- **round-trip 契约**：keys.rs 序列化结果必须被自身解析器还原一致——动它必跑测试。
- **记录-回放**：Rhai 宿主调用不直接碰 UI 状态，只产生 `HostOp`，由消费者回放；
  脚本无副作用、可单测。

## 提交规范

conventional commits，scope 用 `core`/`tui`/`id`/`ttoll`/`build`，提交信息用中文。
