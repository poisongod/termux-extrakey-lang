import { readFile, writeFile, rename, mkdir } from "node:fs/promises"
import { execFile } from "node:child_process"
import { pathToFileURL } from "node:url"
import { parse } from "./parser.mjs"
import { compile, validateTermuxJson, CompileError, collectWarnings } from "./compiler.mjs"
import { stringifyDsl } from "./dsl-stringify.mjs"
import { parseJsonDsl } from "../tjek/json-dsl.mjs"
import { validateExtraKeyJson } from "../tjek/validate-json.mjs"

const HOME = process.env.HOME
export const DEFAULT_PROPERTIES = `${HOME}/.termux/termux.properties`

export class IoError extends Error {
  constructor(message, cause) {
    super(`tekl: ${message}`)
    this.cause = cause
  }
}

export function compileSource(src) {
  const rows = parse(src)
  const json = compile(rows)
  validateTermuxJson(json)
  validateExtraKeyJson(json)
  return { rows, json, warnings: collectWarnings(json) }
}

export async function updateProperties(json, propsPath = DEFAULT_PROPERTIES) {
  const serialized = stringifyDsl(json)

  // round-trip: Android will re-parse this exact text with JSONTokener
  const reparsed = parseJsonDsl(serialized)
  if (JSON.stringify(reparsed) !== JSON.stringify(json)) {
    throw new IoError("generated extra-keys does not round-trip through Android JSONTokener")
  }

  let content = ""

  try {
    content = await readFile(propsPath, "utf8")
  } catch {
    content = ""
  }

  const lines = content.split("\n")
  let replaced = false

  const out = lines.map((line) => {
    if (/^\s*extra-keys\s*=/.test(line)) {
      replaced = true
      return `extra-keys = ${serialized}`
    }
    return line
  })

  if (!replaced) {
    if (out.length > 0 && out[out.length - 1] !== "") out.push("")
    out.push(`extra-keys = ${serialized}`)
  }

  await writePropertiesAtomic(propsPath, out.join("\n"))

  const written = await readFile(propsPath, "utf8")
  if (!written.includes(`extra-keys = ${serialized}`)) {
    throw new IoError(`verification failed: 'extra-keys' not found in ${propsPath}`)
  }

  return propsPath
}

async function writePropertiesAtomic(propsPath, content) {
  const dir = propsPath.includes("/") ? propsPath.slice(0, propsPath.lastIndexOf("/")) : "."
  await mkdir(dir, { recursive: true })

  const backup = `${propsPath}.tekl.bak`
  try {
    await rename(propsPath, backup)
  } catch {
    // no existing file to back up
  }

  try {
    await writeFile(propsPath, content)
  } catch (err) {
    try {
      await rename(backup, propsPath)
    } catch {
      // restore failed, report original error
    }
    throw new IoError(`failed to write ${propsPath}`, err)
  }
}

export function reloadSettings() {
  return new Promise((resolve, reject) => {
    execFile("termux-reload-settings", (error, stdout, stderr) => {
      if (error) {
        reject(new IoError(`termux-reload-settings failed: ${stderr || error.message}`))
        return
      }
      resolve(stdout)
    })
  })
}

export async function compileFile(filePath, options = {}) {
  const { propsPath, apply = false, reload = false } = options
  let src
  try {
    src = await readFile(filePath, "utf8")
  } catch (err) {
    throw new IoError(`cannot read '${filePath}'`, err)
  }

  const { json, warnings } = compileSource(src)

  if (apply) {
    await updateProperties(json, propsPath)
    if (reload) await reloadSettings()
  }

  return { json, propsPath, src, warnings }
}
