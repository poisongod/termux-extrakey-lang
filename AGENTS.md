# AGENTS.md

Termux extra-keys toolchain, developed **on Termux (Android)**: `tekl` DSL compiler + `tjek` loose-JSON validator. GPL-3.0. Requires Node 26.3+ (26.4 verified here).

## Layout

- `lib/tekl/` — tekl compiler: `tokenizer.mjs` → `parser.mjs` (recursive descent per `tekl.ebnf`) → `compiler.mjs` (AST → ExtraKeyButton JSON + warnings) → `compiler-io.mjs` (write properties) + `cli.mjs`, `jit.mjs` (`--watch`)
- `lib/tjek/` — Android `JSONTokener`-compatible parser (`json-dsl.mjs`), four-field validator (`validate-json.mjs`), error reporter
- `src/app.mjs` — experimental opentui TUI scaffold, **not** the main deliverable
- `keys.tekl` — canonical example; `PR.md` — draft PR proposing Kotlin port to termux-app (context, not build input)
- Docs (READMEs, warnings) are in Chinese; `tekl.ebnf` is the grammar source of truth (comments are `(* *)`, `A`/`N`/`S` are placeholders)

## Commands

```bash
npm install && npm link   # provides termux-extrakey-tekl and termux-extrakey-tjek
npm test                  # 76 tests; single file: node --test test/tekl.test.mjs
npm start                 # node --experimental-ffi src/app.mjs (TUI dev)
```

- CLI: `termux-extrakey-tekl [file|default ~/.termux/termux.tekl] [--output <props>] [--reload] [--watch]`; `termux-extrakey-tjek <file|-|stdin>`. `--watch` recompiles on change and **exits on first success**.
- Shebangs in `lib/*/cli.mjs` are Termux absolute paths (`/data/data/...`) — run via `node` or `npm link` outside Termux.

## Key quirks

- **Round-trip contract**: `compiler-io.mjs` serializes DSL then re-parses it with tjek's parser and requires identical JSON before writing. Changing `dsl-stringify.mjs` or the tjek parser can break this invariant — run `npm test`.
- Write path (`updateProperties`): replaces/appends `extra-keys = ...` in `~/.termux/termux.properties`, atomic via `.tekl.bak` rename (restored on write failure), verifies read-back.
- Case rules (`compiler.mjs` `keyName`): named keys (ESC, F1…) and modifiers (CTRL/ALT/FN/SHIFT) normalize to uppercase (case-insensitive input); plain combo identifiers and quoted literals keep original case. Combos default to **no** `display` field.
- Modifiers may not repeat within a row; `key`/`macro` are mutually exclusive; `popup` single level only; no rename/popup inside `()`.
- Warnings (`collectWarnings`, non-fatal, yellow, Chinese): empty field values, uneven row lengths, missing `DRAWER`/`KEYBOARD`.
- All ESM `.mjs`, no external test framework (`node --test`). Commit style: conventional commits with `tekl`/`tjek` scope, messages in Chinese.

## Stale docs

- Root `README.md` says "53 用例"; actual is 76 (lib/tekl/README.md has it right).
