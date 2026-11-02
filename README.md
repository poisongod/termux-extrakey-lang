# tekl

**Termux Extra key Language** — 键盘布局 DSL + TUI 工具链。

用一门小语言描述 Termux 额外按键行（extra-keys），配合两套终端 UI 可视化编辑
（键盘布局编辑、行为规则编辑），并一键编译回写 `termux.properties` 热部署。

- Rust Core（微内核）+ Rhai ID 插件，GPL-3.0（`LICENSE`）
- 当前版本 v0.3.0
- 需求文档：`tekl-PRD.md`；文法：`ebnf/*.ebnf`（另有 `.ebnf.txt` 同文副本）

```
        .tekl 源码                 ttoll 规则                ID 插件
 ┌──────────────────┐     ┌────────────────────┐    ┌──────────────────┐
 │ termux.tekl 语法  │     │ 按键 → 行为 ID 接线 │    │ 每个 ID 一个 .rhai│
 │ ESC; TAB "~"; …  │     │ 0x01,UP>0x10 …     │    │ #{run: || …}      │
 └──────────────────┘     └────────────────────┘    └──────────────────┘
        │                          │                         │
        v                          v                         v
   keys 编译器               dispatch 表                Rhai 引擎 (微内核 API)
        │                                                    │
        v                                                    v
  extra-keys JSON      ┌───────────────────────────────────────┐
        │              │  Arc<Mutex<App>> / <EditorState> 状态 │
        v              └───────────────────────────────────────┘
  termux.properties
        │
        v
  termux-reload-settings
```

## 目录

