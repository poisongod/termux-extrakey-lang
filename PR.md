# PR title (草案)

> `feat(extra-keys): 以 tekl DSL 替代 JSON(tjek)作为 extra-keys 配置语言`
>
> EN title: `feat(extra-keys): replace JSON (tjek) with tekl DSL for extra-keys`

---

## Summary / 摘要

This proposal replaces hand-written JSON with **tekl** (Termux Extra Key Language) as
the authoring language for the `extra-keys` keyboard configuration. tekl compiles
directly to Termux's internal button structure — there is **no intermediate JSON layer**
(tjek) any more. JSON remains supported only as a legacy/read-back path for existing
configs; it is no longer the authored format.

## Motivation — why tekl replaces tjek

Today `extra-keys` is authored as JSON and then *parsed*:

- Quotes, commas and brackets must be balanced by hand. The failure mode is a runtime
  `JSONException` at load time with no line info, or a silently mis-formatted keyboard.
- A single key needs verbose boilerplate: `{"macro":"CTRL C","display":"CTRL C"}`.
- **tjek** (Termux Json Extended key language) is a *JSON parser/validator*: it exists
  to cope with hand-written JSON after the fact. As an authoring step it is the symptom,
  not the cure — it still forces the user to write JSON.
- **tekl** fixes the root cause: an authoring DSL with a compiler. Semantic errors are
  caught at compile time (parse/compile errors carry line+column + caret), and the
  compiled output **is** the button structure — valid by construction, so no JSON
  validation layer is needed afterwards.

If we keep compiling tekl → JSON → re-parse, we are still walking through tjek; the
proposal is to skip that entirely.

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

- `A(A)` — combo → button with macro + display
- `A:N` — label → display text
- `A.A` — popup
- `;` — separate keyboard rows
- blank `\n` — code line break (not a new row)

## Implementation strategy (in termux-app)

1. **Property**: add `extra-keys-tekl` to `TermuxPropertyConstants`. `extra-keys`
   remains untouched and fully backward compatible; when `extra-keys-tekl` is present
   it wins, otherwise fall back to the legacy JSON path.
2. **Compiler**: port the tekl compiler (tokenizer + parser + compiler, ~300 lines of JS
   in `lib/tekl/`) to a pure-Kotlin class in `termux-shared` (no JS runtime needed).
   The compiler's output is the **internal model** Termux renders
   (`ExtraKeyInfo`/button objects), *not* a JSON string — there is no JSON round-trip.
3. **Integration point**: `TermuxTerminalExtraKeys.java` reads the property and passes
   it to `ExtraKeysInfo`; `ExtraKeysInfo` branches: tekl → compile straight into the
   internal model; JSON property → keep the existing `new JSONArray(...)` read-back for
   legacy configs.
4. **Validation is the compiler**: key/macro mutual exclusion, single-level popup, and
   string display checks are part of the compilation step itself, so invalid tekl is
   rejected at compile time with line+column, before anything reaches the view layer.
5. **Errors/warnings**:
   - Compile errors (line/column + caret snippet) surface via the existing toast+log
     path (`Could not load and set the "extra-keys" property ...`).
   - Compile-time warnings (uneven row lengths, missing DRAWER/KEYBOARD) logged, not fatal.

## Files affected (estimate)

- `termux-shared/.../TermuxPropertyConstants.java` — new property key
- `termux-shared/.../extrakeys/ExtraKeysInfo.java` — tekl branch (compile to model,
  JSON only for legacy read-back)
- `app/src/main/java/com/termux/app/terminal/io/TermuxTerminalExtraKeys.java` — read+dispatch
- new: `termux-shared/.../extrakeys/tekl/*` (Kotlin port of compiler)
- `docs/` + README — authoring guide for tekl

## Compatibility & migration

- `extra-keys` JSON configs keep working unchanged (legacy, read-only path).
- tekl is opt-in via the new property; user configs untouched.
- tekl output is the exact same model/behaviour Termux has today (`macro` vs `key`,
  `popup`), so rendering and interaction are unchanged.

## Open questions

- Drop tjek entirely vs keep JSON read-back for legacy `extra-keys`? (proposal: keep it
  only as a legacy reader, remove it as an authoring/validation path)
- Property discovery: dedicated `extra-keys-tekl` key vs. value-prefix detection in
  `extra-keys`? (dedicated key preferred — avoids `=`/escape issues in `.properties`)
- Should the Kotlin port be code-generated from the JS reference (single source of
  truth), or maintained separately for now?
- Language of warnings/logs: English following existing Termux conventions.