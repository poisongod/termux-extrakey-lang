// 焦点下移
#{
    desc: "焦点下移",
    run: || if cursor_row() + 1 < row_count() { set_cursor(cursor_row() + 1, cursor_col()) }
}
