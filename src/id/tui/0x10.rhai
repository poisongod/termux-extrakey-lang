// 焦点上移
#{
    desc: "焦点上移",
    run: || if cursor_row() > 0 { set_cursor(cursor_row() - 1, cursor_col()) }
}
