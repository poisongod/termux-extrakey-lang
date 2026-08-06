# termux-extrakey

Extra keys for Termux, powered by two languages:

- **tekl** — Termux Extra Key Language: 描述扩展键盘的 DSL,编译器生成 `extra-keys` JSON 写入 `~/.termux/termux.properties`
- **tjek** — Termux Json Extended key language: Android `JSONTokener` 兼容的宽松 JSON DSL 解析/校验工具链

## 命令

| 命令 | 作用 |
| --- | --- |
| `termux-extrakey-tekl <file.tekl> [--output <path>] [--reload] [--watch]` | 编译 tekl 并写入 termux.properties |
| `termux-extrakey-tjek <file \| ->` | 解析并校验手写 JSON DSL |

## 安装

```bash
npm install
npm link
```

## tekl 示例

```
ESC TAB CTRL ALT;
CTRL(C) ALT(A) HOME:Start ESC.F1;
```

## 文档

- [`lib/tekl/README.md`](lib/tekl/README.md) — tekl 语言与编译
- [`lib/tjek/README.md`](lib/tjek/README.md) — tjek 宽松 JSON 校验
- [`tekl.ebnf`](tekl.ebnf) — 语法定义

## 开发

```bash
npm test   # 53 用例: tekl EBNF 逐产生式 / tjek JSON DSL / 结构验证
```