// App 根组件 (设计 §5/§6): 模式条 + 网格 + 覆盖面板 + 问题面板 + 状态栏。
// 无 JSX (createElement 零构建);键盘事件 → eventToken → mode.dispatch → {state, effects}。
// services 由 render 插件注入 (内核装配), App 不直接 import tui 单例。

import { createElement, useEffect, useState } from "react"
import { useKeyboard } from "@xincli/opentui-react"
import { EFFECT } from "../lib/ui.mjs"
import {
  AppHeader,
  Grid,
  ModeBar,
  MenuPanel,
  MatrixPanel,
  NameInput,
  Problems,
  StatusBar,
} from "./components.mjs"

export function App({ file, services }) {
  const store = services.store
  const { dispatch, eventToken, saveDoc } = services
  const [snapshot, setSnapshot] = useState(() => store.getState())
  useEffect(() => store.subscribe(setSnapshot), [store])

  useKeyboard((key) => {
    const state = store.getState()
    // 名称输入框: 自由文本,不经过模式机 (§6.6 唯一例外)
    if (state.panel === "name") {
      handleNameKey(key, state)
      return
    }
    const token = eventToken(key)
    if (!token) return
    const { state: next, effects, ok } = dispatch(state, token)
    if (!ok && effects.length === 0) return
    store.setState(next)
    for (const eff of effects) handleEffect(eff)
  })

  function handleNameKey(key, state) {
    if (eventToken(key) === "esc") {
      const { state: next } = dispatch(state, "esc")
      store.setState(next)
      return
    }
    if (eventToken(key) === "enter") {
      const { state: next, effects } = dispatch(state, "enter")
      store.setState(next)
      for (const eff of effects) handleEffect(eff)
      return
    }
    if (key.name === "backspace") {
      store.setState({ ...state, nameDraft: state.nameDraft.slice(0, -1) })
      return
    }
    if (key.sequence && key.sequence.length === 1) {
      store.setState({ ...state, nameDraft: state.nameDraft + key.sequence })
    }
  }

  async function handleEffect(eff) {
    const state = store.getState()
    if (eff.type === "exit") {
      await persist(state)
      process.exit(0)
    }
    if (eff.type === "apply") {
      try {
        await persist(state)
        store.setGlobalErrors([])
      } catch (err) {
        store.setGlobalErrors([`保存失败: ${err.message}`])
      }
    }
    // cancel: 预览回滚,不写盘 (§6.7 铁律 2)
  }

  async function persist(state) {
    const { docPath, propsPath } = store.getPaths()
    await saveDoc(state.rows, { file: docPath, propsPath, reload: true })
  }

  return createElement(
    "box",
    { flexDirection: "column", gap: 1 },
    ...[
      createElement(AppHeader),
      createElement(ModeBar, { mode: snapshot.mode }),
      createElement(Grid, { state: snapshot }),
      snapshot.panel === "rowmenu" ? createElement(MenuPanel, { state: snapshot }) : null,
      snapshot.panel === "matrix" ? createElement(MatrixPanel, { state: snapshot }) : null,
      snapshot.panel === "name" ? createElement(NameInput, { state: snapshot }) : null,
      createElement(Problems, { state: snapshot, services }),
      createElement(StatusBar, { state: snapshot }),
    ].filter(Boolean),
  )
}