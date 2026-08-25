// 光标右移
#{
    desc: "光标右移",
    run: || if cursor_col() < line_len() { set_cursor(cursor_row(), cursor_col() + 1) }
}
