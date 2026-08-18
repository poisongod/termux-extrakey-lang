# tjek — Termux Json Extended key language

> 注: **合并前的旧文档** (保留参考)。工具链现为单文件插件 `lib/check.mjs`
> (子命令 `termux-extrakey tjek <file|->` 不变); 以 `docs/design.md` 与测试为准。

tjek 是 Termux 扩展键盘 JSON 的解析/校验工具链,复刻 Android `org.json.JSONTokener` 的宽松语法,用于直接解析与校验 termux.properties 中手写的 `extra-keys` JSON。

- **JSON DSL 解析器** 与 Android `JSONTokener` 语义一致:key/值可省略引号、支持单双引号、数字、布尔、`\u` 转义、尾逗号
- **四字段结构验证** 对照 Termux 源码 `ExtraKeyButton` 的约束(key/macro 互斥、单层 popup、display 字符串)
- **完整错误报告** 源码片段 + 行列定位 + 修复提示

## 使用

```bash
termux-extrakey tjek <file | ->   # '-' 从 stdin 读取
```

## 宽松 JSON 语法

```bash
termux-extrakey tjek - <<'EOF'
[
  [{key: ESC, popup: {macro: "CTRL f d", display: "tmux exit"}},
   {key: TAB, popup: {macro: "ALT a", display: A-a}}]
]
EOF
# OK: valid termux extra-keys JSON DSL
```

错误报告带源码片段、行列定位与修复提示:

```
SyntaxError: tjek: expected ',' or '}', got ']' (line 3, column 1)
  location: line 3, column 1 (char 43)
    3 | ]
      | ^
  hint: fix the JSON structure
```

## 架构

```
lib/tjek/
├── json-dsl.mjs       # Android JSONTokener 兼容的宽松 JSON 解析器
├── validate-json.mjs  # 四字段结构验证 (ExtraKeyButton 约束)
├── error-report.mjs   # 完整错误报告 (源码片段+行列+hint)
└── cli.mjs            # termux-extrakey tjek 子命令
```
