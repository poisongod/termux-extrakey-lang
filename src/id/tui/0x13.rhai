// 焦点右移
#{
    desc: "焦点右移",
    run: || if cursor_col() + 1 < col_count() { set_cursor(cursor_row(), cursor_col() + 1) }
}
