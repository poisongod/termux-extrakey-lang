// 焦点左移
#{
    desc: "焦点左移",
    run: || if cursor_col() > 0 { set_cursor(cursor_row(), cursor_col() - 1) }
}
