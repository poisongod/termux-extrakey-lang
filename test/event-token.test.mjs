import { test } from "node:test"
import assert from "node:assert/strict"
import { eventToken, isTextInput } from "../lib/ui.mjs"

function key(partial = {}) {
  return { name: "", sequence: "", ctrl: false, meta: false, option: false, ...partial }
}

test("event-token: 方向键归一化", () => {
  assert.equal(eventToken(key({ name: "up" })), "up")
  assert.equal(eventToken(key({ name: "down" })), "down")
  assert.equal(eventToken(key({ name: "left" })), "left")
  assert.equal(eventToken(key({ name: "right" })), "right")
})

test("event-token: enter/esc/space", () => {
  assert.equal(eventToken(key({ name: "return" })), "enter")
  assert.equal(eventToken(key({ name: "enter" })), "enter")
  assert.equal(eventToken(key({ name: "escape" })), "esc")
  assert.equal(eventToken(key({ name: "esc" })), "esc")
  assert.equal(eventToken(key({ name: "space" })), "space")
})

test("event-token: ctrl/meta/option 修饰键", () => {
  assert.equal(eventToken(key({ name: "a", ctrl: true })), "ctrl")
  assert.equal(eventToken(key({ name: "up", meta: true })), "alt")
  assert.equal(eventToken(key({ name: "up", option: true })), "alt")
})

test("event-token: 普通字符键返回 null (不经模式机)", () => {
  assert.equal(eventToken(key({ name: "a", sequence: "a" })), null)
  assert.equal(eventToken(key({ name: "backspace" })), null)
})

test("event-token: isTextInput 识别可输入字符", () => {
  assert.equal(isTextInput(key({ name: "a", sequence: "a" })), true)
  assert.equal(isTextInput(key({ name: "space", sequence: " " })), true)
  assert.equal(isTextInput(key({ name: "up" })), false)
  assert.equal(isTextInput(key({ name: "escape" })), false)
})