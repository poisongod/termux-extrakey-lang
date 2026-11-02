# AGENTS.md

tekl — Termux 键盘布局 DSL + TUI 工具链。**Rust Core + Rhai ID 插件**，GPL-3.0。
权威文档：`README.md`（架构）、`tekl-PRD.md`（需求）、`ebnf/*.ebnf`（文法）。

> ⚠️ README 的「HostOp 记录-回放」一节已过时：当前是**直接执行模型**
> （stub：`src/worker/script_runtime.rs`、`src/tui/app.rs::tui_engine`、`src/tui/editor.rs::ed_engine`）。

## 布局

- `src/main.rs` — 入口。子命令 `edit` / `reload` 的 `<file>` 均为必填；无参即布局 TUI。
- `src/lib/` — 共享核心（**Cargo.toml 无 [lib]，只是 bin crate 内的 `mod lib`**）：
  - `keys.rs` tekl 词法/语法/编译/打印（round-trip 契约）
  - `behavior.rs` ttoll 解析 + `dispatch`/`dispatch_chord` + 静态检查
  - `id.rs` `IdRegistry`：内置 `include_str!` 清单 + 用户 `~/.termux/id/` 扫描
  - `check.rs` `validate.rs` `json_dsl.rs` `keyset.rs` `user_dir.rs`
- `src/tui/` — `app.rs`（布局 TUI）+ `editor.rs`（双分区行为编辑器）+ `preview.rs`（键盘预览渲染）
- `src/id/{tui,editor}/<ID>.rhai` — 内置插件，**一 ID 一文件，文件名即 ID**
- `src/worker/script_runtime.rs` — Rhai 引擎封装 `ScriptHost{engine, lib}`；消费者把状态包进
  `Arc<Mutex<S>>`，在 `tui_engine`/`ed_engine` 用 `register_fn` 注册读写原语（捕获句柄）
- `src/compiler/generator.rs` + `src/termux/writer.rs` — reload 的编译与原子写盘
- 数据模板：`termux.tekl`（布局样例）、`tekl.ttoll` / `editor.ttoll`（**内容即 `~/.termux/` 下同名规则**，
  首次运行复制到主目录）。发布包把三个模板放在二进制同目录，运行时按 `bundled_asset()` 的顺序查找

## 命令

```bash
PATH=/data/data/com.termux/files/usr/bin:$PATH cargo …   # 见下方 Termux 坑
cargo build --release
cp target/release/tekl $PREFIX/bin/tekl                 # Termux 安装
```

运行：

```bash
tekl                          # 布局 TUI，编辑 ~/.termux/termux.tekl（首次运行自动生成）
tekl edit <file>              # .tekl → 布局 TUI；.ttoll → 行为编辑器；其他后缀报错退出
tekl reload <file> [--no-reload]
                              # 编译 .tekl → termux.properties（原子写+读回校验）→ termux-reload-settings
git tag vX.Y.Z && git push origin vX.Y.Z
                              # 触发 Release：NDK 交叉编译 aarch64/armv7 → 打包 → 发布
```

## ⚠️ 真实性注意（与文档/直觉不符处）

以前版本在这节记录的「`cargo test` 缺 `use std::sync::Arc;`」已在 `src/worker/script_runtime.rs`
的 tests 模块补上；若再动测试模块注意保留该 import。
- **规则文件由 provision 落地**：TUI 读 `~/.termux/tekl.ttoll`、编辑器读 `~/.termux/editor.ttoll`，
  `main` 在 TUI 路径启动时调用 `provision_user_dir()`，从二进制同目录（或源码目录）的
  `tekl.ttoll` / `editor.ttoll` / `termux.tekl` 复制，目标名与读取路径一一对应，已存在不覆盖。
  三份模板都随 Release 打包；规则文件缺失时 dispatch 表为空，所有按键无效。
- **模式号两套语义**：`0x01/0x02/0x03` 在布局 TUI 是 Normal/Edit/Popup（`Mode` in app.rs），
  在 ttoll 编辑器是 Normal/Insert/Command（`Mode` in editor.rs），互不共享。
