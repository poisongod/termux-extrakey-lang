# tekl — Termux 键盘布局 DSL + TUI 工具链（Rust）

Termux extra-keys 的声明式配置语言、交互式 TUI 编辑器与热部署工具链。
目标：**完全替代 `termux.properties`**，改布局只碰一个 `.tekl` 文件。

> **当前架构 = Rust Core + Rhai ID 插件**，全仓库无 Node.js 依赖。

---

## 1. 双层受众

| 受众 | 面向 | 使用的东西 | 产出 |
|---|---|---|---|
| **普通用户** | 编辑键盘布局 | `tekl edit xxx.tekl`（布局 TUI） | `~/.termux/termux.tekl` → 编译写入 termux.properties |
| **插件开发者** | 定制行为/扩展操作 | `ttoll` 规则 + `<ID>.rhai` 插件 | 按键→ID 接线 + ID→逻辑实现 |

两层通过**十六进制 ID** 解耦：ttoll 只接线不实现，Rhai 只实现不接线。
普通用户永远不需要接触第二层。

## 2. 核心数据流

```
按键事件
   ↓ 事件归一化（Core: KeyEvent → 键名 token，和弦 CTRL+X）
ttoll dispatch（查 <consumer>.ttoll 规则表：模式+键名 → 动作ID）
   ↓
IdRegistry.eval_action(ID)          ← 仅查本消费者的注册池
   ↓
Rhai 引擎调用该 ID 的 run 闭包       ← src/id/<consumer>/<ID>.rhai
   ↓  闭包内调用宿主函数（exit/focus/save…），被记录为 HostOp 序列
apply_host_op 回放                   ← Core 把 HostOp 应用到 TUI 状态
   ↓
界面更新 / 写盘退出
```

**无硬编码原则**：TUI 的所有按键行为由 `.ttoll` 定义接线，所有操作语义由 `.rhai`
的 `run` 闭包定义。Core 只提供三样东西：事件归一化、dispatch 解释器、宿主函数。

## 3. 用户可见的文件

```
~/.termux/
├── termux.tekl        # 键盘布局源码（普通用户唯一需要关心的文件）
├── tekl.ttoll         # 布局 TUI 的行为规则      ┐
├── editor.ttoll       # ttoll 编辑器的行为规则    ├─ 插件开发者层
└── id/<ID>.rhai       # 用户 ID 插件扩展（出厂为空）┘
```

内置 ID 插件随二进制发布（编译期清单嵌入，见 `src/lib/id.rs` 的
`TUI_IDS` / `EDITOR_IDS`），开箱即用；用户目录只做**增量扩展，不覆盖内置**。

## 4. ID 插件规范（一 ID 一文件）

文件名即 ID。`src/id/tui/0x00.rhai`：

```rhai
// 退出/返回
#{
    desc: "退出/返回",
    run: || exit()
}
```

组合操作直接在 `run` 里写 Rhai：

```rhai
// src/id/editor/0x17.rhai — 保存退出
#{
    desc: "保存退出",
    run: || { save(true); exit() }
}
```

- `desc`：描述（编辑器左栏 ID 表显示）
- `run`：Rhai 闭包，即**实际实现**；调用宿主函数组合出操作序列

### 编号约定

- `0x00` 全局等同 = 退出（每个消费者的注册池都有）
- 其余 ID 各消费者**从 `0x01` 起**独立编号，互不共享、语义独立
- 布局 TUI 内置段：0x01–0x53；编辑器内置段：0x01–0x17（按使用场景频率排序）

### 宿主函数词汇表（Rhai ↔ Rust 接口）

| 分类 | 函数 |
|---|---|
| 系统 | `exit()` `set_mode(n)` |
| 矩阵 | `focus(dir)` `add_row(pos)` `add_col(pos)` `del_row()` `del_col()` `swap_row()` `swap_col()` `select_key()` `swap_key()` |
| 面板 | `show_matrix(kind)` `input_value(kind)` `input_name(kind)` `toggle_panel(kind)` |
| 编辑 | `cursor(dir)` `jump(t)` `delete_char()` `delete_line()` `new_line(pos)` `insert_text(s)` `backspace()` `exec_cmd()` `cancel_cmd()` |
| 文件 | `save(validate)` `apply()` |

宿主调用被记录为类型化 `HostOp` 序列（记录-回放模式），由消费者同步应用到自身
状态——脚本求值无副作用，无需跨线程共享 UI 状态。

## 5. ttoll 行为规则（文法已冻结）

