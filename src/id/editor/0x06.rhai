// 跳转末行
#{
    desc: "跳转末行",
    run: || if row_count() > 0 { set_cursor(row_count() - 1, cursor_col()) }
}