- `id.txt` 只是人工速查表；ID 的 `desc` 真相源是各 `.rhai` 文件。

## 关键不变量

- **前后端分离硬契约**：Rust 核心 = 前端 UI（渲染/事件读取/输入层归一化）+ 完整 API（细粒度
  read/write 原语与不可再分的原子数据操作，如 `set_cursor`/`insert_text`/`add_row`/`save_file`），
  **不写任何策略**。所有操作组合/守卫/路由决策在 `.rhai` 的 `run` 闭包里（如 `if cursor_row() > 0 {...}`）。
  事件层必须把动作交给 dispatch→rhai（布局 TUI 插入菜单 Enter 只返回 action ID，不直接改数据）。
  新增功能 = 加 rhai 插件 +（必要时）加核心原语；禁止在 Rust 事件分支里实现操作逻辑。
- **无硬编码操作逻辑**：按键→ID 由 `.ttoll` 接线，ID→实现是 `.rhai` 的 `run` 闭包；
  Core 只做事件归一化 / dispatch / 原语注册。改行为 = 改数据文件（或加原语），不改 consumer 逻辑。
- **直接执行模型**：`run` 闭包经引擎内注册的原语直接读写 `Arc<Mutex<S>>` 状态（如
  `if cursor_row() > 0 { set_cursor(...) }`），不再经 HostOp 回放。事件循环两阶段：
  阶段一短暂持锁做路由/输入层处理，阶段二无锁执行 Rhai（引擎内部自行加锁）。
- **增删内置 ID 两处联动**：新建/删除 `src/id/{tui,editor}/<ID>.rhai` 的同时，必须同步
  `src/lib/id.rs` 的 `TUI_IDS` / `EDITOR_IDS` 清单（增一行 `include_str!`），否则不生效。
  用户 `~/.termux/id/` 只增不覆盖（同名内置被警告忽略）。
- **注册池按消费者隔离**：布局 TUI↔`tui` 清单 + `tekl.ttoll`，编辑器↔`editor` 清单 + `editor.ttoll`；
  `0x00` 全局等同（退出），其余各自独立编号。
- **文法冻结**：`ebnf/ttoll.ebnf` 键词汇固定 K∪D 共 9 个（ESC/ALT/CTRL/SPACE/ENTER + 四方向）。
  和弦（`CTRL+x`/`ALT+x`）的 x 也必须是文法键——`CTRL+SPACE`、`ALT+LEFT` 合法，`CTRL+C`/`CTRL+S`
  （字符级和弦）**非法**：`tokenize_rule` 直接拒绝，`is_known_key`/`crossterm_to_chord` 不产生候选。
  Ctrl+C（退出）、Ctrl+S（保存）等系统级快捷由**输入层硬编码**（布局 TUI `app.rs::handle_event`，
  编辑器 `editor.rs` Ctrl+C 强退），不经 ttoll。
  改文法 = 破坏性变更，须同步 behavior.rs 与全部 `.ttoll` 及测试。`ebnf/` 下 `.ebnf` 与
  `.ebnf.txt` 双份须同步。
- **输入层打字不经 ttoll**：编辑器 Insert 模式、布局 TUI 的 Edit/Popup 模式的字符/退格/Tab 直接
  内部处理；只有 crossterm 事件归一化出的键名（K∪D，及 CTRL+/ALT+ 且尾键属文法的和弦）走 dispatch。
- **round-trip 契约**：keys.rs 序列化结果必须被自身解析器还原一致——动它必跑测试。

## 测试

```bash
cargo test        # 需要 `use std::sync::Arc;`（tests 模块内已引入，勿删）
```

解析器、dispatch、ID 注册池、Rhai 脚本均有纯数据测试；布局 TUI 与行为编辑器的交互
（ratatui raw mode）靠实机验证。

## 提交规范

conventional commits，scope 用 `core`/`tui`/`id`/`ttoll`/`build`，提交信息用中文。