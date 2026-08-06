# tekl — Termux Extra Key Language

tekl 是描述 Termux 扩展键盘(`extra-keys`)的领域语言及其工具链:解析器、编译器与 CLI。

- **tekl DSL** → 编译为 Termux 使用的 JSON(`extra-keys = [...]`,写入 `~/.termux/termux.properties`)
- **四字段结构验证** 编译期对照 Termux 源码 `ExtraKeyButton` 的约束(key/macro 互斥、单层 popup、display 字符串)
- 配套 **tjek**(JSON DSL 解析/校验工具链)见 `lib/tjek/README.md`

## 安装

```bash
# Node.js 26.3+ (Termux: pkg install nodejs-lts)
npm install
npm link   # 提供 termux-extrakey-tekl 与 termux-extrakey-tjek 命令
```

## 快速开始

```bash
termux-extrakey-tekl keys.tekl                     # 编译并写入 ~/.termux/termux.properties
termux-extrakey-tekl keys.tekl --output my.props   # 写入自定义文件
termux-extrakey-tekl keys.tekl --reload            # 写入后调用 termux-reload-settings
termux-extrakey-tekl keys.tekl --watch             # 监听文件, 改动自动重编译写入
```

## 语法 (tekl.ebnf)

```
N ::= "ESC" | "TAB" | ... | "SPACE" | [A-Za-z0-9_]*
S ::= "CTRL" | "ALT" | "FN" | "SHIFT"
A ::= N | S | A(A) | A:N
M ::= A | A.A | M M | M'\n' | M';\n'
```

| 写法 | 语义 | 编译结果 |
| --- | --- | --- |
| `ESC` | 普通单键 | `"ESC"` |
| `CTRL` | 特殊修饰键单键 | `"CTRL"` |
| `CTRL(C)` | 组合键(外层修饰内层, 可嵌套) | `{"macro":"CTRL C","display":"CTRL C"}` |
| `CTRL(ALT(X))` | 嵌套组合 → 展平宏 | `{"macro":"CTRL ALT X",...}` |
| `ESC:quit` | 键名重命名(显示文本) | `{"key":"ESC","display":"quit"}` |
| `CTRL(C):copy` | 组合键重命名 | `{"macro":"CTRL C","display":"copy"}` |
| `ESC.F1` | 弹出键(向上滑) | `{"key":"ESC","popup":"F1"}` |
| `CTRL(C).F5` | 组合键 + 弹出 | `{"macro":"CTRL C","display":"CTRL C","popup":"F5"}` |
| `;` | 键盘行分隔 | 新一行 |
| `\n` | 代码换行(不切行) | — |

示例 `keys.tekl`:

```
ESC TAB CTRL ALT;
CTRL(C) ALT(A) HOME:Start ESC.F1;
```

等价 JSON:

```json
[["ESC","TAB","CTRL","ALT"],[{"macro":"CTRL C","display":"CTRL C"},{"macro":"ALT A","display":"ALT A"},{"key":"HOME","display":"Start"},{"key":"ESC","popup":"F1"}]]
```

## 架构

```
lib/tekl/
├── tokenizer.mjs      # tekl DSL 词法 (N/S 键, '()', ':', '.', ';')
├── parser.mjs         # EBNF 递归下降, ParseError + formatParseError
├── compiler.mjs       # AST -> ExtraKeyButton JSON, CompileError
├── compiler-io.mjs    # compileSource / updateProperties (原子写+验证) / reload
├── jit.mjs            # --watch 热重载
└── cli.mjs            # termux-extrakey-tekl 命令
```

## 测试

```bash
npm test   # 53 用例: tekl EBNF 逐产生式 / tjek JSON DSL / 结构验证
```

## 约束 (与 Termux 一致)

- 一行内根按键为特殊键 (CTRL/ALT/FN/SHIFT) 时不可重复
- `key` 与 `macro` 互斥 (ExtraKeyButton)
- `popup` 仅一层, 内部不可再嵌套 popup
- 组合键中的键只能使用 `N`/`S`(重命名/弹出不可放入 `()`)