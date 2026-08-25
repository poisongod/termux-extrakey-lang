// 光标下移
#{
    desc: "光标下移",
    run: || if cursor_row() + 1 < row_count() { set_cursor(cursor_row() + 1, cursor_col()) }
}
