import { watch } from "node:fs"
import { compileFile, reloadSettings } from "./compiler-io.mjs"

export async function jit(teklPath, options = {}) {
  const { propsPath, apply = true, reload = false, onError, onUpdate } = options
  let pending = false
  let lastError = null

  async function run() {
    try {
      const { json } = await compileFile(teklPath, { propsPath, apply })
      if (reload) {
        try {
          await reloadSettings()
        } catch (err) {
          if (onError) onError(err)
        }
      }
      lastError = null
      if (onUpdate) onUpdate(json)
    } catch (err) {
      lastError = err
      if (onError) onError(err)
    }
  }

  try {
    await run()
  } catch {
    // errors reported via onError
  }

  watch(teklPath, { persistent: true }, () => {
    if (pending) return
    pending = true
    setTimeout(() => {
      pending = false
      run()
    }, 100)
  })

  return {
    getLastError: () => lastError,
  }
}
