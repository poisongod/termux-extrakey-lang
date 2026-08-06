#!/usr/bin/env node

import { readFile } from "node:fs/promises"
import { compileFile } from "./compiler-io.mjs"
import { jit } from "./jit.mjs"
import { formatParseError, ParseError } from "./parser.mjs"
import { CompileError } from "./compiler.mjs"

const USAGE = `usage: tekl <file.tekl> [--output <termux.properties>] [--reload] [--watch]
  tekl <file.tekl>                      compile tekl and write extra-keys into ~/.termux/termux.properties
  --output <path>                       write to a custom termux.properties (default: ~/.termux/termux.properties)
  --reload                              run termux-reload-settings after writing
  --watch                               JIT mode: watch the file, recompile + write on change`

function parseArgs(argv) {
  const opts = { watch: false, reload: false }
  const rest = []
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (arg === "--watch") opts.watch = true
    else if (arg === "--reload") opts.reload = true
    else if (arg === "--output") {
      opts.output = argv[++i]
      if (!opts.output) {
        console.error(USAGE)
        process.exit(1)
      }
    } else if (arg.startsWith("-")) {
      console.error(`unknown option: ${arg}\n${USAGE}`)
      process.exit(1)
    } else {
      rest.push(arg)
    }
  }
  opts.file = rest[0]
  if (rest.length > 1) {
    console.error(`unexpected arguments: ${rest.slice(1).join(" ")}\n${USAGE}`)
    process.exit(1)
  }
  return opts
}

function printError(err, src) {
  if (err instanceof ParseError && src !== undefined) {
    console.error(formatParseError(src, err))
    return
  }
  if (err instanceof CompileError || err?.name === "CompileError") {
    console.error(`tekl: ${err.message.replace(/^tekl: /, "")}`)
    return
  }
  console.error(err.message)
  if (err.cause?.message) console.error(`  cause: ${err.cause.message}`)
}

async function loadSource(file) {
  return readFile(file, "utf8")
}

async function main() {
  const opts = parseArgs(process.argv.slice(2))
  const propsPath = opts.output

  if (!opts.file) {
    console.error(USAGE)
    process.exit(1)
  }

  if (opts.watch) {
    console.log(`[tekl] watching ${opts.file} -> ${propsPath ?? "~/.termux/termux.properties"}`)
    await jit(opts.file, {
      propsPath,
      apply: true,
      reload: opts.reload,
      onUpdate: (json) => {
        console.log(`[tekl] ${new Date().toLocaleTimeString()} updated ${json.length} row(s)`)
      },
      onError: async (err) => {
        let src
        try {
          src = await loadSource(opts.file)
        } catch {
          src = undefined
        }
        printError(err, src)
      },
    })
    return
  }

  try {
    const { src } = await compileFile(opts.file, {
      propsPath,
      apply: true,
      reload: opts.reload,
    })
    const where = propsPath ?? "~/.termux/termux.properties"
    console.log(`[tekl] written to ${where}${opts.reload ? ", settings reloaded" : ""}`)
  } catch (err) {
    let src
    try {
      src = await loadSource(opts.file)
    } catch {
      src = undefined
    }
    printError(err, src)
    process.exit(1)
  }
}

main()