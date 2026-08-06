import { test } from "node:test"
import assert from "node:assert/strict"
import { parseJsonDsl, JsonDslSyntaxError } from "../lib/tjek/json-dsl.mjs"
import { validateExtraKeyJson } from "../lib/tjek/validate-json.mjs"

test("wikipedia toy-level valid DSL forms", () => {
  assert.deepEqual(parseJsonDsl(`[[ESC, TAB], {key: CTRL, display: "Ctrl"}]`), [
    ["ESC", "TAB"],
    { key: "CTRL", display: "Ctrl" },
  ])
})

const VALID = `[
  ['ESC', '/', '-', {key: '-', popup: '|'}, DOWN, UP],
  ['TAB', 'CTRL', 'ALT', 'LEFT', 'DOWN', 'RIGHT', 'PGDN'],
  [{key: ESC, popup: {macro: "CTRL f d", display: "tmux exit"}}],
  [{macro: "ALT j", display: A-j, popup: {macro: "ALT g", display: A-g}}],
  [{key: "ESC", popup: {macro: ":wq\\n", display: "Write And Exit"}}]
]`

test("official-style DSL parses and validates", () => {
  const json = parseJsonDsl(VALID)
  assert.doesNotThrow(() => validateExtraKeyJson(json))
})

test("numbers, booleans, null, unicode", () => {
  assert.equal(parseJsonDsl(`[123, 3.14, 1e5, -2, true, false, null, "\\u0041"]`).length, 8)
})

test("duplicate key rejected", () => {
  assert.throws(() => parseJsonDsl(`[[{key: "A", key: "B"}]]`), /duplicate key/)
})

test("missing colon rejected", () => {
  assert.throws(() => parseJsonDsl(`[[{key}]`)/* eslint-disable-line */, /expected ':'/)
})

test("trailing comma in array allowed (Android accepts it)", () => {
  assert.deepEqual(parseJsonDsl(`[["A",]]`), [["A"]])
  assert.deepEqual(parseJsonDsl(`[["A",\n]]`), [["A"]])
})

test("trailing comma in object allowed (Android accepts it)", () => {
  assert.deepEqual(parseJsonDsl(`[ [ {"key":"A",} ] ]`), [[{ key: "A" }]])
})

test("numbers only when token starts with '-' or digit", () => {
  assert.deepEqual(parseJsonDsl(`[123, -2, 3.14, 1e5, 5., .5, +5, 1-2]`), [
    123,
    -2,
    3.14,
    100000,
    5,
    ".5",
    "+5",
    "1-2",
  ])
})

test("object keys stay raw strings, never numeric/boolean", () => {
  assert.deepEqual(parseJsonDsl(`[{key: true, "1": "a", 1e3: "b"}]`), [
    { key: true, 1: "a", "1e3": "b" },
  ])
})

test("unterminated object", () => {
  assert.throws(() => parseJsonDsl(`[["A"]`), /unterminated array/)
})

test("unterminated string", () => {
  assert.throws(() => parseJsonDsl(`[["A`), /unterminated string/)
})

test("unterminated object", () => {
  assert.throws(() => parseJsonDsl(`[ [ {"key":"A"`), /expected ',' or '}'/)
})

test("invalid escape", () => {
  assert.throws(() => parseJsonDsl(`[["\\x"]]`), /invalid escape/)
})

test("bad unicode escape", () => {
  assert.throws(() => parseJsonDsl(`[["\\u12"]]`), /hex digits/)
})

test("unexpected char after top-level", () => {
  assert.throws(() => parseJsonDsl(`[["a"]] x`), /after top-level/)
})

test("error carries line and column", () => {
  try {
    parseJsonDsl(`[\n  ["A" "B"\n]`)
    assert.fail("should throw")
  } catch (err) {
    assert.ok(err instanceof JsonDslSyntaxError)
    assert.equal(err.line, 2)
    assert.equal(err.column, 8)
  }
})

test("empty input rejected", () => {
  assert.throws(() => parseJsonDsl(``)/*1*/, /unexpected end of input/)
})

test("deep nesting works", () => {
  const json = parseJsonDsl(`{"a":{"b":{"c":[1,2,{"d":"e"}]}}}`)
  assert.equal(json.a.b.c[2].d, "e")
})