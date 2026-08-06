# PR title (草案)

> `feat(extra-keys): 支持 tekl DSL 作为 extra-keys 配置语言(替代手写 JSON)`
>
> EN title: `feat(extra-keys): add tekl DSL support for `extra-keys` configuration`

---

## Summary / 摘要

This proposal adds **tekl** (Termux Extra Key Language) as an authoring DSL for the
`extra-keys` keyboard configuration, replacing hand-written JSON. tekl is a compact,
compile-only language whose single compiler emits the exact JSON structure Termux
already understands today (`extra-keys = [...]`). The change supersedes the need to
hand-edit JSON in `termux.properties`.

## Motivation — why tekl instead of tjek

Hand-writing `extra-keys` JSON is the ergonomic bottleneck today:

- Quotes, commas and brackets must be balanced by hand; Android's `JSONTokener` is
  lenient on input but unforgiving when a value is half-edited — and JSON-style errors
  only surface at load time with no clear line.
- A single key needs verbose boilerplate: `{"macro":"CTRL C","display":"CTRL C"}`.
- The **tjek** toolchain (Termux Json Extended key language) *parses and validates*
  hand-written JSON — it fixes the symptom after the fact.
- **tekl** fixes the root cause: it is an authoring language with a compiler, so
  malformed keyboard definitions are caught at compile time (parse/compile errors carry
  line+column) and the emitted JSON is always valid by construction.

tekl → a compiler emits validated JSON; tjek is now only relevant as a debugging/
validation grab-bag for existing JSON configs, and is no longer needed for authoring.

### JSON vs tekl 对比

| JSON (now) | tekl (proposed) |
| --- | --- |
| `["ESC","TAB","CTRL","ALT"]` | `ESC TAB CTRL ALT;` |
| `[{"macro":"CTRL C","display":"CTRL C"}]` | `CTRL(C);` |
| `[{"key":"HOME","display":"Start"}]` | `HOME:Start;` |
| `[{"key":"ESC","popup":"F1"}]` | `ESC.F1;` |
| `[{"macro":"CTRL C","display":"copy","popup":"F5"}]` | `CTRL(C):copy.F5;` |

tekl grammar (see `tekl.ebnf`):

```
N ::= "ESC" | "TAB" | ... | [A-Za-z0-9_]*
S ::= "CTRL" | "ALT" | "FN" | "SHIFT"
A ::= N | S | A(A) | A:N
M ::= A | A.A | M M | M'\n' | M';\n'
```

- `A(A)` — combo → `{"macro":"CTRL C","display":"CTRL C"}`
- `A:N` — label → `display`
- `A.A` — popup
- `;` — separate keyboard rows
- blank `\n` — code line break (not a new row)

## Implementation strategy (in termux-app)

1. **Property**: add `extra-keys-tekl` to `TermuxPropertyConstants` (keep `extra-keys`
   unchanged and fully backward compatible; when `extra-keys-tekl` is present it wins,
   otherwise fall back to the existing JSON path).
2. **Compiler**: port the tekl compiler (tokenizer + parser + compiler, currently ~300
   lines of JS in `lib/tekl/`) to a pure-Kotlin class in `termux-shared`, so no JS
   runtime is required. Output is an `org.json.JSONArray` identical to today's.
3. **Integration point**: `TermuxTerminalExtraKeys.java` reads the property and hands it
   to `ExtraKeysInfo`; inside `ExtraKeysInfo`, branch on the property: if `tekl` → run
   the kotlin compiler, else keep the existing `new JSONArray(...)` path.
4. **Errors/warnings**:
   - Compile errors carry line + column and a caret snippet, surfaced via the existing
     toast+log path (`Could not load and set the "extra-keys" property ...`).
   - Compile-time warnings (uneven row lengths, missing DRAWER/KEYBOARD) logged, not fatal.
5. **Validation**: the compiled JSON goes through the existing `ExtraKeyButton`
   constraints (key/macro mutual exclusion, single-level popup, string display) before
   it is applied — same guarantee as JSON today.

## Files affected (estimate)

- `termux-shared/.../TermuxPropertyConstants.java` — new property key
- `termux-shared/.../extrakeys/ExtraKeysConstants.java` / `ExtraKeysInfo.java` — tekl branch
- `app/src/main/java/com/termux/app/terminal/io/TermuxTerminalExtraKeys.java` — read+dispatch
- new: `termux-shared/.../extrakeys/tekl/*` (Kotlin port of compiler)
- `docs/` + README — authoring guide for tekl

## Compatibility & migration

- `extra-keys` JSON continues to work unchanged.
- tekl is opt-in via the new property; no changes to stored user configs.
- Compiler emits JSON matches current Termux semantics (`macro` vs `key` vs `popup`).

## Open questions

- Property discovery: dedicated `extra-keys-tekl` key vs. value-prefix detection in
  `extra-keys`? (dedicated key preferred — avoids `=`/escape issues in `.properties`)
- Should the Kotlin port be code-generated from the JS reference (single source of
  truth), or maintained separately for now?
- Language of warnings/logs: English following existing Termux conventions.