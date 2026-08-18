# tekl — Termux Extra Key Language

> 注: **合并前的旧文档** (保留参考)。工具链现为单文件插件 `lib/keys.mjs`
> (子命令 `termux-extrakey tekl <file>` 不变), 布局源码示例为 `keys.layout`; 以 `docs/design.md` 与测试为准。

tekl 是描述 Termux 扩展键盘 (`extra-keys`) 的领域语言及工具链:解析器、编译器与 CLI。

- **tekl DSL** → 宽松 JSON DSL (`extra-keys = [...]`, 写入 `~/.termux/termux.properties`)
- **四字段结构验证** 编译期对照 Termux 源码 `ExtraKeyButton` 的约束 (key/macro 互斥、单层 popup、display 字符串)
- 配套 **tjek** (JSON DSL 解析/校验工具链) 见 `lib/tjek/README.md`

## 安装

```bash
# Node.js 26.3+ (Termux: pkg install nodejs-lts)
npm install
npm link   # 提供 termux-extrakey 唯一命令 (tekl 为子命令)
```

## 快速开始

```bash
termux-extrakey tekl keys.tekl                     # 编译并写入 ~/.termux/termux.properties
termux-extrakey tekl keys.tekl --output my.props   # 写入自定义文件
termux-extrakey tekl keys.tekl --reload            # 写入后调用 termux-reload-settings
termux-extrakey tekl keys.tekl --watch             # 监听文件, 改动自动重编译写入, 成功即退出
```

## 语法 (tekl.ebnf)

```
N ::= "ESC" | "TAB" | ... | "SPACE"
    | '"' , { [^\n] } , '"'
    | "'" , { [^\n] } , "'";
S ::= "CTRL" | "ALT" | "FN" | "SHIFT";
A ::= N | S | A(A) | A:N
M ::= A | A.A | M M | M'\n' | M';\n'
```

注释中变量模板 (`` `A` `` / `` `N` `` 为占位符, 由语法中对应位置的值替换):

```
A ::= N           (* A *)
    | S           (* S *)
    | A(A)        (* {macro: "A A"} *)
    | A:N         (* {key: A, display: N} *)

M ::= A           (* A *)
    | A.A         (* {key: A, popup: A} *)
    | M M         (* [A, A] *)
    | M'\n'       (* \n *)
    | M';\n'      (* M],' \ \n'[ *)
```

| 写法 | 语义 | 编译结果 |
| --- | --- | --- |
| `ESC` | 普通单键 | `"ESC"` |
| `CTRL` | 特殊修饰键单键 | `"CTRL"` |
| `"~"` | 引号字面键 (任意字符, 非换行) | `"~"` |
| `CTRL(C)` | 组合键 (外层修饰内层, 可嵌套) | `{macro: "CTRL C"}` |
| `CTRL(ALT(X))` | 嵌套组合 → 展平宏 | `{macro: "CTRL ALT X"}` |
| `ESC:quit` | 键名重命名 (显示文本, 标签不用引号) | `{key: "ESC", display: "quit"}` |
| `CTRL(C):copy` | 组合键重命名 | `{macro: "CTRL C", display: "copy"}` |
| `ESC.F1` | 弹出键 (向上滑) | `{key: "ESC", popup: "F1"}` |
| `CTRL(C).F5` | 组合键 + 弹出 | `{macro: "CTRL C", popup: "F5"}` |
| `;` | 键盘行分隔 | 新一行 |
| `\n` | 代码换行 (不切行) | — |

- 修饰键 (CTRL/ALT/FN/SHIFT) 与命名键大小写不敏感, 编译时规范为大写
- 普通标识符 (如被修饰的 `d`) 保留原样
- 组合键默认无 `display` 字段; 只有显式 `:label` 才生成

示例 `keys.tekl`:

```
ESC TAB CTRL ALT;
CTRL(C) ALT(A) HOME:Start ESC.F1;
```

等价 DSL (宽松 JSON, 字段名与标识符值裸写, 其余引号):

```
[[ESC, TAB, CTRL, ALT],[{macro: "CTRL C"},{macro: "ALT A"},{key: HOME, display: Start},{key: ESC, popup: F1}]]
```

## 架构

```
lib/tekl/
├── tokenizer.mjs      # tekl DSL 词法 (N/S 键, '()', ':', '.', ';', 引号字面量)
├── parser.mjs         # EBNF 递归下降, ParseError + formatParseError
├── compiler.mjs       # AST -> ExtraKeyButton JSON, CompileError + 警告
├── compiler-io.mjs    # compileSource / updateProperties (原子写+验证) / reload
├── dsl-stringify.mjs  # 宽松 JSON DSL 序列化 (字段/标识符值裸写)
├── jit.mjs            # --watch 热重载 (成功即退出)
└── cli.mjs            # termux-extrakey tekl 子命令
```

## 测试

```bash
npm test   # 76 用例: tekl EBNF 逐产生式 / tjek JSON DSL / 结构验证
```

## 约束 (与 Termux 一致)

- 一行内根按键为特殊键 (CTRL/ALT/FN/SHIFT) 时不可重复
- `key` 与 `macro` 互斥 (ExtraKeyButton)
- `popup` 仅一层, 内部不可再嵌套 popup
- 组合键中的键只能使用 `N`/`S` (重命名/弹出不可放入 `()`)
