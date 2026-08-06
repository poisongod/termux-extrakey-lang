import { test } from "node:test"
import assert from "node:assert/strict"
import { validateExtraKeyJson, ValidationError } from "../lib/tjek/validate-json.mjs"

test("all supported Termux forms are valid", () => {
  const OK = [[
    "ESC",
    "TAB",
    { key: "CTRL", display: "Ctrl" },
    { key: "-", popup: "|" },
    { macro: "CTRL f d", display: "tmux exit" },
    { key: "UP", popup: { macro: "CTRL u", display: "UP-PG" } },
    { key: "CTRL", popup: { key: "C", display: "Ctrl+C" } },
  ]]
  assert.doesNotThrow(() => validateExtraKeyJson(OK))
})

test("official termux-tools example (macro + popup macro)", () => {
  const cfg = [[
    { macro: "ALT j", display: "A-j", popup: { macro: "ALT g", display: "A-g" } },
    { key: "ESC", popup: { macro: "CTRL f d", display: "tmux exit" } },
    { key: "(", popup: "{" },
    { key: ":", popup: ";" },
    { key: "/", popup: "\\\\" },
    "ALT",
    { macro: ":wq\n", display: "Write And Exit" },
  ]]
  assert.doesNotThrow(() => validateExtraKeyJson(cfg))
})

test("string shorthand entry", () => {
  assert.doesNotThrow(() => validateExtraKeyJson([["HOME", { key: "END" }]]))
})

test("mutually exclusive key+macro rejected", () => {
  assert.throws(() => validateExtraKeyJson([[{ key: "A", macro: "B" }]]), /both 'key' and 'macro/)
})

test("missing key/macro rejected", () => {
  assert.throws(() => validateExtraKeyJson([[{ display: "X" }]]), /must specify either/)
})

test("unknown field rejected", () => {
  assert.throws(() => validateExtraKeyJson([[{ key: "A", foo: "bar" }]]), /unknown field 'foo'/)
})

test("non-string field value rejected", () => {
  assert.throws(() => validateExtraKeyJson([[{ key: "A", display: 123 }]]), /'display' must be a string/)
})

test("nested popup rejected", () => {
  assert.throws(
    () => validateExtraKeyJson([[{ key: "A", popup: { key: "B", popup: "C" } }]]),
    /cannot be nested/,
  )
})

test("popup missing key/macro rejected", () => {
  assert.throws(() => validateExtraKeyJson([[{ key: "A", popup: { display: "X" } }]]), /popup must specify/)
})

test("non-array top level rejected", () => {
  assert.throws(() => validateExtraKeyJson({ key: "A" }), /must be a JSON array/)
})

test("invalid entry type rejected", () => {
  assert.throws(() => validateExtraKeyJson([["A", 42]]), /string or object/)
})

test("row must be array", () => {
  assert.throws(() => validateExtraKeyJson(["ESC"]), /row must be an array/)
})

test("empty row rejected", () => {
  assert.throws(() => validateExtraKeyJson([[]]), /empty row/)
})

test("validation error carries path", () => {
  try {
    validateExtraKeyJson([["A", { key: "B", macro: "C" }]])
    assert.fail("should throw")
  } catch (err) {
    assert.ok(err instanceof ValidationError)
    assert.equal(err.path, "extra-keys[0][1]")
  }
})
