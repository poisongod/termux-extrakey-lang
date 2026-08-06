# PR (draft for discussion)

> **title**: `feat(extra-keys): author extra-keys with the tekl DSL, bypassing JSON`

---

## 0 TL;DR

- Adds an optional `extra-keys-tekl` property holding tekl source.
- Ships a ~300-line pure-Kotlin compiler producing Termux's internal
  `ExtraKeyButton` model directly — **no JSON round-trip**.
- `extra-keys` (JSON) keeps working unchanged as a legacy read-only path.
- tekl is rejected at compile time with line+column; rendering is untouched.
- Zero new dependencies; same GPLv3.

---

## 1 Why now: the evidence is already in this repo's issue tracker

### 1.1 The maintainer already called the current format unfixable

**#4589** ("extra-keys of termux.properties malfunctioning", open):

> "The backslash handling of `extra-keys` is glitchy (demonstrable by `"\u005c"` alike).
> But **likely unfixable at this point**." — maintainer

The author of #4386 tried `'\\'`, `"\\\\"` and `"\u005c"` — all error — just to get one
`\` key. The accepted solution is to memorize the magic keyword `BACKSLASH`.

**Root cause**: `extra-keys` is JSON buried inside a `.properties` double-escaping
context. Two escaping languages collide inside one string. Any "fix" to the JSON path is
another patch in that same maze — which is why the maintainer says it is not fixable.

**tekl has no escapes**: keys are identifiers (`ESC`, `TAB`, `CTRL`…). The class of bugs
in #4386/#4589 literally cannot exist, because there is no embedded string/JSON layer.

### 1.2 The maintainer already prescribes the alternative — let's do it properly

**#3601** ("Support for multiple sets of extra-keys", feature request):

> "For now you can also create a **script yourself** that dynamically sets required
> `extra-keys` value in properties file and runs `termux-reload-settings`."

Users want dynamic/complex keyboards. The recommended workaround is a hand-rolled bash
script doing `sed` on `termux.properties`. That is exactly what a proper toolchain should
replace:

| Maintainer's workaround today | tekl proposal |
| --- | --- |
| user-written bash + `sed` on JSON string | `termux-extrakey-tekl keyboard.tekl --watch` |
| string surgery, no validation | compiler: parse + semantic checks + warnings |
| JSON error at load, no position | compile error with line/column + caret |
| one-off, unmaintainable | a language, a compiler, a CLI — reusable, testable |

This PR ships the compiler itself; the CLI (`termux-extrakey-tekl`, `--watch`,
`--reload`) is the existing companion tool from the same language project.

### 1.3 The community teaches around it

Third-party tutorials (e.g. blog.vinfall.com) tell readers the *official sample* fails
with a JSON parse error after `termux-reload-settings`; every guide ends with a
memorized-correct snippet. Extra-keys authoring is the most user-visible, most
error-prone configuration surface Termux has — and it currently teaches by fire.

---

## 2 Why a DSL, and why this DSL

### 2.1 `extra-keys` is a small language already

A keyboard layout is a matrix; each cell has a tiny fixed semantics: plain key / combo /
label / popup. That is a DSL by nature. JSON describes it only with 3–5× overhead,
no comments, and no load-time diagnostics. Key-binding configs in vim/zsh/ssh are DSLs —
not JSON — for the same reason.

### 2.2 tekl is minimal and Termux-native

```
N ::= "ESC" | "TAB" | ... | [A-Za-z0-9_]*   // plain keys
S ::= "CTRL" | "ALT" | "FN" | "SHIFT"        // modifiers
A ::= N | S | A(A) | A:N
M ::= A | A.A | M M | M'\n' | M';\n'
```

| written | meaning | internal model |
| --- | --- | --- |
| `ESC` | plain key | `"ESC"` |
| `CTRL(C)` | combo | `{"macro":"CTRL C","display":"CTRL C"}` |
| `HOME:Start` | label | `{"key":"HOME","display":"Start"}` |
| `ESC.F1` | popup | `{"key":"ESC","popup":"F1"}` |
| `CTRL(C):copy.F5` | combo+label+popup | `{"macro":"CTRL C","display":"copy","popup":"F5"}` |
| `;` | new row | — |

Every reserved word is an existing Termux key name; the grammar is four productions and
the compiler is a pure function (~300 lines, no Android API, unit-testable).

### 2.3 Author-facing checks that JSON cannot offer

- **Rejects at compile time** (line + column + caret): unbalanced `(`, popup-in-popup,
  duplicate modifier in a row (`CTRL CTRL`).
- **Warns** (non-fatal): empty label, uneven row lengths, missing `DRAWER`/`KEYBOARD`.
- Failures surface via the existing toast + `Logger` path.

### 2.4 Future-proof: multiple keyboard sets

#3601 asks for multiple `extra-keys` sets. A DSL solves this structurally — named
configs/blocks in files — instead of inventing `extra-keys-1..9` property hacks. The
maintainer already said the idea is "not bad for future"; tekl is the vocabulary that
future can be built in.

---

## 3 Why this is cheap for the maintainer

- **JSON path untouched**: `ExtraKeysInfo`'s `new JSONArray(...)` path stays exactly as is;
  legacy configs keep working; no migration script.
- **Rendering untouched**: `ExtraKeysView` sees the same model it sees today.
- **Compiler is isolated**: a pure function in `termux-shared` (reusable by plugins, a
  future TUI, and the existing `termux-extrakey-tekl` CLI), covered by unit tests
  (tokenizer / parser / compiler / model mapping / error & warning matrices).
- **Opt-in**: empty/absent `extra-keys-tekl` ⇒ behavior byte-identical to today.
- **Already battle-tested**: the compiler exists (JS reference in
  `poisongod/termux-extrakey-lang`, GPLv3, 59 passing tests) — this PR is a port, not a
  greenfield language design.

## 4 Scope / files / success

```
termux-shared/.../TermuxPropertyConstants.java        +KEY_EXTRA_KEYS_TEKL
termux-shared/.../extrakeys/ExtraKeysInfo.java        tekl branch → List<ExtraKeyButton>
termux-shared/.../extrakeys/tekl/*.kt                  compiler (tokenizer/parser/compiler)
app/src/main/java/.../terminal/io/TermuxTerminalExtraKeys.java   read + dispatch
app/src/test/...                                        tekl test suite
```

Success:

- Same keyboard in tekl is readable/writable in ~5 min, appears after
  `termux-reload-settings`.
- Typing `CTRL(C` yields a positioned diagnostic, not a silent empty row.
- All existing JSON configs unchanged; CI green.

## 5 Open questions

1. Dedicated `extra-keys-tekl` key vs value-prefix detection? (proposal: dedicated key)
2. Keep JSON path read-only (proposal) vs drop?
3. Sync strategy between Kotlin port and the JS reference?
4. Diagnostics language: English (proposal) vs i18n?