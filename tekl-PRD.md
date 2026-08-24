# Tekl 完整项目设计方案

## 一、项目定位与受众分层

Termux 键盘布局的声明式配置语言 + 交互式 TUI 编辑器 + 实时热部署系统。
tekl 完全替代 termux.properties，成为 Termux 的唯一配置文件。

**双层受众**：

| 受众 | 面向 | 工具 | 产出 |
|---|---|---|---|
| 普通用户 | 键盘布局的日常编辑 | tekl DSL + 布局 TUI | `~/.termux/termux.tekl` → 编译 → `termux.properties` |
| 插件开发者 | 行为定制与新操作扩展 | ttoll 规则 + `.rhai` ID 插件 | `*.ttoll`（按键→ID 接线）+ `*.rhai`（ID→逻辑实现） |

普通用户不需要接触 ttoll 与 .rhai；插件开发者不需要理解 Termux 属性细节。
两层通过**十六进制 ID** 解耦：ttoll 只接线，Rhai 只实现。

## 二、核心设计哲学

| 原则 | 说明 |
|---|---|
| 完全替代 | tekl 代替 termux.properties |
| 零认知负担 | DSL 语法接近自然描述；普通用户只见 tekl 一层 |
| 实时反馈 | 任何修改立即渲染到 Termux 键盘 |
| 比例自适应 | 按键宽度随终端尺寸自动调整 |
| 双向通路 | 手写配置与 TUI 编辑等价，可互相转换 |
| 统一 Rust | Rust Core 承载一切运行时逻辑，脚本层用纯 Rust 的 Rhai 引擎执行，无外部 Node.js 依赖 |
| 分层受众 | tekl 面向用户，ttoll/rhai 面向插件开发者 |
| 内置 + 扩展 | 内置 ID 插件随二进制发布保证开箱即用；用户目录只做增量扩展 |

## 三、DSL 定义

### 3.1 tekl.ebnf（键盘布局语法）

```
Termux Extra key Language (tekl)
EBNF Standard: ISO/IEC 14977

(* N：按键名 *)
(* S：修饰键 *)
(* A：按键动作 *)
(* G：Termux 全局配置项 *)
(* M：键盘矩阵 *)
(* P：完整 tekl 文件 *)

N = 'ESC' | 'TAB' | 'HOME' | 'END' | 'PGUP' | 'PGDN'
  | 'INS' | 'DEL' | 'BKSP'
  | 'UP' | 'LEFT' | 'RIGHT' | 'DOWN'
  | 'ENTER' | 'BACKSLASH' | 'QUOTE' | 'APOSTROPHE'
  | 'F1' | … | 'F12'
  | 'KEYBOARD' | 'DRAWER' | 'SCROLL' | 'SPACE' | 'PASTE'
  | '"' { [^\n] } '"'
  | "'" { [^\n] } "'"

S = 'CTRL' | 'ALT' | 'FN' | 'SHIFT'

A = N | S | A '(' A ')' | A ':' N

G = 'allow-external-apps' '=' ( 'true' | 'false' )
  | 'default-working-directory' '=' '"' { [^\n] } '"'
  | 'hide-soft-keyboard-on-startup' '=' ( 'true' | 'false' )
  | 'soft-keyboard-toggle-behaviour' '=' ( 'enable' | 'disable' )
  | 'terminal-transcript-rows' '=' [0-9]+
  | 'volume-keys' '=' ( 'volume' | 'keyboard' | 'off' )
  | 'extra-keys-style' '=' ( 'default' | 'arrows-only' | 'arrows-all' | 'all' | 'none' )
  | 'extra-keys-text-all-caps' '=' ( 'true' | 'false' )
  | … （完整清单见 ebnf/tekl.ebnf）

M = A | A '.' A | M M | M '\n' | M ';\n'

P = { G '\n' } { M }
```

### 3.2 ttoll.ebnf（行为规则语法 — 插件开发者层）