- [安装与构建](#安装与构建)
- [快速开始](#快速开始)
- [tekl：键盘布局语言](#tekl键盘布局语言)
- [ttoll：行为规则语言](#ttoll行为规则语言)
- [ID 插件系统（微内核完整实现清单）](#id-插件系统)
- [数据文件](#数据文件)
- [热部署 reload](#热部署-reload)
- [架构与契约](#架构与契约)
- [测试](#测试)
- [许可](#许可)

---

## 安装与构建

在 Termux 中构建（避免 rustup 空壳遮蔽系统工具链）：

```bash
PATH=/data/data/com.termux/files/usr/bin:$PATH cargo build --release
cp target/release/tekl $PREFIX/bin/tekl        # 通常即上面的 /data/data/.../usr/bin
```

> release 配置了 LTO + opt-level z，构建较慢；开发期用
> `cargo build`（debug）后拷贝同一路径即可。

打包 Release 资产（二进制与出厂模板同包发放）：

```bash
scripts/package-release.sh aarch64    # → release/tekl-aarch64.tar.gz
scripts/package-release.sh armv7      # → release/tekl-armv7.tar.gz
gh release upload v0.4.0 release/tekl-aarch64.tar.gz --clobber
```

包内 `tekl-<arch>` 与三个模板（`termux.tekl` / `tekl.ttoll` / `editor.ttoll`）同级。程序查找出厂模板的顺序是：
`$TEKL_ASSET_DIR` → 可执行文件同目录 → `$PREFIX/share/tekl/` → 编译期源码目录，
所以解压后直接运行即可完成首次初始化（布局与两套按键接线一并落地）。

> 脚本 shebang 是 Termux 绝对路径；在非 Termux 环境用 `bash scripts/package-release.sh …`。

## CI/CD

| 工作流 | 触发 | 内容 |
|---|---|---|
| `.github/workflows/ci.yml` | `main` 推送 / PR | `cargo build --release --locked` + `cargo test --locked` |
| `.github/workflows/release.yml` | 推送 `v*` 标签 | Android NDK 交叉编译 aarch64 / armv7 → 打包 → 发布 Release |

发版只需打标签：

```bash
git tag v0.4.0
git push origin v0.4.0
```

资产为 `tekl-aarch64.tar.gz` / `tekl-armv7.tar.gz`，包内是 `tekl-<arch>` +
`termux.tekl` + `README.md`。CI 用 NDK r27b、API 24（Termux 最低支持 Android 7）。

## 快速开始

三个入口命令（子命令的 `<file>` 为必填；不带子命令即进入布局 TUI）：

```bash
tekl                              # 布局 TUI，编辑 ~/.termux/termux.tekl（首次运行自动生成）
tekl edit <file>                  # .tekl → 布局 TUI；.ttoll → 行为编辑器；其他后缀报错
tekl reload <file> [--no-reload]  # 编译 .tekl → termux.properties → termux-reload-settings
```

### 布局 TUI（`tekl` / `tekl edit keys.tekl`）

- **键盘浏览**（Normal，`0x01`）：方向键移动焦点；`SPACE` 选中/取消；`ENTER` 进入主键编辑；
  `ESC` 退出。预览区按当前模式显示键值（Normal/Edit 显示主键值，Popup 显示弹出键值）。
- **主键编辑**（Edit，`0x02`）：直接输入键名/标签/宏文本；`Tab` 在字段
  Key → Display → Macro → Popup 间循环；`ENTER` 应用并返回。
- **弹出键编辑**（Popup，`0x03`）：编辑弹出键值，字段限定在弹出内容。
- 插入菜单（由 `0x40` 打开）：增行/增列/删列，Enter 确认后交 ID 插件执行。
- `Ctrl+S` 保存、`Ctrl+C` 退出 —— 输入层硬编码，不经 ttoll。

### 行为规则编辑器（`tekl edit editor.ttoll`）

双分区：左侧 ID 表（内置 + 用户插件），右侧 ttoll 规则文件编辑区。ID 表里
选中某个 ID 后按 `Enter` 跳到内容区中该 ID 对应的接线行；未接线则提示用
`:map` 生成。

- Normal：方向键移动光标；`ENTER` 进入插入；`SPACE` 进入命令；`ESC` 退出；
  `Ctrl+UP/DOWN` 跳首/末行；`Ctrl+ENTER` 保存（校验）；`Ctrl+ESC` 保存退出；
  `Alt+ENTER` 帮助；`Alt+Space` 切换 ID 面板。
- Insert：字符/退格/`Tab` 输入；`ENTER` 换行；`ESC` 返回 Normal。
- Command：输入命令后 `ENTER` 执行，`ESC` 放弃。状态栏实时回显 `:命令`。
- `ALT+LEFT` / `ALT+RIGHT` 切换左/右分区（输入层，不经 ttoll）；`Ctrl+C` 强制退出。

### 命令（Command 模式）

```text
:w :s       校验并保存
:w! :s!     强制保存
:wq :x      保存并退出
:q          退出（有改动则拒绝，用 :q!）
:q!         放弃改动退出
:map 01 UP 10   生成合法接线行 0x01,UP>0x10（光标停在 ID 处直接改写）
:tmpl       插入模板 0x01,UP>0x10
:del        删除当前行
:ids        在状态栏列出全部 ID
:set ids / :hide ids   焦点切到 ID 面板 / 内容面板
:help :h    帮助
```

生成规则不必打 `,` `>` `0x` 等符号——`:map 01 UP 10` 全是字母数字，
手机端也能顺手输入。

## tekl：键盘布局语言

词法/语法/编译/打印实现于 `src/lib/keys.rs`，文法见 `ebnf/tekl.ebnf`。
序列化结果必须能被自身解析器还原（round-trip 契约）。

| 写法 | 含义 |
|---|---|
| `ESC` | 命名键（大写键名） |
| `a` | 字面键（小写字符/串），如 `d`、`;`、`"\\"` |
| `A:B` | 重命名标签，如 `UP:Home` |
| `A(B)` | 组合键，可嵌套，如 `CTRL(c)`、`KEYBOARD(CTRL(d))` |
| `A(B):Label` | 组合键重命名 |
| `A.B` | 弹出键（A 为主键，B 为弹出内容），可 `A.B:Label` |
| `;` | 键盘行分隔符 |

示例（仓库 `termux.tekl`，实体键盘映射 + 倒 T 方向键）：

```text
ESC.DRAWER "`"."~" "["."{" "]"."}" APOSTROPHE.QUOTE UP.HOME "="."+" KEYBOARD;
TAB."^" CTRL ALT BACKSLASH."|" LEFT."<" DOWN.END RIGHT.">" ENTER.PASTE;
```

> 字面串不做转义：`"\\"` 会被解析成两个反斜杠。反斜杠、双引号、单引号请用命名键
> `BACKSLASH`、`QUOTE`、`APOSTROPHE`。

## ttoll：行为规则语言

TUI 操作逻辑语言：把按键事件映射到行为 ID。输入层（`src/tui/app.rs` /
`editor.rs`）先把 crossterm 事件归一化为键名，再按当前模式号查规则表分派。
解析/分派实现于 `src/lib/behavior.rs`，文法见 `ebnf/ttoll.ebnf`。

规则形式：

```text
0x02,ESC>0x01        # 模式,键 → ID（最常用形式）
0x03,ENTER>0x13
CTRL+SPACE>0x40      # 和弦可作全局规则（无模式前缀时任意模式生效）
0x01,ALT+LEFT>0x42
```

### 文法冻结（K ∪ D）

键词汇**固定**为 9 键两族：

- **K** = `ESC` `ALT` `CTRL` `SPACE` `ENTER`
- **D** = `UP` `DOWN` `LEFT` `RIGHT`

约束（解释器直接拒绝非法形式）：

- 和弦 `CTRL+x` / `ALT+x` 的 x **必须也是文法键**：`CTRL+SPACE`、`ALT+LEFT` 合法；
  `CTRL+C`、`CTRL+S` 等字符级和弦 **非法**。
- 字符键 `i`、`Enter` 打引号形式均不产生分派候选。

改变文法 = 破坏性变更，须同步 `behavior.rs`、全部 `.ttoll`、测试与
`ebnf/*.ebnf(.txt)` 双份。

### 输入层硬编码（不经 ttoll）

以下行为由输入层内部处理，不查规则表、不可被 `.ttoll` 覆盖：

- **系统快捷键**：布局 TUI `Ctrl+C` 退出、`Ctrl+S` 保存；编辑器 `Ctrl+C` 强退。
- **打字**：编辑器 Insert 模式的字符/退格/`Tab`；布局 TUI Edit/Popup 模式的文本编辑。
- **界面导航**：编辑器 `ALT+LEFT/RIGHT` 切换分区、ID 表 `↑↓` 滚动。

### 默认接线

布局 TUI 规则模板 `tekl.ttoll`（首次运行复制至 `~/.termux/tekl.ttoll`）：

```text
0x01,ESC>0x00        # 退出
0x01,UP>0x10 0x01,DOWN>0x11 0x01,LEFT>0x12 0x01,RIGHT>0x13
0x01,ENTER>0x02      # 主键编辑
0x01,SPACE>0x30      # 选中/取消
0x01,CTRL+SPACE>0x40 # 插入菜单（键帽/矩阵操作）
```

编辑器规则 `editor.ttoll`：

```text
0x01,UP>0x04 0x01,DOWN>0x03 0x01,LEFT>0x01 0x01,RIGHT>0x02
0x01,ENTER>0x07      # 进入插入模式
0x01,SPACE>0x08      # 进入命令模式
0x01,ESC>0x00        # 退出
0x01,CTRL+UP>0x05    # 跳首行         0x01,CTRL+DOWN>0x06   # 跳末行
0x01,ALT+ENTER>0x12  # 帮助           0x01,ALT+SPACE>0x11   # 切 ID 面板
0x01,CTRL+ENTER>0x15 # 保存(校验)     0x01,CTRL+ESC>0x17    # 保存退出
0x02,ESC>0x09        # Insert → Normal
0x02,ENTER>0x0B      # 换行
0x03,ESC>0x09        # Command → Normal
0x03,ENTER>0x13      # 执行命令
```

## ID 插件系统

**微内核即完整实现**：id.txt 列出的每一个 ID，都有对应的 `.rhai` 插件文件与核心原语支撑，
三处清单一一对应、缺一不可：

1. `id.txt` —— 人工速查表（非真相源）
2. `src/id/{tui,editor}/<ID>.rhai` —— 真相源，文件名即 ID，`run: || 闭包`
3. `src/lib/id.rs` —— `TUI_IDS` / `EDITOR_IDS` 编译期 `include_str!` 清单

每个插件是 Rhai 对象字面量，至少含 `desc` 与 `run`：

```rhai
// src/id/tui/0x10.rhai — 焦点上移
#{
    desc: "焦点上移",
    run: || if cursor_row() > 0 { set_cursor(cursor_row() - 1, cursor_col()) }
}
```

`run` 闭包调用**核心提供的细粒度原语**（读 + 原子写），自行组合守卫与路由。

### 内置 ID 一览

布局 TUI（`id.txt`，含 0x00–0x53 共 25 个）：

```text
0x00  退出/返回          0x10-13  焦点上/下/左/右
0x01  键盘模式          0x20/21  行前/后增行
0x02  主键编辑          0x22/23  列前/后增列
0x03  弹出键编辑        0x24/25  删除当前行/列
0x04  应用编辑并返回    0x26/27  交换行/列
0x30  选中/取消          0x31  交换键
0x40  主键值矩阵(插入菜单)  0x41  主键值输入框   0x42  主键名输入框
0x50  弹出键值矩阵      0x51  弹出键值输入框  0x53  弹出键名输入框
```

ttoll 编辑器（0x00–0x17 共 24 个）：

```text
0x00  退出
0x01-04  光标左/右/下/上     0x0A  退格            0x11  切换ID面板
0x05  跳转首行              0x0B  换行(拆行)       0x12  切换帮助
0x06  跳转末行              0x0C  插入Tab         0x13  执行命令
0x07  进入插入模式          0x0D  删除字符        0x14  退出命令
0x08  进入命令模式          0x0E  删除当前行      0x15  保存(校验)
0x09  返回Normal            0x0F  下方新建行      0x16  强制保存
                            0x10  上方新建行      0x17  保存退出
```

### 用户扩展

`~/.termux/id/<ID>.rhai` 可增加自定义 ID（出厂为空目录，同名内置被警告忽略）。
文件名即 ID；用户插件**只增加、不覆盖**内置。

### 注册池隔离

- 布局 TUI ↔ `tui` 清单 + `tekl.ttoll`
- 编辑器 ↔ `editor` 清单 + `editor.ttoll`
- `0x00` 两池等同（退出），其余 ID 各自独立编号。

> **增删内置 ID 两处联动**：新建/删除 `<ID>.rhai` 时，必须同步
> `src/lib/id.rs` 清单，否则不生效。

### 模式号两套语义

- 布局 TUI：`0x01` 键盘浏览 / `0x02` 主键编辑 / `0x03` 弹出键编辑
- ttoll 编辑器：`0x01` Normal / `0x02` Insert / `0x03` Command

互不共享、同名异义，规则表的模式号按各自消费者解释。

## 数据文件

| 路径 | 内容 |
|---|---|
| `~/.termux/termux.tekl` | 键盘布局源码（首次运行从 `termux.tekl` 复制） |
| `~/.termux/tekl.ttoll` | 布局 TUI 行为规则（首次运行从 `tekl.ttoll` 复制） |
| `~/.termux/editor.ttoll` | 编辑器行为规则（首次运行从 `editor.ttoll` 复制） |
| `~/.termux/id/` | 用户 ID 插件目录 |

> **注意**：三个模板都在 Release 包内、与二进制同级，首次运行时由 `provision_user_dir()`
> 复制到 `~/.termux/`，已存在的文件不覆盖。规则文件缺失时 dispatch 表为空，所有按键无效。
>
> 也是因为这一点，`Cargo.toml` 无 `[lib]` target —— 全部代码是 bin crate 内的
> `mod`，`cargo test` 直接在二进制目标上跑。

## 热部署 reload

`tekl reload <file>`（`src/compiler/generator.rs` + `src/termux/`）：

1. **编译**：读取 `.tekl` → `keys::compile_source` 得 `extra-keys` JSON 行（含警告）。
2. **原子写盘**：替换/追加 `~/.termux/termux.properties` 中 `extra-keys = ...` 行；
   先写临时文件再 rename，写前备份 `*.tekl.bak`，写后读回校验。
3. **生效**：调用 `termux-reload-settings`；失败仅告警不致命（文件已写入）。

`--no-reload` 跳过第 3 步。

## 架构与契约

### 前后端分离硬契约

- **Rust 核心 = 前端 UI**：渲染、事件读取、输入层归一化 —— **不写任何策略**。
- **完整 API**：细粒度 read/write 原语与不可再分的原子数据操作
  （`set_cursor` / `insert_text` / `add_row` / `save_file` / `apply_edit` …）。
- **rhai 插件 = 全部操作逻辑**：守卫、路由、组合都写在各 `.rhai` 的 `run` 闭包里
  （如 `if cursor_row() > 0 { set_cursor(...) }`）。
- **事件层不实现操作**：阶段一短暂持锁产出待动作键名/ID，阶段二在无锁状态下
  dispatch → Rhai 闭包执行。布局 TUI 插入菜单 Enter 只返回动作 ID，不直接改数据。

> 改行为 = 改 `.ttoll` / `.rhai`（或加核心原语）；核心事件分支禁止写操作逻辑。

### 直接执行模型

`run` 闭包经引擎内注册的原语**直接读写** `Arc<Mutex<状态>>`，无中间操作/回放层
（`src/worker/script_runtime.rs`）：

```rust
let shared = Arc::new(Mutex::new(app));
let host = ScriptHost { engine: tui_engine(shared.clone()).engine, lib };
// …事件归一化后调 host.call(run)…
```

状态所有权归 UI，全部注册原语捕获句柄克隆，引擎内部自行加锁。

### 核心 API 原语

布局 TUI（`app.rs::tui_engine`）：

| 类别 | 原语 |
|---|---|
| 读 | `cursor_row` `cursor_col` `row_count` `col_count` `selected_row` `selected_col` |
| 写 | `set_cursor` `set_selected` `clear_selection` |
| 原子操作 | `add_row("before"/"after")` `add_col(…)` `del_row` `del_col` `swap_row` `swap_col` `swap_key` |
| 系统/模式 | `set_mode(0x01/0x02/0x03)` `exit` `save_file` `apply_edit` |
| UI 控制 | `show_matrix` `input_value` `input_name` |

ttoll 编辑器（`editor.rs::ed_engine`）：

| 类别 | 原语 |
|---|---|
| 读 | `cursor_row` `cursor_col` `row_count` `line_len` |
| 写/编辑 | `set_cursor` `insert_text` `backspace` `delete_char` `delete_line` `new_line("above"/"below")` `split_line` |
| 模式/面板 | `set_mode` `toggle_panel("id"/"help")` `cancel_cmd` |
| 命令/文件 | `exec_cmd` `save(true/false)` `exit` |

## 测试

```bash
cargo test
```

覆盖：tekl 词法/语法/round-trip、ttoll 解析与文法冻结（拒绝 `CTRL+C` 等字符级和弦）、
ID 注册池、Rhai 脚本加载、布局矩阵行/列/移动/交换等纯数据测试（当前 118 通过）。
交互（ratatui raw mode）靠实机验证。

## 许可

GPL-3.0-or-later（见 `LICENSE`）。