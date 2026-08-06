# PR(title, 讨论稿)

> **title**: `feat(extra-keys): 以 tekl DSL 作为 extra-keys 作者语言(不经 JSON)`
> EN: `feat(extra-keys): author extra-keys with the tekl DSL, bypassing JSON`

---

## 0 这是什么样的改动

- 新增一个可选属性 `extra-keys-tekl`,承载 tekl 源码
- 内置一个 ~300 行的纯 Kotlin 编译器,把 tekl 直译成 **termux 内部 ExtraKeyButton 模型**
- 不经过 `JSONArray` 解析,不引入 tjek 层
- `extra-keys`(JSON)仍然原样工作,作为遗留只读路径
- 错误的 tekl 在**编译期**被拒(带行列),而非加载期抛 `JSONException`
- 全程不改变渲染/交互层任何一行行为

---

## 1 问题的本质:extra-keys 的书写体验是维护者必须面对的「积压技术债」

### 1.1 有来自用户的持续伤害,这不是"设计偏好"问题

- **#4386** (`extra-keys` 反斜杠无法使用,open):用户为要一个 `\` 键,写出 `'\\'` 报错、
  `"\\\\"` 报错、`"\u005c"` 报错,最终还是被 suggestion「直接用 `BACKSLASH` 关键字」。一个字符
  诉求,用户跟解析器搏斗数轮。
- **#4589**(2025-06):"extra-keys of termux.properties malfunctioning" 仍在 open,维护者
  亲自与企业 verse 的 `"\\\\"` 转义缠绕。
- 社区教程(blog.vinfall.com 等)都在教用户"官方示例贴进去会 JSON parse error",主流
  extra-keys 配置教程要靠各人自撞后才把正确格式背下来。