```
Tekl Tui Operational Logic Language (ttoll)
EBNF Standard: ISO/IEC 14977

K = 'ESC' | 'ALT' | 'CTRL' | 'SPACE' | 'ENTER'
D = 'UP' | 'DOWN' | 'LEFT' | 'RIGHT'
M = '0x01' | '0x02' | '0x03'
ID = /0x[0-9A-Fa-f]{2}/
C = K ',' C | K | '{' C '}'
R = M ',' C '>' ID
  | M ',' D '>' ID
  | C '>' ID
  | D '>' ID
P = { R '\n' }
```

规则语义：`模式, 按键序列 > 动作ID`。无前缀模式的规则为全局生效。
ID 是纯标识符——ttoll 只负责"什么键触发什么 ID"，不关心 ID 做什么。

## 四、模式对应关系

| ttoll M 值 | 含义 | 操作对象 | 对应 tekl 规则 |
|---|---|---|---|
| 无前缀 | 全局规则 | 任意模式 | — |
| 0x00 | 系统/退出 | 应用级动作 | — |
| 0x01 | 键盘模式 | 整个键盘矩阵（行/列） | `M` |
| 0x02 | 主键模式 | 弹出键的主键 | `A '.' A` 左侧 |
| 0x03 | 弹出键模式 | 弹出键的弹出内容 | `A '.' A` 右侧 |

各消费者（见 §7）在自身注册池内约定各自的模式段编号。

## 五、文件路径

| 路径 | 归属 | 说明 |
|---|---|---|
| `~/.termux/termux.tekl` | 用户 | 主配置文件（普通用户） |
| `~/.termux/tekl.ttoll` | 开发者 | 布局 TUI 的行为规则 |
| `~/.termux/editor.ttoll` | 开发者 | ttoll 编辑器的行为规则 |
| `~/.termux/id/<ID>.rhai` | 开发者 | 用户 ID 插件（一 ID 一文件，出厂为空） |
| `src/id/tui/<ID>.rhai` ×25 | 内置 | 布局 TUI 内置 ID 插件（编译期清单嵌入） |
| `src/id/editor/<ID>.rhai` ×24 | 内置 | ttoll 编辑器内置 ID 插件（编译期清单嵌入） |

**一个 ID = 一个 .rhai 文件，文件名即 ID**：`0x00.rhai` 只含 0x00 的注册与实现。

## 六、ID 插件系统（Rhai）

### 6.1 核心原则

- **统一 Rust**：插件由内嵌 Rhai 引擎（纯 Rust）执行，无外部 Node.js/V8 依赖
- **一个 ID 必须对应一个操作逻辑**：插件映射中每个 `"0xHH"` 必须携带 `action`
- **按消费者隔离注册池**：每个消费者只加载自己的 `.rhai` 与 `.ttoll`，无共享 ID 池
  - 布局 TUI ← `tui.rhai` + `tekl.ttoll`
  - ttoll 编辑器 ← `editor.rhai` + `editor.ttoll`
- **内置 + 扩展分层**：
  - 内置插件位于项目 `src/id/*.rhai`，经 `include_str!` 编译进二进制，开箱即用
  - 用户目录 `~/.termux/id/*.rhai` 由开发者自行创建，**只添加新 ID，不覆盖内置条目**
- **纯标识符**：ID 不进行任何业务映射，语义完全由 action 定义

### 6.2 插件格式（一 ID 一文件）

每个 `.rhai` 文件对应一个 ID，文件名即 ID，内容为该 ID 的注册信息与实际实现：

```rhai
// src/id/tui/0x00.rhai — 退出/返回
#{
    desc: "退出/返回",
    run: || exit()
}
```

```rhai
// src/id/editor/0x76.rhai — 组合操作示例
#{
    desc: "保存退出",
    run: || { save(true); exit() }
}
```

字段约束：

| 字段 | 类型 | 必须 | 说明 |
|---|---|---|---|
| `desc` | 字符串 | 是 | 描述（TUI ID 面板显示） |
| `run` | Rhai 闭包 | 是 | 操作逻辑，调用宿主函数组合实现 |

**注册 + 实现同文件**：`run` 即实现本体，通过宿主函数词汇表（§6.4）组合出任意操作序列。

