// 选中/取消
#{
    desc: "选中/取消",
    run: || {
    // 已选中当前键则取消，否则选中当前键
    if selected_row() == cursor_row() && selected_col() == cursor_col() {
        clear_selection()
    } else {
        set_selected(cursor_row(), cursor_col())
    }
}
}