权威文法：`ebnf/ttoll.ebnf`。键词汇固定为 **K ∪ D 共 9 个**：

```
K = ESC ALT CTRL SPACE ENTER        D = UP DOWN LEFT RIGHT
M = 0x01 | 0x02 | 0x03              ID = 0x + 两位十六进制
C = M ',' 步序列 | 步序列            步 = KEY | CTRL+X 和弦 | '{'集合'}' | KEY*
R = C '>' ID                        P = { R '\n' }
```

示例（`editor.ttoll`）：

```
CTRL+C>0x00          (* 全局：退出 *)
0x01,LEFT>0x01       (* Normal 模式：← 移动光标 *)
0x01,ENTER>0x07      (* 进入插入模式 *)
0x01,CTRL+LEFT>0x0D  (* 和弦：删除字符 *)
```

不在词汇表内的键（如字母键）不属于文法——输入层的字符输入/退格/Tab 属于
Insert 模式的"打字"，不经过 ttoll。

## 6. 目录地图

```
src/
├── main.rs                 入口：tekl [edit|reload]
├── id/
│   ├── tui/<ID>.rhai ×25   内置 ID 插件（布局 TUI）
│   └── editor/<ID>.rhai ×24 内置 ID 插件（ttoll 编辑器）
├── lib/                    ★ 共享核心
│   ├── keys.rs             tekl 词法/语法/编译/打印（含往返一致性约束）
│   ├── behavior.rs         ttoll 解析 + dispatch/dispatch_chord + 静态检查
│   ├── id.rs               IdRegistry：清单嵌入、用户扫描、eval_action
│   ├── keyset.rs           按键词汇表
│   ├── check.rs            tjek 校验报告
│   ├── validate.rs         四字段 JSON 校验
│   ├── json_dsl.rs         Android JSONTokener 兼容解析
│   └── user_dir.rs         ~/.termux 目录规范
├── tui/
│   ├── app.rs              布局 TUI（消费 tui.rhai + tekl.ttoll）
│   ├── editor.rs           ttoll 编辑器：双分区（左 ID 表｜右内容）
│   ├── mode.rs             模式枚举 0x01/0x02/0x03
│   ├── keymap.rs preview.rs render.rs
├── worker/
│   └── script_runtime.rs   ★ Rhai 引擎 + HostOp 记录-回放 + 宿主注册
├── compiler/generator.rs   reload 子命令：编译 .tekl 并写盘
└── termux/writer.rs        termux.properties 原子写（extra-keys 行替换）

keys.layout                 布局样例（出厂模板）
behavior.base               TUI 规则基座模板（首次运行复制到 ~/.termux/ui/init.ttoll）
```

## 7. 构建 / 安装 / 测试

```bash
cargo test              # 单元测试（纯数据层为主，TUI 交互靠实机验证）
cargo build --release
cp target/release/tekl $PREFIX/bin/tekl    # Termux 安装
```

> Termux 注意：若 `~/.cargo/bin` 里有 rustup 空壳（无 toolchain）会遮蔽系统
> cargo/rustc，用 `PATH=/data/data/com.termux/files/usr/bin:$PATH cargo …`
> 或卸载 rustup shim。

```bash
tekl                    # 默认启动布局 TUI
tekl edit <file>        # .tekl → 布局 TUI；.ttoll → 双分区行为编辑器
tekl reload [--no-reload]  # 编译 .tekl → termux.properties → termux-reload-settings
```

编辑器分区：左栏 ID 表（只读浏览，↑↓ 滚动），右栏可编辑内容；
`ALT+←/→` 切换焦点。Normal 模式按键全部由 `~/.termux/editor.ttoll` 接线。

## 8. 测试与质量约定

- `cargo test`：解析器、dispatch、ID 注册池、Rhai 记录-回放均有纯数据测试
- 往返一致性：`keys.rs` 序列化后必须能被同一解析器还原（round-trip 契约）
- 文法冻结：修改 `ebnf/ttoll.ebnf` 视同破坏性变更，需同步 `behavior.rs` 与全部 `.ttoll`
- 行为调整的正确姿势：**只改 `.ttoll` / `.rhai` 数据文件，不改源码**

## 9. 路线图

1. **ttoll JIT 真实现**：目标为由 `ebnf/ttoll.ebnf` 驱动的规则编译 + 静态检查前置
   （现由 `behavior.rs` 手写递归下降承担）
2. **规则/插件热加载**：编辑器运行中监听 `.ttoll` 与 `id/*.rhai` 变更，保存即生效
3. Worker 隔离落地：action 执行超时与 panic 隔离