### 6.3 内置清单机制

内置 ID 不写死在 Core 里，而是以**编译期清单**声明（`src/lib/id.rs`）：

```rust
pub const TUI_IDS: &[(&str, &str)] = &[
    ("0x00", include_str!("../id/tui/0x00.rhai")),
    ("0x01", include_str!("../id/tui/0x01.rhai")),
    ...
];
```

- 增删一个内置 ID = 增删一个文件 + 清单一行，Core 逻辑零改动
- 清单遗漏文件 → 编译期报错（include_str! 强制存在）
- 用户插件 `~/.termux/id/<ID>.rhai` 同样文件名即 ID，运行时扫描加载，**只添加不覆盖**

### 6.3 ID 范围约定

| 范围 | 说明 |
|---|---|
| 0x00–0x04 | 系统保留（退出/模式切换/编辑应用） |
| 0x10–0x5F | 布局 TUI 内置段（tui.rhai 使用） |
| 0x60–0x7F | ttoll 编辑器内置段（editor.rhai 使用） |
| 0x80–0xFF | 插件开发者自由扩展 |

不同消费者的注册池相互隔离，同号在不同消费者中可各自定义。

### 6.4 宿主函数词汇表（Rhai ↔ Rust 接口）

Rust Core 向 Rhai 引擎注册以下宿主函数（`script_runtime.rs`）：

| 函数 | 签名 | 语义 |
|---|---|---|
| `exit()` | `()` | 退出当前消费者 |
| `set_mode(n)` | `(int)` | 切换模式状态 |
| `focus(dir)` | `(str)` | 矩阵焦点移动 up/down/left/right |
| `add_row(pos) / add_col(pos)` | `(str)` | before/after 增行增列 |
| `del_row() / del_col()` | `()` | 删行删列 |
| `swap_row() / swap_col()` | `()` | 交换行/列 |
| `select_key() / swap_key()` | `()` | 选中 / 交换键 |
| `show_matrix(kind)` | `(str)` | key/popup 值矩阵面板 |
| `input_value(kind) / input_name(kind)` | `(str)` | 值/名输入框 |
| `apply()` | `()` | 应用编辑并写盘 |
| `cursor(dir) / jump(target)` | `(str)` | 编辑器光标移动/跳转 |
| `delete_char() / delete_line() / backspace()` | `()` | 编辑器删除类操作 |
| `new_line(pos) / insert_text(s)` | `(str)` | 编辑器插入类操作 |
| `toggle_panel(kind)` | `(str)` | id/help 面板切换 |
| `exec_cmd() / cancel_cmd()` | `()` | 命令模式执行/取消 |
| `save(validate)` | `(bool)` | 保存（可校验语法） |

宿主函数由各消费者实现 `ActionHost` trait 注入；未注册函数在 Rhai 中调用即报错，天然故障隔离。

### 6.5 ID 执行流程

```
用户按键
    ↓
keyevent-token 归一化（和弦优先）
    ↓
ttoll dispatch(当前模式, 键名)
    ↓
命中规则 → 得到十六进制 ID
    ↓
IdRegistry.id_action(ID)        ← 仅查本消费者的注册池
    ↓
┌──────────────────────────────────┐
│ 有 action → Rhai 引擎求值         │
│             ↓                    │
│           调用宿主函数            │
│             ↓                    │
│           Rust 变更 TUI 状态      │
├──────────────────────────────────┤
│ 未注册 / 空 action → 忽略，无响应 │
└──────────────────────────────────┘
```

## 七、TUI 设计

### 7.1 启动行为

```bash
tekl              # 默认启动布局 TUI（普通用户入口）
tekl <file>       # .tekl → 布局 TUI；.ttoll → ttoll 编辑器（开发者入口）
```

### 7.2 两个消费者

| | 布局 TUI | ttoll 编辑器 |
|---|---|---|
| 受众 | 普通用户 | 插件开发者 |
| 编辑对象 | 键盘矩阵（所见即所得） | `.ttoll` 规则文本 |
| ID 插件 | `tui.rhai` | `editor.rhai` |
| 行为规则 | `tekl.ttoll` | `editor.ttoll` |
| 注册池 | 独立实例 | 独立实例 |

