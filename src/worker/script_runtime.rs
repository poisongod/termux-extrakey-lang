//! Rhai 脚本运行时 — 直接执行模型。
//!
//! 消费者把自身状态包进 `Arc<Mutex<S>>`，注册细粒度原语（读+写）时捕获该句柄。
//! ID 插件的 `run` 闭包在此之上组合出真实逻辑——读状态、做判断、写状态，
//! 全部发生在 `.rhai` 文件内。无中间操作层。

use anyhow::Result;
use rhai::{Engine, FnPtr};
use std::sync::{Arc, Mutex};

/// 引擎 + 注册池 AST 的打包体 —— 每个消费者持有一份。
pub struct ScriptHost {
    pub engine: Engine,
    pub lib: rhai::AST,
}

impl ScriptHost {
    /// 调用一个已加载的 action 闭包（来自 `<ID>.rhai` 的 `run` 字段）。
    /// 闭包通过捕获在引擎里的状态句柄直接读写消费者状态。
    pub fn call(&self, run: &FnPtr) -> Result<()> {
        run.call::<()>(&self.engine, &self.lib, ())
            .map_err(|e| anyhow::anyhow!("action error: {e}"))
    }
}

/// 把共享状态句柄注册为 Rhai 类型并构建引擎的标准姿势：
///
/// ```ignore
/// let mut engine = Engine::new();
/// engine.register_type_with_name::<Arc<Mutex<S>>>("State");
/// // 读原语: 闭包捕获句柄克隆
/// engine.register_fn("cursor_row", move || handle.lock().unwrap().row);
/// // 写原语
/// engine.register_fn("set_cursor", move |r: i64, c: i64| { ... });
/// ```
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 演示用消费者状态
    #[derive(Default)]
    struct Demo {
        row: i64,
        col: i64,
        rows: i64,
    }

    /// 构建演示引擎: 句柄类型 + 读/写原语
    fn demo_engine(handle: Arc<Mutex<Demo>>) -> Engine {
        let mut engine = Engine::new();
        engine.register_type_with_name::<Arc<Mutex<Demo>>>("Demo");
        engine.register_fn("to_string", |s: &Arc<Mutex<Demo>>| "Demo".to_string());

        // 读原语 — 让 .rhai 能做条件判断
        {
            let h = handle.clone();
            engine.register_fn("cursor_row", move || h.lock().unwrap().row);
        }
        {
            let h = handle.clone();
            engine.register_fn("row_count", move || h.lock().unwrap().rows);
        }
        // 写原语
        {
            let h = handle.clone();
            engine.register_fn("set_cursor", move |r: i64, c: i64| {
                let mut d = h.lock().unwrap();
                d.row = r;
                d.col = c;
            });
        }
        engine
    }

    #[test]
    fn plugin_reads_and_writes_state() {
        let handle = Arc::new(Mutex::new(Demo { row: 3, col: 1, rows: 10 }));
        let engine = demo_engine(handle.clone());

        // 插件源码: 带守卫的真实逻辑
        let src = r#"
// 上移: 有上一行才动
#{ desc: "上移", run: ||
    if cursor_row() > 0 { set_cursor(cursor_row() - 1, 1) }
}
"#;
        let ast = engine.compile(src).unwrap();
        let map: rhai::Map = engine.eval_ast(&ast).unwrap();
        let run = map.get("run").unwrap().clone().cast::<FnPtr>();

        run.call::<()>(&engine, &ast, ()).unwrap();
        assert_eq!(handle.lock().unwrap().row, 2);

        // 边界: 已在第 0 行则不动
        handle.lock().unwrap().row = 0;
        run.call::<()>(&engine, &ast, ()).unwrap();
        assert_eq!(handle.lock().unwrap().row, 0);
    }

    #[test]
    fn unknown_function_errors() {
        let handle = Arc::new(Mutex::new(Demo::default()));
        let engine = demo_engine(handle);
        let src = r#"#{ desc: "坏", run: || no_such_fn() }"#;
        let ast = engine.compile(src).unwrap();
        let map: rhai::Map = engine.eval_ast(&ast).unwrap();
        let run = map.get("run").unwrap().clone().cast::<FnPtr>();
        assert!(run.call::<()>(&engine, &ast, ()).is_err());
    }
}
