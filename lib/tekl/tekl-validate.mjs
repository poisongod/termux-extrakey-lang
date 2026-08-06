#!/usr/bin/env node

import { readFile } from "node:fs/promises"
import { parseJsonDsl } from "./json-dsl.mjs"
import { validateExtraKeyJson } from "./validate-json.mjs"
import { formatFullReport } from "./error-report.mjs"

const USAGE = `usage: tekl-validate <file | ->   # '-' reads stdin
  parses a termux extra-keys DSL (Android JSONTokener syntax, quotes optional)
  then validates the four-field structure (key/macro/display/popup)`

async function main() {
  const file = process.argv[2]
  if (!file) {
    console.error(USAGE)
    process.exit(1)
  }

  let text
  try {
    text = file === "-" ? await readStdin() : await readFile(file, "utf8")
  } catch (err) {
    console.error(`tekl-validate: cannot read '${file}': ${err.message}`)
    process.exit(1)
  }

  let json
  try {
    json = parseJsonDsl(text)
  } catch (err) {
    console.error(formatFullReport(text, err))
    if (process.env.TEKL_DEBUG !== undefined) console.error(err)
    process.exit(1)
  }

  try {
    validateExtraKeyJson(json)
    console.log("OK: valid termux extra-keys DSL")
  } catch (err) {
    console.error(formatFullReport(text, err, { json }))
    if (process.env.TEKL_DEBUG !== undefined) console.error(err)
    process.exit(1)
  }
}

function readStdin() {
  return new Promise((resolve, reject) => {
    let data = ""
    process.stdin.setEncoding("utf8")
    process.stdin.on("data", (chunk) => (data += chunk))
    process.stdin.on("end", () => resolve(data))
    process.stdin.on("error", reject)
  })
}

main()