两者结构对称：同一套 dispatch 引擎、同一套 ActionHost 接口，仅词汇表不同。
ttoll 编辑器本身是"用 ttoll 驱动的 ttoll 编辑器"——自举验证行为语言完备性。

### 7.3 TUI 布局

| 区域 | 说明 |
|---|---|
| 顶部状态栏 | 当前模式、布局名称、终端尺寸 |
| 中间主区 | 键盘预览（布局 TUI）/ 规则文本（编辑器），实时渲染 |
| 底部操作栏 | 快捷键提示、编辑状态、错误信息 |

### 7.4 无硬编码原则

TUI 不包含任何硬编码操作逻辑。所有按键行为完全由 `.ttoll` 文件定义，
所有操作语义完全由 `.rhai` 插件的 action 实现。Rust Core 只提供：

1. 事件归一化（KeyEvent → ttoll 键名 token，和弦 CTRL+C 等）
2. dispatch 解释器（查表 → 守卫 → 执行）
3. 宿主函数（§6.4 词汇表的 Rust 实现）

`tekl.ttoll` 示例：

```
(* 全局规则 — 任何模式生效 *)
CTRL+C>0x00

(* 0x01 键盘模式 *)
0x01,ESC>0x00
0x01,UP>0x10
0x01,DOWN>0x11
0x01,LEFT>0x12
0x01,RIGHT>0x13
0x01,ENTER>0x02
0x01,SPACE>0x30

(* 0x02 主键模式 *)
0x02,ESC>0x01
0x02,ENTER>0x04

(* 0x03 弹出键模式 *)
0x03,ESC>0x01
0x03,ENTER>0x04
```

## 八、JIT 链路

```
ttoll.ebnf（语法定义）
        ↓
递归下降解析器（behavior.rs: tokenize_rule / parse_rules）
        ↓
静态检查（check_program：键/守卫/ID/可达性）
        ↓
dispatch 表构建（build_table：模式 → 规则集）
        ↓
驱动 TUI（dispatch / dispatch_chord）
```

## 九、比例自适应机制

### 9.1 按键比例自适应键盘比例

| 原则 | 说明 |
|---|---|
| 等宽均分 | 键盘总宽度 / 最大列数 |
| 不等宽比例 | `[2, 1, 2]` 表示 2:1:2 |
| 空位占位 | 计入均分但不显示 |

### 9.2 键盘比例自适应终端比例

| 终端变化 | 响应 |
|---|---|
| 宽度变化 | 键盘宽度自适应，按键等比缩放 |
| 高度变化 | 键盘垂直居中，行高自适应 |
| 极小 | 压缩到最小可读尺寸 |
| 极大 | 放大到舒适尺寸，添加内边距 |

## 十、脚本执行与隔离

```
action 求值（Rhai Engine）
        ↓
┌─────────────────────────────────┐
│ 成功 → 宿主函数变更状态          │
├─────────────────────────────────┤
│ 脚本错误 → 返回 Err，记录日志    │
│            TUI 状态不受损        │
├─────────────────────────────────┤
│ 未注册函数 → Rhai 原生报错       │
│            同样被捕获            │
└─────────────────────────────────┘
        ↓
主循环继续，下一次按键正常 dispatch
```

隔离保障：
- **类型安全**：宿主函数签名固定，参数类型不符即报错
- **无副作用逃逸**：Rhai 默认无 IO/网络能力，只能调用显式注册的宿主函数
- **状态不泄漏**：每次 action 求值为独立脚本上下文，无常驻全局态

## 十一、热部署流程

```
用户编辑完成（手写保存 / TUI 确认）
        ↓
编译器校验语法（validate_termux_json + validate_extra_key_json）
        ↓
校验失败 → 返回错误信息，不写入
校验成功 ↓
        ↓
分离 G 配置和 M 矩阵
        ↓
生成完整 termux.tekl 内容
        ↓
原子写入 ~/.termux/termux.tekl
        ↓
执行 termux-reload-settings
        ↓
Termux 键盘更新，完成 < 200ms
```

