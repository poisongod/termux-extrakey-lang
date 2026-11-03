# tekl conformance fixtures

One shared corpus for the two independent tekl implementations:

| Implementation | Location | Role |
|---|---|---|
| Rust CLI | `termux-extrakey-lang/src/lib/keys.rs` | `tekl check` / `tekl reload` |
| Java app | `termux-app/termux-shared/.../extrakeys/tekl/` | parses `~/.termux/termux.tekl` |

Both must accept exactly the files marked `ok` and reject the ones marked `error`, with the
same 1-based line/column on parse errors. This prevents the CLI from writing a layout the app
then refuses to load (or vice versa).

## Manifest

`manifest.tsv` is tab-separated, `#` starts a comment:

| Column | Meaning |
|---|---|
| `name` | fixture stem; the source is `cases/<name>.tekl` |
| `expect` | `ok` or `error` |
| `line` | expected 1-based error line, `0` = do not check (compile-stage errors have no position) |
| `column` | expected 1-based error column |
| `rows` | for `ok` cases, expected number of keyboard rows |

Every `.tekl` file under `cases/` must be listed; the Rust test enforces that.

## Running

```bash
cargo test conformance                    # Rust half

scripts/sync-conformance.sh               # copy fixtures into the app repo
cd ../termux-app && ./gradlew :termux-shared:test --tests '*TeklConformanceTest'
```

## Adding a case

1. Add `cases/<name>.tekl`.
2. Add a manifest row.
3. Run both suites. If they disagree, the parsers drifted — fix the implementation, not the
   fixture, unless the fixture itself is wrong.
4. Re-run `scripts/sync-conformance.sh` so the app copy stays current.
