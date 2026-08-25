// 光标上移
#{
    desc: "光标上移",
    run: || if cursor_row() > 0 { set_cursor(cursor_row() - 1, cursor_col()) }
}
