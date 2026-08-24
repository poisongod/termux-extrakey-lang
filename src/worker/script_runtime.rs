//! Rhai 脚本运行时 — 记录-回放模式执行 ID 插件 action。
//!
//! action 代码调用注册的宿主函数时，引擎只**记录**类型化操作（HostOp），
//! 由消费者（App / EditorState）在 Rust 侧同步回放到自身状态。
//! 好处：脚本求值无副作用、无需跨线程共享 TUI 状态、操作可测试。

use anyhow::Result;
use rhai::Engine;
use std::sync::{Arc, Mutex};

/// 宿主操作 — Rhai 宿主函数调用的类型化记录。
#[derive(Debug, Clone, PartialEq)]
pub enum HostOp {
    Exit,
    SetMode(u8),
    Focus(String),
    AddRow(String),
    AddCol(String),
    DelRow,
    DelCol,
    SwapRow,
    SwapCol,
    SelectKey,
    SwapKey,
    ShowMatrix(String),
    InputValue(String),
    InputName(String),
    Apply,
    Cursor(String),
    Jump(String),
    DeleteChar,
    NewLine(String),
    DeleteLine,
    TogglePanel(String),
    InsertText(String),
    Backspace,
    ExecCmd,
    Save(bool),
    CancelCmd,
}

type OpLog = Arc<Mutex<Vec<HostOp>>>;

fn push_op(log: &OpLog, op: HostOp) -> std::result::Result<(), Box<rhai::EvalAltResult>> {
    log.lock().unwrap().push(op);
    Ok(())
}

/// 创建记录型引擎：宿主函数调用被记录为 HostOp 序列。
pub fn create_engine(log: OpLog) -> Engine {
    let mut engine = Engine::new();

    macro_rules! reg0 {
        ($name:literal, $op:expr) => {{
            let l = log.clone();
            engine.register_fn($name, move || -> std::result::Result<(), Box<rhai::EvalAltResult>> {
                push_op(&l, $op)
            });
        }};
    }
    macro_rules! reg1s {
        ($name:literal, $variant:ident) => {{
            let l = log.clone();
            engine.register_fn($name, move |a: &str| -> std::result::Result<(), Box<rhai::EvalAltResult>> {
                push_op(&l, HostOp::$variant(a.to_string()))
            });
        }};
    }

    // ── 系统 ──
    reg0!("exit", HostOp::Exit);
    {
        let l = log.clone();
        engine.register_fn("set_mode", move |n: i64| -> std::result::Result<(), Box<rhai::EvalAltResult>> {
            push_op(&l, HostOp::SetMode(n as u8))
        });
    }

    // ── 矩阵编辑 ──
    reg1s!("focus", Focus);
    reg1s!("add_row", AddRow);
    reg1s!("add_col", AddCol);
    reg0!("del_row", HostOp::DelRow);
    reg0!("del_col", HostOp::DelCol);
    reg0!("swap_row", HostOp::SwapRow);
    reg0!("swap_col", HostOp::SwapCol);
    reg0!("select_key", HostOp::SelectKey);
    reg0!("swap_key", HostOp::SwapKey);

    // ── 面板/输入 ──
    reg1s!("show_matrix", ShowMatrix);
    reg1s!("input_value", InputValue);
    reg1s!("input_name", InputName);
    reg0!("apply", HostOp::Apply);

    // ── 文本编辑器 ──
    reg1s!("cursor", Cursor);
    reg1s!("jump", Jump);
    reg0!("delete_char", HostOp::DeleteChar);
    reg1s!("new_line", NewLine);
    reg0!("delete_line", HostOp::DeleteLine);
    reg1s!("toggle_panel", TogglePanel);
    reg1s!("insert_text", InsertText);
    reg0!("backspace", HostOp::Backspace);
    reg0!("exec_cmd", HostOp::ExecCmd);
    {
        let l = log.clone();
        engine.register_fn("save", move |v: bool| -> std::result::Result<(), Box<rhai::EvalAltResult>> {
            push_op(&l, HostOp::Save(v))
        });
    }
    reg0!("cancel_cmd", HostOp::CancelCmd);

    engine
}

/// 调用一个已加载的 action 闭包（来自 .rhai 插件的 `run` 字段），
/// 返回按序记录的宿主操作。`ast` 为插件编译产物（闭包函数体所在 lib）。
pub fn call_action(run: &rhai::FnPtr, ast: &rhai::AST) -> Result<Vec<HostOp>> {
    let log: OpLog = Arc::new(Mutex::new(Vec::new()));
    let engine = create_engine(log.clone());
    run.call::<()>(&engine, ast, ())
        .map_err(|e| anyhow::anyhow!("action error: {e}"))?;
    let ops = log.lock().unwrap().clone();
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从内联插件源码提取某 ID 的 run 闭包再调用（模拟完整链路）。
    fn run_of(src: &str, id: &str) -> (rhai::FnPtr, rhai::AST) {
        let engine = Engine::new();
        let ast = engine.compile(src).unwrap();
        let map: rhai::Map = engine.eval_ast(&ast).unwrap();
        let f = map.get(id).unwrap().clone().cast::<rhai::Map>()
            .get("run").unwrap().clone().cast::<rhai::FnPtr>();
        (f, ast)
    }

    #[test]
    fn test_call_exit_closure() {
        let (f, ast) = run_of(r#"#{ "0x00": #{ desc: "d", run: || exit() } }"#, "0x00");
        assert_eq!(call_action(&f, &ast).unwrap(), vec![HostOp::Exit]);
    }

    #[test]
    fn test_call_sequence_closure() {
        let (f, ast) = run_of(
            r#"#{ "0x76": #{ desc: "d", run: || { save(true); exit() } } }"#,
            "0x76",
        );
        assert_eq!(call_action(&f, &ast).unwrap(), vec![HostOp::Save(true), HostOp::Exit]);
    }

    #[test]
    fn test_string_args_recorded() {
        let (f, ast) = run_of(
            r#"#{ "0x10": #{ desc: "d", run: || focus("up") } }"#,
            "0x10",
        );
        assert_eq!(call_action(&f, &ast).unwrap(), vec![HostOp::Focus("up".into())]);
    }

    #[test]
    fn test_unknown_function_errors() {
        let (f, ast) = run_of(
            r#"#{ "0xEE": #{ desc: "d", run: || no_such_fn() } }"#,
            "0xEE",
        );
        assert!(call_action(&f, &ast).is_err());
    }
}
