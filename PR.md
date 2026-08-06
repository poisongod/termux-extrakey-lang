# PR (draft for discussion)

> **title**: `feat(extra-keys): author extra-keys with the tekl DSL, bypassing JSON`

---

## 0 What this change is

- Adds an optional property `extra-keys-tekl` that holds tekl source.
- Ships a ~300-line pure-Kotlin compiler that translates tekl straight into Termux's
  internal `ExtraKeyButton` model.
- No `JSONArray` round-trip, no tjek layer involved.
- `extra-keys` (JSON) keeps working unchanged as a legacy read-only path.
- Invalid tekl is rejected **at compile time** (with line + column), not by a load-time
  `JSONException`.
- Rendering and interaction layers are untouched.

---

## 1 The real problem: authoring `extra-keys` is accumulated technical debt

### 1.1 It repeatedly hurts real users — not a "design taste" issue

- **#4386** (`extra-keys` backslash unusable, still open): a user just wants a `\` key.
  `'\\'` errors, `"\\\\"` errors, `"\u005c"` errors; in the end the suggested fix is to
  memorize the magic keyword `BACKSLASH` instead. One character, several rounds of
  fighting the parser.
- **#4589** (2025-06): "extra-keys of termux.properties malfunctioning" is still open, and
  the maintainer is engaged in the same `"\u005c"` escape mud-wrestling.
- Community tutorials (e.g. blog.vinfall.com) teach readers that pasting the *official
  sample* produces a JSON parse error with `termux-reload-settings`, and that matching
  syntax sentences have to be memorized by trial and error.

**Root cause**: the `extra-keys` value is JSON sitting inside a `termux.properties`
double-escaping context. Two layers of semantics — the `.properties` `\` rules and the JSON
`\` rules — collide inside one string. Everyone gets it wrong; the only correct answer is
to go read `ExtraKeysConstants.java` and copy a keyword.

### 1.2 `extra-keys` is a perfect DSL candidate; JSON is just an awkward container

A keyboard layout is fundamentally a tiny **domain-specific language**:

- It is a **matrix** (rows = keyboard rows, columns = keys).
- Each key has a small fixed set of semantics: plain key / combo / label / popup.
- The keys are named constants (`ESC`, `TAB`, …) with a fixed structure — fully
  expressible in a DSL.

Describing that with JSON (designed as a generic interchange format) means paying a tax
that `extra-keys` did not ask for:

- Quotes, commas and brackets that add nothing but typing.
- Semantic errors only surface at load time.
- No natural way to write a comment.
- Long rows inflate 3–5×, becoming unreadable and hard to diff.

### 1.3 Precedent in adjacent projects

- Key-binding configs in vim/zsh/ssh are DSLs/block syntax, not JSON.
- Termux itself already exposes `extra-keys` as an agnostic library in `termux-shared`
  (commit `2a74d43c`, "Convert `extra-keys` to agnosticism"). A DSL compiler fits that
  direction perfectly: plugin authors get the same language for free.

---

## 2 What tekl is, and the line with tjek

- **tekl** = Termux Extra Key Language: a small language whose scope is just "the keyboard
  matrix".
- Every reserved word in the grammar is an existing Termux key keyword (`ESC`, `TAB`, `CTRL`…).

It is a direct de-elevation of today's JSON row:

```
# termux.properties (today)                          # termux.properties (proposed)
extra-keys = [[ "ESC", "TAB", "CTRL", "ALT" ],       extra-keys-tekl =
             [ { "macro": "CTRL C",                  ESC TAB CTRL ALT;
                 "display": "CTRL C" } ]]               CTRL(C);
```

### Grammar (reference: tekl.ebnf)

```
N ::= "ESC" | "TAB" | ... | [A-Za-z0-9_]*   // plain keys
S ::= "CTRL" | "ALT" | "FN" | "SHIFT"        // special modifier keys
A ::= N | S | A(A) | A:N
M ::= A | A.A | M M | M'\n' | M';\n'
```

| written | meaning | internal model |
| --- | --- | --- |
| `ESC` | plain key | `"ESC"` |
| `CTRL(C)` | combo | `{"macro":"CTRL C","display":"CTRL C"}` |
| `HOME:Start` | label | `{"key":"HOME","display":"Start"}` |
| `ESC.BS` | popup | `{"key":"ESC","popup":"F1"}` |
| `CTRL(C):copy.F5` | combo+label+popup | `{"macro":"CTRL C","display":"copy","popup":"F5"}` |
| `;` | new row | — |

tekl has **no `.properties` escape problem**: keys are identifiers (reserved words), so
double-escaping scenarios like #4386 / #4589 literally cannot exist because there is no
embedded string/JSON layer.

### Compile-time checks (author-facing infrastructure)

- Rejects: unbalanced `CTRL(`C:`, popup inside popup, repeated modifier in a row
  (`CTRL CTRL`).
- Warns (non-fatal): empty label, uneven row lengths, missing `DRAWER`/`KEYBOARD`.
- Errors carry line + column with a caret; load failures can `toast` and go through the
  existing `Logger.logErrorWithStackTrace` path.

> None of this is feasible on the JSON authoring path.

---

## 3 Minimal footprint

### 3.1 Files touched (estimate)

```
termux-shared/.../TermuxPropertyConstants.java          +KEY_EXTRA_KEYS_TEKL
termux-shared/.../extrakeys/ExtraKeysInfo.java          branch tekl -> List<ExtraKeyButton>
termux-shared/.../extrakeys/tekl/*.kt                    new compiler (tokenizer/parser/compiler)
app/src/main/java/.../terminal/io/TermuxTerminalExtraKeys.java  read + dispatch
app/src/test/...                                         test suite for tekl
```

### 3.2 Relationship with existing code

- The existing `new JSONArray(...)` path in `ExtraKeysInfo` is **left untouched**.
- `ExtraKeysView` rendering is unchanged.
- The compiler is a **pure function** (string → `List<ExtraKeyButton>`), no Android API,
  unit-testable, and usable by plugins / a future TUI editor from the `termux-shared` layer.

### 3.3 Explicitly out of scope

- No migration script; existing JSON configs stay untouched.
- JSON path is not removed; legacy users are unaffected.
- tekl is not enabled by default; the default value for `extra-keys` is unchanged.
- No debate over dropping tjek — tjek stays an independent validation tool for JSON configs.

---

## 4 Validation / definition of success

- A developer types the same keyboard in JSON and tekl with a stopwatch: the tekl side is
  ready in ~5 minutes after reading the grammar and appears right after
  `termux-reload-settings`.
- Typing `CTRL(C` by accident yields a line/column diagnostic instead of a silent empty row.
- Unit tests cover: tokenizer, parser, compiler, full mapping of written–onto–model, and
  the error/warning matrices.
- Compatibility: all existing extra-keys JSON configurations unchanged; CI passes.

---

## 5 Mergeable checklist

- [ ] No new dependencies (same GPLv3 origin as the repo).
- [ ] Compiler builds and is testable inside `termux-shared` module alone.
- [ ] When `extra-keys-tekl` is absent/empty, behavior is byte-identical to today.
- [ ] New property naming + docs reviewed by maintainers.

---

## Open questions

1. Property discovery: a dedicated `extra-keys-tekl` key vs. value-prefix detection in
   `extra-keys`? (proposal: dedicated key, avoids escape ambiguity)
2. JSON (tjek) path: keep legacy read-only vs. drop entirely? (proposal: keep read-only)
3. Kotlin port vs. JS reference in `poisongod/termux-extrakey-lang` (GPLv3) — sync strategy?
4. Log/diagnostic language: English (as today) vs i18n?