**根本原因**:extra-keys 的值是"terminator properties 双层转义"里的 JSON。两层语义
(properties 的 `\` + JSON 的 `\`)叠在一个字符串里,谁写谁错,正解只有查
`ExtraKeysConstants.java` 来背。

### 1.2 extra-keys 是"配置 DSL 的绝配",JSON 是委屈的容器

键盘布局本质上是一种**小型领域语言**:

- 它是**矩阵布局**(行=键盘行,col=键)
- 每个键只有少量语义:单键/组合/重命名/弹层
- 常量键名(`ESC` `TAB`...)、固定结构,完全可被 DSL 吸收

用 JSON(设计为「通用数据交换」)描述它=用笔写字王式的费用:

- 引号/逗号/号全都冗余
- 语义错误要等加载期
- 无法自然注释
- 长条目膨胀 3~5 倍,不可读不可 diff

### 1.3 同类项目的共识

- ssh/vim/zsh 的"按键绑定"都选 DSL,不是 JSON;
- 连 XDG/主机的 `.desktop` 也都是 key-value+块语法;
- 键盘配置因为**它是个小 DSL,它是作者听得懂的东西** —— tekl 就是这个 DSL,和现有渲染层
  同样全部通用(2.8c already `termux-shared` 的 agnostic)。

---

## 2 tekl 是什么(与 tjek 的界线)

- `tekl` = Termux Extra Key Language:一门"写键盘配置"的小语言,作用域:keyboard 矩阵。
- 语法面积极小,全部保留字即 Termux 已有的按键关键字。

把 `extra-keys` 那条 JSON 展开成 tekl 是自然降维:

```
# termux.properties (today)                          # termux.properties (proposed)
extra-keys = [[ "ESC", "TAB", "CTRL", "ALT" ],       extra-keys-tekl =
 [ { "macro": "CTRL C", "display": "CTRL C" } ]]        ESC TAB CTRL ALT;
                                                         CTRL(C);
```

### 语法表(参考 tekl.ebnf)

```
N ::= "ESC" | "TAB" | ... | [A-Za-z0-9_]*   // 普通键
S ::= "CTRL" | "ALT" | "FN" | "SHIFT"        // 特殊修饰键
A ::= N | S | A(A) | A:N
M ::= A | A.A | M M | M\n | M;\n
```

| 书写 | 语义 | 现有模型 |
| --- | --- | --- |
| `ESC` | 单键 | `"ESC"` |
| `CTRL(C)` | 组合 | `{"macro":"CTRL C","display":"CTRL C"}` |
| `HOME:Start` | 重命名 | `{"key":"HOME","display":"Start"}` |
| `ESC.F1` | 弹层 | `{"key":"ESC","popup":F1"}` |
| `CTRL(C):copy.F5` | 组合+重命名+弹层 | `{"macro":"CTRL C","display":"copy","popup":"F5"}` |
| `;` | 键盘行分隔 | — |

tekl 无 `.properties` 转义问题(键名=标识符,保留字),因此 #4386/#4589 一类
被两层转义反复折磨的场景**字面上不存在**(没有 JSON)。

### 编译期校验(视为"作者层的基础设施")

- 拒绝:`CTRL(C:` 括号不闭合、弹层内嵌弹层、一行重复特殊键 `CTRL CTRL`
- 警告:{空标签族, 行数不齐, 缺 `DRAWER`/`KEYBOARD`}
- 报错带行列+指针,加载失败可 `toast` + `Logger.logErrorWithStackTrace`

> 这一切在现有 JSON 路径里几乎没有可能。

---

## 3 落地最小公倍数

### 9.0 移动面小

```
termux-shared/.../TermuxPropertyConstants.java        +KEY_EXTRA_KEYS_TEKL
termux-shared/.../extrakeys/ExtraKeysInfo.java         tekl 分支 → 编译为 ExtraKeyButton
termux-shared/.../extrakeys/tekl/*.kt                  编译器(~新,3 文件:tokenizer/parser/compiler)
app/.../terminal/io/TermuxTerminalExtraKeys.java       读属性+分发
app/src/test/...                                        tekl 用例集
```

### 3.2 与现有代码的关系

- `extra-keys` 走 `ExtraKeysConstants` 现有 `new JSONArray(...)` 路径**一字不改**
- `ExtraKeysView` 渲染层无改动
- tekl 编译器是**纯函数**(字串 → List<ExtraKeyButton>), 无 Android api, 可单测可复用,
  甚至可放到 termux-shared 供插件 / 未来 TUI 编辑器共用 —— 对维护者友好

### 3.3 不做(明确范围外)

- 不设迁移脚本: 不再动用户既有 JSON
- 不删 JSON 路径: legacy 常开, 用户无感
- 不把 tekl 绑进 defaults(默认仍用 JSON)
- 不在本项目内讨论是否放弃 tjek —— tjek 是现存解析器工具, 与 tekl 独立

---

## 4 怎么验证 / 什么时候成功

- 对照组: 同一 keyboard 用 JSON 和 tekl 各写一遍, 后者 5 分钟全上手, `reload-settings` 后直接出现
- `CTRL(C` 手误时得到行列, 而非整个键盘静默
- 单测覆盖: tokenizer/parser/compiler/全产映射 + 错误/警告矩阵
- 兼容: 全部现有 extra-keys 配置保持原样, CI 回归通过

---

## 5 让它可被合并的 checklist

- [ ] 不新增依赖、: GPLv3 同源
- [ ] 编译器可独立构建/测试(`termux-shared` 模块级)
- [ ] 默认 `extra-keys-tekl` 为空时应与现状逐字节一致
- [ ] 新增属性的文档/建议命名由维护者 review

---

## Open questions

1. 属性入口:`extra-keys-tekl` vs `extra-keys` 值前缀?(建议独立键 —— 规避转义)
2. JSON(tjek)路径: 保留 legacy 只读 vs 全移除?(建议保留)
3. Kotlin 移植与 JS 参考(库仓库 `poisongod/termux-extrakey-lang`, GPLv3)的同步策略?
4. 错误/日志语言: 英 / i18n?(建议英, 与现行一致)