import { test } from "node:test"
import assert from "node:assert/strict"
import { readFile, mkdir, rm } from "node:fs/promises"
import { loadDoc, saveDoc } from "../lib/keys.mjs"
import { parse } from "../lib/keys.mjs"
import { defaultLayoutFile } from "../lib/user-dir.mjs"

const TMP = "/data/data/com.termux/files/usr/tmp/opencode/tui-io-test"

test("io: loadDoc 读取并解析 tekl 文件", async () => {
  const { rows, src, file } = await loadDoc("keys.layout")
  assert.equal(file, "keys.layout")
  assert.ok(rows.length >= 1)
  assert.ok(src.includes("ESC"))
})

test("io: loadDoc 文件不存在返回空文档", async () => {
  const { rows, src } = await loadDoc("/data/data/com.termux/files/usr/tmp/opencode/nope.tekl")
  assert.deepEqual(rows, [])
  assert.equal(src, "")
})

test("io: loadDoc null 回退默认文件 (render 不传 file 时)", async () => {
  const { rows, file } = await loadDoc(null)
  assert.equal(file, defaultLayoutFile())
  assert.equal(file, `${process.env.HOME}/.termux/termux-extrakey/keys.layout`)
  const { rows: viaUndef } = await loadDoc(undefined)
  assert.deepEqual(rows, viaUndef)
})

test("io: saveDoc 写 tekl + properties, reload 可读回", async () => {
  await mkdir(TMP, { recursive: true })
  const file = `${TMP}/k.tekl`
  const props = `${TMP}/t.properties`
  await rm(file, { force: true })
  await rm(props, { force: true })

  const rows = parse("ESC TAB;\nCTRL(C) HOME:Start;")
  const { src, json } = await saveDoc(rows, { file, propsPath: props, reload: false })
  assert.ok(src.includes("ESC TAB;"))

  const reloaded = await loadDoc(file)
  assert.equal(reloaded.rows.length, 2)
  assert.equal(json.length, 2)

  const content = await readFile(props, "utf8")
  assert.ok(content.includes("extra-keys ="))
})

test("io: saveDoc 写出为规范化 (round-trip)", async () => {
  await mkdir(TMP, { recursive: true })
  const file = `${TMP}/rt.tekl`
  const props = `${TMP}/rt.properties`
  const src = "ESC TAB CTRL ALT;\nCTRL(C) ESC.F1;"
  const { src: out } = await saveDoc(parse(src), { file, propsPath: props, reload: false })
  assert.deepEqual(parse(out), parse(src))
})