## 十二、项目文件结构

```
tekl/
├── src/
│   ├── main.rs                             # 入口：子命令分发（默认 TUI）
│   │
│   ├── compiler/                           # 编译器
│   │   ├── mod.rs
│   │   └── jit.rs                          # ttoll JIT（预留：EBNF 驱动解析）
│   │
│   ├── tui/                                # TUI 层
│   │   ├── mod.rs
│   │   ├── app.rs                          # 布局 TUI（消费 tui.rhai + tekl.ttoll）
│   │   ├── editor.rs                       # ttoll 编辑器（消费 editor.rhai + editor.ttoll）
│   │   └── mode.rs                         # 模式管理（0x01/0x02/0x03）
│   │
│   ├── id/                                 # 内置 ID 插件（include_str! 嵌入）
│   │   ├── tui.rhai                        # 布局 TUI 词汇表
│   │   └── editor.rhai                     # ttoll 编辑器词汇表
│   │
│   ├── worker/                             # 脚本运行时
│   │   ├── mod.rs
│   │   └── script_runtime.rs               # Rhai 引擎 + ActionHost + 宿主函数注册
│   │
│   └── lib/                                # 共享库
│       ├── mod.rs
│       ├── keys.rs                         # tekl 解析器/编译器/打印器
│       ├── keyset.rs                       # 按键词汇表
│       ├── behavior.rs                     # ttoll 解析/dispatch/check
│       ├── id.rs                           # IdRegistry（按消费者隔离，加载 .rhai）
│       ├── check.rs                        # tjek 校验报告
│       ├── validate.rs                     # 四字段 JSON 校验
│       ├── json_dsl.rs                     # JSONTokener 兼容解析
│       └── user_dir.rs                     # ~/.termux 目录规范
│
├── ebnf/                                   # 语法定义
│   ├── tekl.ebnf
│   └── ttoll.ebnf
│
├── keys.layout                             # 布局源码示例
├── Cargo.toml
├── Cargo.lock
├── README.md
└── LICENSE
```

## 十三、技术栈

| 组件 | 技术 |
|---|---|
| 语言 | Rust 2021 Edition |
| 解析 | ISO EBNF（自实现递归下降解析器） |
| TUI | ratatui + crossterm |
| 并发 | tokio 多线程运行时 |
| 脚本引擎 | rhai（纯 Rust 嵌入式脚本语言，用于 ID 插件） |
| 序列化 | serde + serde_json |

## 十四、设计总结

| 维度 | 设计方案 |
|---|---|
| 核心理念 | DSL + TUI + 热部署 + 故障隔离 + 完全替代 |
| 受众分层 | tekl 面向普通用户；ttoll + .rhai 面向插件开发者 |
| 替代目标 | tekl 完全代替 termux.properties |
| DSL 结构 | G（全局配置）+ M（键盘矩阵） |
| ttoll 模式 | 0x01=键盘模式 / 0x02=主键模式 / 0x03=弹出键模式；无前缀=全局 |
| ID 策略 | 纯标识符，不映射业务 |
| ID 注册池 | 每消费者独立实例，互不共享 |
| ID 插件 | 内置 `src/id/*.rhai`（编译期嵌入）+ 用户 `~/.termux/id/*.rhai`（只增不改） |
| 插件约束 | 一个 ID 必须对应一个操作逻辑 `{name, action}` |
| 脚本引擎 | rhai（统一 Rust，无外部 Node.js 依赖） |
| 宿主接口 | §6.4 词汇表，ActionHost trait 注入 |
| TUI 操作逻辑 | 由 `.ttoll` 定义接线、由 action 实现语义，Core 无硬编码 |
| 解析器 | ISO EBNF 自实现递归下降解析器 |
| 技术栈 | 纯 Rust + ratatui + crossterm + tokio + rhai |
| 用户入口 | `tekl` 命令，默认启动布局 TUI |
