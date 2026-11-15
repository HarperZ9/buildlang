// ===============================================================================
// BUILDOS TEXT EDITOR (qedit)
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// A nano-like text editor for BuildOS with:
// - Full text editing capabilities
// - Search and replace
// - Cut/copy/paste
// - Undo support
// - Syntax highlighting (basic)
// - Line numbers
// - Status bar
//
// ===============================================================================

#![no_std]
#![no_main]
#![allow(unused_assignments)]
#![allow(dead_code)]

use core::panic::PanicInfo;

// =============================================================================
// CONSTANTS
// =============================================================================

const MAX_LINES: usize = 65536;
const MAX_LINE_LEN: usize = 4096;
const MAX_FILENAME: usize = 256;
const MAX_STATUS: usize = 256;
const MAX_SEARCH: usize = 256;
const UNDO_STACK_SIZE: usize = 100;
const TAB_SIZE: usize = 4;

// Syscall numbers
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_LSEEK: u64 = 8;
const SYS_EXIT: u64 = 60;
const SYS_IOCTL: u64 = 16;

// File flags
const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0o100;
const O_TRUNC: u64 = 0o1000;

// Terminal ioctl
const TIOCGWINSZ: u64 = 0x5413;

// =============================================================================
// SYSCALL INTERFACE
// =============================================================================

#[inline(always)]
unsafe fn syscall(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    ret
}

fn write(fd: i32, buf: &[u8]) -> isize {
    unsafe { syscall(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64, 0, 0, 0) as isize }
}

fn read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe { syscall(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0, 0) as isize }
}

fn open(path: &[u8], flags: u64, mode: u64) -> i32 {
    unsafe { syscall(SYS_OPEN, path.as_ptr() as u64, flags, mode, 0, 0, 0) as i32 }
}

fn close(fd: i32) -> i32 {
    unsafe { syscall(SYS_CLOSE, fd as u64, 0, 0, 0, 0, 0) as i32 }
}

fn exit(code: i32) -> ! {
    unsafe { syscall(SYS_EXIT, code as u64, 0, 0, 0, 0, 0) };
    loop {}
}

fn ioctl(fd: i32, request: u64, arg: u64) -> i32 {
    unsafe { syscall(SYS_IOCTL, fd as u64, request, arg, 0, 0, 0) as i32 }
}

// =============================================================================
// OUTPUT UTILITIES
// =============================================================================

fn print(s: &str) {
    write(1, s.as_bytes());
}

fn print_bytes(buf: &[u8]) {
    write(1, buf);
}

fn print_num(mut n: usize) {
    if n == 0 {
        print("0");
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = 20;

    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    write(1, &buf[i..]);
}

// =============================================================================
// ANSI ESCAPE CODES
// =============================================================================

fn clear_screen() {
    print("\x1b[2J");
}

fn move_cursor(row: usize, col: usize) {
    print("\x1b[");
    print_num(row + 1);
    print(";");
    print_num(col + 1);
    print("H");
}

fn hide_cursor() {
    print("\x1b[?25l");
}

fn show_cursor() {
    print("\x1b[?25h");
}

fn set_color(fg: u8, bg: u8) {
    print("\x1b[");
    print_num(fg as usize);
    print(";");
    print_num(bg as usize);
    print("m");
}

fn reset_color() {
    print("\x1b[0m");
}

fn invert_colors() {
    print("\x1b[7m");
}

fn bold() {
    print("\x1b[1m");
}

fn clear_line() {
    print("\x1b[K");
}

fn save_cursor() {
    print("\x1b[s");
}

fn restore_cursor() {
    print("\x1b[u");
}

// =============================================================================
// TERMINAL HANDLING
// =============================================================================

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

fn get_terminal_size() -> (usize, usize) {
    let mut ws = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let result = ioctl(0, TIOCGWINSZ, &mut ws as *mut _ as u64);
    if result >= 0 {
        (ws.ws_row as usize, ws.ws_col as usize)
    } else {
        (24, 80) // Default
    }
}

fn enable_raw_mode() {
    // Send escape sequence to enable raw mode (simplified)
    print("\x1b[?1049h"); // Alternative screen buffer
}

fn disable_raw_mode() {
    print("\x1b[?1049l"); // Restore main screen buffer
}

// =============================================================================
// TEXT LINE
// =============================================================================

struct Line {
    data: [u8; MAX_LINE_LEN],
    len: usize,
}

impl Line {
    const fn new() -> Self {
        Self {
            data: [0; MAX_LINE_LEN],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn get(&self) -> &[u8] {
        &self.data[..self.len]
    }

    fn set(&mut self, data: &[u8]) {
        let len = data.len().min(MAX_LINE_LEN);
        self.data[..len].copy_from_slice(&data[..len]);
        self.len = len;
    }

    fn insert(&mut self, pos: usize, ch: u8) -> bool {
        if self.len >= MAX_LINE_LEN - 1 || pos > self.len {
            return false;
        }

        // Shift characters right
        for i in (pos..self.len).rev() {
            self.data[i + 1] = self.data[i];
        }

        self.data[pos] = ch;
        self.len += 1;
        true
    }

    fn insert_str(&mut self, pos: usize, s: &[u8]) -> bool {
        if self.len + s.len() > MAX_LINE_LEN || pos > self.len {
            return false;
        }

        // Shift characters right
        for i in (pos..self.len).rev() {
            self.data[i + s.len()] = self.data[i];
        }

        self.data[pos..pos + s.len()].copy_from_slice(s);
        self.len += s.len();
        true
    }

    fn delete(&mut self, pos: usize) -> bool {
        if pos >= self.len {
            return false;
        }

        // Shift characters left
        for i in pos..self.len - 1 {
            self.data[i] = self.data[i + 1];
        }

        self.len -= 1;
        true
    }

    fn split(&mut self, pos: usize) -> Line {
        let mut new_line = Line::new();
        if pos < self.len {
            new_line.set(&self.data[pos..self.len]);
            self.len = pos;
        }
        new_line
    }

    fn append(&mut self, other: &Line) {
        let copy_len = other.len.min(MAX_LINE_LEN - self.len);
        self.data[self.len..self.len + copy_len].copy_from_slice(&other.data[..copy_len]);
        self.len += copy_len;
    }
}

// =============================================================================
// UNDO OPERATION
// =============================================================================

#[derive(Clone, Copy)]
enum UndoType {
    InsertChar,
    DeleteChar,
    InsertLine,
    DeleteLine,
    SplitLine,
    JoinLine,
}

struct UndoOp {
    op_type: UndoType,
    row: usize,
    col: usize,
    ch: u8,
    line_data: [u8; MAX_LINE_LEN],
    line_len: usize,
}

impl UndoOp {
    const fn new() -> Self {
        Self {
            op_type: UndoType::InsertChar,
            row: 0,
            col: 0,
            ch: 0,
            line_data: [0; MAX_LINE_LEN],
            line_len: 0,
        }
    }
}

// =============================================================================
// CLIPBOARD
// =============================================================================

struct Clipboard {
    lines: [[u8; MAX_LINE_LEN]; 256],
    lens: [usize; 256],
    count: usize,
}

impl Clipboard {
    const fn new() -> Self {
        Self {
            lines: [[0; MAX_LINE_LEN]; 256],
            lens: [0; 256],
            count: 0,
        }
    }

    fn clear(&mut self) {
        self.count = 0;
    }

    fn add_line(&mut self, data: &[u8]) {
        if self.count < 256 {
            let len = data.len().min(MAX_LINE_LEN);
            self.lines[self.count][..len].copy_from_slice(&data[..len]);
            self.lens[self.count] = len;
            self.count += 1;
        }
    }

    fn get_line(&self, idx: usize) -> Option<&[u8]> {
        if idx < self.count {
            Some(&self.lines[idx][..self.lens[idx]])
        } else {
            None
        }
    }
}

// =============================================================================
// EDITOR STATE
// =============================================================================

struct Editor {
    // File content
    lines: [Line; MAX_LINES],
    num_lines: usize,

    // Cursor position
    cursor_row: usize,
    cursor_col: usize,

    // Scroll offset
    scroll_row: usize,
    scroll_col: usize,

    // Terminal size
    term_rows: usize,
    term_cols: usize,

    // File info
    filename: [u8; MAX_FILENAME],
    filename_len: usize,
    modified: bool,

    // Status message
    status_msg: [u8; MAX_STATUS],
    status_len: usize,

    // Search
    search_query: [u8; MAX_SEARCH],
    search_len: usize,
    search_active: bool,
    search_direction: i8, // 1 = forward, -1 = backward

    // Selection
    select_active: bool,
    select_start_row: usize,
    select_start_col: usize,

    // Clipboard
    clipboard: Clipboard,

    // Undo stack
    undo_stack: [UndoOp; UNDO_STACK_SIZE],
    undo_count: usize,

    // Mode
    mode: EditorMode,

    // Line numbers
    show_line_numbers: bool,
    line_number_width: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum EditorMode {
    Normal,
    Search,
    Replace,
    GotoLine,
    Help,
    Save,
}

impl Editor {
    fn new() -> Self {
        const LINE_INIT: Line = Line::new();
        const UNDO_INIT: UndoOp = UndoOp::new();

        let mut editor = Self {
            lines: [LINE_INIT; MAX_LINES],
            num_lines: 1,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            term_rows: 24,
            term_cols: 80,
            filename: [0; MAX_FILENAME],
            filename_len: 0,
            modified: false,
            status_msg: [0; MAX_STATUS],
            status_len: 0,
            search_query: [0; MAX_SEARCH],
            search_len: 0,
            search_active: false,
            search_direction: 1,
            select_active: false,
            select_start_row: 0,
            select_start_col: 0,
            clipboard: Clipboard::new(),
            undo_stack: [UNDO_INIT; UNDO_STACK_SIZE],
            undo_count: 0,
            mode: EditorMode::Normal,
            show_line_numbers: true,
            line_number_width: 4,
        };

        let (rows, cols) = get_terminal_size();
        editor.term_rows = rows;
        editor.term_cols = cols;

        editor
    }

    fn set_status(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        let len = bytes.len().min(MAX_STATUS);
        self.status_msg[..len].copy_from_slice(&bytes[..len]);
        self.status_len = len;
    }

    fn set_filename(&mut self, name: &[u8]) {
        let len = name.len().min(MAX_FILENAME);
        self.filename[..len].copy_from_slice(&name[..len]);
        self.filename_len = len;
    }

    fn get_filename(&self) -> &[u8] {
        &self.filename[..self.filename_len]
    }

    fn current_line(&self) -> &Line {
        &self.lines[self.cursor_row]
    }

    fn current_line_mut(&mut self) -> &mut Line {
        &mut self.lines[self.cursor_row]
    }

    fn text_area_height(&self) -> usize {
        if self.term_rows > 2 {
            self.term_rows - 2 // Reserve 2 lines for status bar
        } else {
            1
        }
    }

    fn text_area_width(&self) -> usize {
        if self.show_line_numbers {
            self.term_cols.saturating_sub(self.line_number_width + 1)
        } else {
            self.term_cols
        }
    }

    fn update_line_number_width(&mut self) {
        let mut width = 1;
        let mut n = self.num_lines;
        while n >= 10 {
            width += 1;
            n /= 10;
        }
        self.line_number_width = width.max(4);
    }

    fn push_undo(&mut self, op: UndoOp) {
        if self.undo_count < UNDO_STACK_SIZE {
            self.undo_stack[self.undo_count] = op;
            self.undo_count += 1;
        } else {
            // Shift stack
            for i in 0..UNDO_STACK_SIZE - 1 {
                self.undo_stack[i] = core::mem::replace(&mut self.undo_stack[i + 1], UndoOp::new());
            }
            self.undo_stack[UNDO_STACK_SIZE - 1] = op;
        }
    }

    fn pop_undo(&mut self) -> Option<UndoOp> {
        if self.undo_count > 0 {
            self.undo_count -= 1;
            Some(core::mem::replace(&mut self.undo_stack[self.undo_count], UndoOp::new()))
        } else {
            None
        }
    }
}

// =============================================================================
// FILE I/O
// =============================================================================

fn load_file(editor: &mut Editor, path: &[u8]) -> bool {
    // Make null-terminated copy
    let mut path_buf = [0u8; MAX_FILENAME + 1];
    let len = path.len().min(MAX_FILENAME);
    path_buf[..len].copy_from_slice(&path[..len]);
    path_buf[len] = 0;

    let fd = open(&path_buf, O_RDONLY, 0);
    if fd < 0 {
        return false;
    }

    editor.set_filename(path);
    editor.num_lines = 0;

    let mut buf = [0u8; 4096];
    let mut line_buf = [0u8; MAX_LINE_LEN];
    let mut line_len = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                if editor.num_lines < MAX_LINES {
                    editor.lines[editor.num_lines].set(&line_buf[..line_len]);
                    editor.num_lines += 1;
                }
                line_len = 0;
            } else if buf[i] != b'\r' {
                if line_len < MAX_LINE_LEN {
                    line_buf[line_len] = buf[i];
                    line_len += 1;
                }
            }
        }
    }

    // Add remaining content as last line
    if line_len > 0 || editor.num_lines == 0 {
        if editor.num_lines < MAX_LINES {
            editor.lines[editor.num_lines].set(&line_buf[..line_len]);
            editor.num_lines += 1;
        }
    }

    if editor.num_lines == 0 {
        editor.num_lines = 1;
    }

    close(fd);
    editor.modified = false;
    editor.update_line_number_width();
    true
}

fn save_file(editor: &mut Editor) -> bool {
    if editor.filename_len == 0 {
        return false;
    }

    // Make null-terminated copy
    let mut path_buf = [0u8; MAX_FILENAME + 1];
    path_buf[..editor.filename_len].copy_from_slice(&editor.filename[..editor.filename_len]);
    path_buf[editor.filename_len] = 0;

    let fd = open(&path_buf, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if fd < 0 {
        return false;
    }

    for i in 0..editor.num_lines {
        let line = &editor.lines[i];
        write(fd, line.get());
        if i < editor.num_lines - 1 {
            write(fd, b"\n");
        }
    }

    close(fd);
    editor.modified = false;
    editor.set_status("File saved");
    true
}

// =============================================================================
// RENDERING
// =============================================================================

fn render(editor: &Editor) {
    hide_cursor();
    move_cursor(0, 0);

    let height = editor.text_area_height();
    let width = editor.text_area_width();

    // Render each visible line
    for screen_row in 0..height {
        let file_row = editor.scroll_row + screen_row;

        if file_row < editor.num_lines {
            // Render line number
            if editor.show_line_numbers {
                set_color(90, 40); // Dark gray on black
                let num_str = format_num(file_row + 1);
                // Right-align line number
                let padding = editor.line_number_width.saturating_sub(num_str.len());
                for _ in 0..padding {
                    print(" ");
                }
                print_bytes(&num_str);
                print(" ");
                reset_color();
            }

            // Render line content
            let line = &editor.lines[file_row];
            let line_data = line.get();

            let mut col = 0;
            let start_col = editor.scroll_col;
            let end_col = start_col + width;

            for (i, &ch) in line_data.iter().enumerate() {
                if col >= end_col {
                    break;
                }

                if ch == b'\t' {
                    // Expand tabs
                    let tab_stop = TAB_SIZE - (col % TAB_SIZE);
                    for _ in 0..tab_stop {
                        if col >= start_col && col < end_col {
                            print(" ");
                        }
                        col += 1;
                    }
                } else {
                    if col >= start_col {
                        // Check if this position is selected
                        let is_selected = editor.select_active && is_position_selected(
                            editor, file_row, i
                        );

                        if is_selected {
                            invert_colors();
                        }

                        // Apply syntax highlighting (basic)
                        if is_keyword(line_data, i) {
                            set_color(33, 40); // Yellow
                        } else if ch == b'"' || ch == b'\'' {
                            set_color(32, 40); // Green
                        } else if ch >= b'0' && ch <= b'9' {
                            set_color(36, 40); // Cyan
                        }

                        write(1, &[ch]);
                        reset_color();
                    }
                    col += 1;
                }
            }

            // Clear rest of line
            clear_line();
        } else {
            // Empty line (beyond file)
            if editor.show_line_numbers {
                set_color(90, 40);
                for _ in 0..editor.line_number_width {
                    print(" ");
                }
                print(" ");
                reset_color();
            }
            set_color(34, 40); // Blue
            print("~");
            reset_color();
            clear_line();
        }

        print("\r\n");
    }

    // Render status bar
    render_status_bar(editor);

    // Render help bar
    render_help_bar(editor);

    // Position cursor
    let cursor_screen_row = editor.cursor_row - editor.scroll_row;
    let cursor_screen_col = if editor.show_line_numbers {
        editor.line_number_width + 1 + editor.cursor_col - editor.scroll_col
    } else {
        editor.cursor_col - editor.scroll_col
    };

    move_cursor(cursor_screen_row, cursor_screen_col);
    show_cursor();
}

fn format_num(n: usize) -> [u8; 10] {
    let mut buf = [0u8; 10];
    let mut num = n;
    let mut i = 9;

    if num == 0 {
        buf[9] = b'0';
        return buf;
    }

    while num > 0 && i > 0 {
        buf[i] = b'0' + (num % 10) as u8;
        num /= 10;
        i -= 1;
    }

    // Shift to beginning
    let start = i + 1;
    let len = 10 - start;
    let mut result = [0u8; 10];
    result[..len].copy_from_slice(&buf[start..]);
    result
}

fn is_position_selected(editor: &Editor, row: usize, col: usize) -> bool {
    if !editor.select_active {
        return false;
    }

    let (start_row, start_col, end_row, end_col) = if editor.select_start_row < editor.cursor_row
        || (editor.select_start_row == editor.cursor_row && editor.select_start_col <= editor.cursor_col)
    {
        (editor.select_start_row, editor.select_start_col, editor.cursor_row, editor.cursor_col)
    } else {
        (editor.cursor_row, editor.cursor_col, editor.select_start_row, editor.select_start_col)
    };

    if row < start_row || row > end_row {
        return false;
    }

    if row == start_row && row == end_row {
        col >= start_col && col < end_col
    } else if row == start_row {
        col >= start_col
    } else if row == end_row {
        col < end_col
    } else {
        true
    }
}

fn is_keyword(line: &[u8], pos: usize) -> bool {
    // Simple keyword detection for common languages
    let keywords: &[&[u8]] = &[
        b"fn", b"let", b"mut", b"const", b"if", b"else", b"while", b"for",
        b"match", b"return", b"struct", b"enum", b"impl", b"pub", b"use",
        b"mod", b"self", b"true", b"false", b"loop", b"break", b"continue",
        b"async", b"await", b"type", b"trait", b"where", b"static",
        b"function", b"var", b"class", b"extends", b"import", b"export",
        b"def", b"elif", b"pass", b"try", b"except", b"finally",
        b"int", b"void", b"char", b"float", b"double", b"long",
    ];

    // Check if we're at the start of a word
    if pos > 0 && is_identifier_char(line[pos - 1]) {
        return false;
    }

    for &kw in keywords {
        if pos + kw.len() <= line.len() {
            let matches = &line[pos..pos + kw.len()] == kw;
            let ends_word = pos + kw.len() >= line.len() || !is_identifier_char(line[pos + kw.len()]);
            if matches && ends_word {
                return true;
            }
        }
    }

    false
}

fn is_identifier_char(ch: u8) -> bool {
    (ch >= b'a' && ch <= b'z') || (ch >= b'A' && ch <= b'Z') ||
    (ch >= b'0' && ch <= b'9') || ch == b'_'
}

fn render_status_bar(editor: &Editor) {
    invert_colors();

    // Left side: filename and modified indicator
    if editor.filename_len > 0 {
        print_bytes(editor.get_filename());
    } else {
        print("[New File]");
    }

    if editor.modified {
        print(" [Modified]");
    }

    // Middle: status message
    if editor.status_len > 0 {
        print(" - ");
        print_bytes(&editor.status_msg[..editor.status_len]);
    }

    // Calculate used width
    let left_len = editor.filename_len + if editor.modified { 11 } else { 0 } + editor.status_len + 3;

    // Right side: position info
    let pos_str = format_position(editor.cursor_row + 1, editor.cursor_col + 1, editor.num_lines);

    // Pad middle
    let right_len = pos_str.iter().take_while(|&&c| c != 0).count();
    let padding = editor.term_cols.saturating_sub(left_len + right_len);
    for _ in 0..padding {
        print(" ");
    }

    // Print position
    for &ch in pos_str.iter() {
        if ch == 0 {
            break;
        }
        write(1, &[ch]);
    }

    reset_color();
    print("\r\n");
}

fn format_position(row: usize, col: usize, total: usize) -> [u8; 32] {
    let mut buf = [0u8; 32];
    let mut pos = 0;

    // "Line X/Y, Col Z"
    let prefix = b"Line ";
    buf[pos..pos + prefix.len()].copy_from_slice(prefix);
    pos += prefix.len();

    pos += write_num_to_buf(&mut buf[pos..], row);
    buf[pos] = b'/';
    pos += 1;
    pos += write_num_to_buf(&mut buf[pos..], total);

    let mid = b", Col ";
    buf[pos..pos + mid.len()].copy_from_slice(mid);
    pos += mid.len();

    pos += write_num_to_buf(&mut buf[pos..], col);

    buf
}

fn write_num_to_buf(buf: &mut [u8], n: usize) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut temp = [0u8; 20];
    let mut i = 20;
    let mut num = n;

    while num > 0 {
        i -= 1;
        temp[i] = b'0' + (num % 10) as u8;
        num /= 10;
    }

    let len = 20 - i;
    buf[..len].copy_from_slice(&temp[i..]);
    len
}

fn render_help_bar(editor: &Editor) {
    set_color(30, 47); // Black on white

    match editor.mode {
        EditorMode::Normal => {
            print("^G Help  ^O Save  ^X Exit  ^K Cut  ^U Paste  ^W Search  ^C Cancel");
        }
        EditorMode::Search => {
            print("Search: ");
            print_bytes(&editor.search_query[..editor.search_len]);
            print("  [Enter] Find  [^C] Cancel");
        }
        EditorMode::GotoLine => {
            print("Go to line: ");
            print_bytes(&editor.search_query[..editor.search_len]);
            print("  [Enter] Go  [^C] Cancel");
        }
        EditorMode::Help => {
            print("Press any key to exit help");
        }
        EditorMode::Save => {
            print("Save as: ");
            print_bytes(&editor.filename[..editor.filename_len]);
            print("  [Enter] Save  [^C] Cancel");
        }
        _ => {}
    }

    clear_line();
    reset_color();
}

// =============================================================================
// CURSOR MOVEMENT
// =============================================================================

fn move_cursor_up(editor: &mut Editor) {
    if editor.cursor_row > 0 {
        editor.cursor_row -= 1;
        clamp_cursor_col(editor);
    }
    scroll_to_cursor(editor);
}

fn move_cursor_down(editor: &mut Editor) {
    if editor.cursor_row < editor.num_lines - 1 {
        editor.cursor_row += 1;
        clamp_cursor_col(editor);
    }
    scroll_to_cursor(editor);
}

fn move_cursor_left(editor: &mut Editor) {
    if editor.cursor_col > 0 {
        editor.cursor_col -= 1;
    } else if editor.cursor_row > 0 {
        editor.cursor_row -= 1;
        editor.cursor_col = editor.lines[editor.cursor_row].len;
    }
    scroll_to_cursor(editor);
}

fn move_cursor_right(editor: &mut Editor) {
    let line_len = editor.lines[editor.cursor_row].len;
    if editor.cursor_col < line_len {
        editor.cursor_col += 1;
    } else if editor.cursor_row < editor.num_lines - 1 {
        editor.cursor_row += 1;
        editor.cursor_col = 0;
    }
    scroll_to_cursor(editor);
}

fn move_cursor_home(editor: &mut Editor) {
    editor.cursor_col = 0;
    scroll_to_cursor(editor);
}

fn move_cursor_end(editor: &mut Editor) {
    editor.cursor_col = editor.lines[editor.cursor_row].len;
    scroll_to_cursor(editor);
}

fn move_cursor_page_up(editor: &mut Editor) {
    let page_size = editor.text_area_height();
    if editor.cursor_row >= page_size {
        editor.cursor_row -= page_size;
    } else {
        editor.cursor_row = 0;
    }
    clamp_cursor_col(editor);
    scroll_to_cursor(editor);
}

fn move_cursor_page_down(editor: &mut Editor) {
    let page_size = editor.text_area_height();
    editor.cursor_row += page_size;
    if editor.cursor_row >= editor.num_lines {
        editor.cursor_row = editor.num_lines - 1;
    }
    clamp_cursor_col(editor);
    scroll_to_cursor(editor);
}

fn move_cursor_word_left(editor: &mut Editor) {
    if editor.cursor_col == 0 && editor.cursor_row > 0 {
        editor.cursor_row -= 1;
        editor.cursor_col = editor.lines[editor.cursor_row].len;
    }

    let line = editor.lines[editor.cursor_row].get();

    // Skip whitespace
    while editor.cursor_col > 0 && !is_identifier_char(line[editor.cursor_col - 1]) {
        editor.cursor_col -= 1;
    }

    // Skip word
    while editor.cursor_col > 0 && is_identifier_char(line[editor.cursor_col - 1]) {
        editor.cursor_col -= 1;
    }

    scroll_to_cursor(editor);
}

fn move_cursor_word_right(editor: &mut Editor) {
    let line = editor.lines[editor.cursor_row].get();
    let line_len = line.len();

    // Skip current word
    while editor.cursor_col < line_len && is_identifier_char(line[editor.cursor_col]) {
        editor.cursor_col += 1;
    }

    // Skip whitespace
    while editor.cursor_col < line_len && !is_identifier_char(line[editor.cursor_col]) {
        editor.cursor_col += 1;
    }

    if editor.cursor_col >= line_len && editor.cursor_row < editor.num_lines - 1 {
        editor.cursor_row += 1;
        editor.cursor_col = 0;
    }

    scroll_to_cursor(editor);
}

fn clamp_cursor_col(editor: &mut Editor) {
    let line_len = editor.lines[editor.cursor_row].len;
    if editor.cursor_col > line_len {
        editor.cursor_col = line_len;
    }
}

fn scroll_to_cursor(editor: &mut Editor) {
    let height = editor.text_area_height();
    let width = editor.text_area_width();

    // Vertical scroll
    if editor.cursor_row < editor.scroll_row {
        editor.scroll_row = editor.cursor_row;
    } else if editor.cursor_row >= editor.scroll_row + height {
        editor.scroll_row = editor.cursor_row - height + 1;
    }

    // Horizontal scroll
    if editor.cursor_col < editor.scroll_col {
        editor.scroll_col = editor.cursor_col;
    } else if editor.cursor_col >= editor.scroll_col + width {
        editor.scroll_col = editor.cursor_col - width + 1;
    }
}

// =============================================================================
// TEXT EDITING
// =============================================================================

fn insert_char(editor: &mut Editor, ch: u8) {
    let line = &mut editor.lines[editor.cursor_row];
    if line.insert(editor.cursor_col, ch) {
        // Record undo
        let mut op = UndoOp::new();
        op.op_type = UndoType::InsertChar;
        op.row = editor.cursor_row;
        op.col = editor.cursor_col;
        op.ch = ch;
        editor.push_undo(op);

        editor.cursor_col += 1;
        editor.modified = true;
    }
    scroll_to_cursor(editor);
}

fn insert_newline(editor: &mut Editor) {
    if editor.num_lines >= MAX_LINES {
        return;
    }

    // Split current line
    let new_line = editor.lines[editor.cursor_row].split(editor.cursor_col);

    // Shift lines down
    for i in (editor.cursor_row + 1..editor.num_lines).rev() {
        let (left, right) = editor.lines.split_at_mut(i + 1);
        core::mem::swap(&mut left[i], &mut right[0]);
    }

    // Insert new line
    if editor.cursor_row + 1 < MAX_LINES {
        editor.lines[editor.cursor_row + 1] = new_line;
    }

    editor.num_lines += 1;
    editor.cursor_row += 1;
    editor.cursor_col = 0;
    editor.modified = true;
    editor.update_line_number_width();

    // Auto-indent
    let prev_line = editor.lines[editor.cursor_row - 1].get();
    let mut indent = 0;
    for &ch in prev_line {
        if ch == b' ' {
            indent += 1;
        } else if ch == b'\t' {
            indent += TAB_SIZE;
        } else {
            break;
        }
    }

    for _ in 0..indent {
        insert_char(editor, b' ');
    }

    scroll_to_cursor(editor);
}

fn delete_char(editor: &mut Editor) {
    if editor.cursor_col > 0 {
        // Delete character before cursor
        let ch = editor.lines[editor.cursor_row].data[editor.cursor_col - 1];
        editor.lines[editor.cursor_row].delete(editor.cursor_col - 1);
        editor.cursor_col -= 1;
        editor.modified = true;

        // Record undo
        let mut op = UndoOp::new();
        op.op_type = UndoType::DeleteChar;
        op.row = editor.cursor_row;
        op.col = editor.cursor_col;
        op.ch = ch;
        editor.push_undo(op);
    } else if editor.cursor_row > 0 {
        // Join with previous line
        let current_line = core::mem::replace(&mut editor.lines[editor.cursor_row], Line::new());
        let prev_len = editor.lines[editor.cursor_row - 1].len;
        editor.lines[editor.cursor_row - 1].append(&current_line);

        // Shift lines up
        for i in editor.cursor_row..editor.num_lines - 1 {
            let (left, right) = editor.lines.split_at_mut(i + 1);
            core::mem::swap(&mut left[i], &mut right[0]);
        }

        editor.num_lines -= 1;
        editor.cursor_row -= 1;
        editor.cursor_col = prev_len;
        editor.modified = true;
        editor.update_line_number_width();
    }

    scroll_to_cursor(editor);
}

fn delete_char_forward(editor: &mut Editor) {
    let line_len = editor.lines[editor.cursor_row].len;
    if editor.cursor_col < line_len {
        editor.lines[editor.cursor_row].delete(editor.cursor_col);
        editor.modified = true;
    } else if editor.cursor_row < editor.num_lines - 1 {
        // Join with next line
        let next_line = core::mem::replace(&mut editor.lines[editor.cursor_row + 1], Line::new());
        editor.lines[editor.cursor_row].append(&next_line);

        // Shift lines up
        for i in editor.cursor_row + 1..editor.num_lines - 1 {
            let (left, right) = editor.lines.split_at_mut(i + 1);
            core::mem::swap(&mut left[i], &mut right[0]);
        }

        editor.num_lines -= 1;
        editor.modified = true;
        editor.update_line_number_width();
    }
}

fn delete_line(editor: &mut Editor) {
    if editor.num_lines == 1 {
        editor.lines[0].clear();
        editor.cursor_col = 0;
    } else {
        // Shift lines up
        for i in editor.cursor_row..editor.num_lines - 1 {
            let (left, right) = editor.lines.split_at_mut(i + 1);
            core::mem::swap(&mut left[i], &mut right[0]);
        }

        editor.num_lines -= 1;
        if editor.cursor_row >= editor.num_lines {
            editor.cursor_row = editor.num_lines - 1;
        }
        clamp_cursor_col(editor);
    }

    editor.modified = true;
    editor.update_line_number_width();
    scroll_to_cursor(editor);
}

fn cut_line(editor: &mut Editor) {
    editor.clipboard.clear();
    editor.clipboard.add_line(editor.lines[editor.cursor_row].get());
    delete_line(editor);
    editor.set_status("Cut 1 line");
}

fn paste(editor: &mut Editor) {
    if editor.clipboard.count == 0 {
        return;
    }

    for i in 0..editor.clipboard.count {
        if editor.num_lines >= MAX_LINES {
            break;
        }

        if let Some(line_data) = editor.clipboard.get_line(i) {
            // Insert new line
            for j in (editor.cursor_row + 1..editor.num_lines).rev() {
                let (left, right) = editor.lines.split_at_mut(j + 1);
                core::mem::swap(&mut left[j], &mut right[0]);
            }

            editor.lines[editor.cursor_row + 1].set(line_data);
            editor.num_lines += 1;
            editor.cursor_row += 1;
        }
    }

    editor.cursor_col = 0;
    editor.modified = true;
    editor.update_line_number_width();
    scroll_to_cursor(editor);

    let mut msg = [0u8; 32];
    let prefix = b"Pasted ";
    msg[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();
    pos += write_num_to_buf(&mut msg[pos..], editor.clipboard.count);
    let suffix = b" lines";
    msg[pos..pos + suffix.len()].copy_from_slice(suffix);
    editor.status_msg[..pos + suffix.len()].copy_from_slice(&msg[..pos + suffix.len()]);
    editor.status_len = pos + suffix.len();
}

fn undo(editor: &mut Editor) {
    if let Some(op) = editor.pop_undo() {
        match op.op_type {
            UndoType::InsertChar => {
                editor.cursor_row = op.row;
                editor.cursor_col = op.col;
                editor.lines[op.row].delete(op.col);
            }
            UndoType::DeleteChar => {
                editor.cursor_row = op.row;
                editor.cursor_col = op.col;
                editor.lines[op.row].insert(op.col, op.ch);
            }
            _ => {}
        }
        editor.modified = true;
        scroll_to_cursor(editor);
        editor.set_status("Undo");
    } else {
        editor.set_status("Nothing to undo");
    }
}

// =============================================================================
// SEARCH
// =============================================================================

fn find_next(editor: &mut Editor) {
    if editor.search_len == 0 {
        return;
    }

    let query = &editor.search_query[..editor.search_len];
    let start_row = editor.cursor_row;
    let start_col = editor.cursor_col + 1;

    // Search from current position to end
    for row in start_row..editor.num_lines {
        let line = editor.lines[row].get();
        let search_start = if row == start_row { start_col } else { 0 };

        if let Some(pos) = find_in_line(line, query, search_start) {
            editor.cursor_row = row;
            editor.cursor_col = pos;
            scroll_to_cursor(editor);
            editor.set_status("Found");
            return;
        }
    }

    // Wrap around
    for row in 0..=start_row {
        let line = editor.lines[row].get();
        let search_end = if row == start_row { start_col } else { line.len() };

        if let Some(pos) = find_in_line(&line[..search_end.min(line.len())], query, 0) {
            editor.cursor_row = row;
            editor.cursor_col = pos;
            scroll_to_cursor(editor);
            editor.set_status("Found (wrapped)");
            return;
        }
    }

    editor.set_status("Not found");
}

fn find_in_line(line: &[u8], query: &[u8], start: usize) -> Option<usize> {
    if query.len() == 0 || line.len() < query.len() {
        return None;
    }

    for i in start..=line.len() - query.len() {
        if &line[i..i + query.len()] == query {
            return Some(i);
        }
    }

    None
}

fn goto_line(editor: &mut Editor, line_num: usize) {
    if line_num > 0 && line_num <= editor.num_lines {
        editor.cursor_row = line_num - 1;
        editor.cursor_col = 0;
        scroll_to_cursor(editor);
        editor.set_status("Jumped to line");
    } else {
        editor.set_status("Invalid line number");
    }
}

// =============================================================================
// INPUT HANDLING
// =============================================================================

fn handle_key(editor: &mut Editor) -> bool {
    let mut buf = [0u8; 8];
    let n = read(0, &mut buf);
    if n <= 0 {
        return true;
    }

    match editor.mode {
        EditorMode::Normal => handle_normal_key(editor, &buf[..n as usize]),
        EditorMode::Search => handle_search_key(editor, &buf[..n as usize]),
        EditorMode::GotoLine => handle_goto_key(editor, &buf[..n as usize]),
        EditorMode::Help => {
            editor.mode = EditorMode::Normal;
            true
        }
        EditorMode::Save => handle_save_key(editor, &buf[..n as usize]),
        _ => true,
    }
}

fn handle_normal_key(editor: &mut Editor, input: &[u8]) -> bool {
    if input.len() == 0 {
        return true;
    }

    match input[0] {
        // Ctrl+X - Exit
        0x18 => {
            if editor.modified {
                editor.set_status("Unsaved changes! Press Ctrl+X again to exit");
                editor.modified = false; // Allow exit on next Ctrl+X
            } else {
                return false;
            }
        }
        // Ctrl+O - Save
        0x0f => {
            if editor.filename_len == 0 {
                editor.mode = EditorMode::Save;
            } else {
                save_file(editor);
            }
        }
        // Ctrl+G - Help
        0x07 => {
            show_help(editor);
        }
        // Ctrl+W - Search
        0x17 => {
            editor.mode = EditorMode::Search;
            editor.search_len = 0;
        }
        // Ctrl+K - Cut line
        0x0b => {
            cut_line(editor);
        }
        // Ctrl+U - Paste
        0x15 => {
            paste(editor);
        }
        // Ctrl+Z - Undo
        0x1a => {
            undo(editor);
        }
        // Ctrl+L - Goto line
        0x0c => {
            editor.mode = EditorMode::GotoLine;
            editor.search_len = 0;
        }
        // Ctrl+N - Find next
        0x0e => {
            find_next(editor);
        }
        // Ctrl+A - Home
        0x01 => {
            move_cursor_home(editor);
        }
        // Ctrl+E - End
        0x05 => {
            move_cursor_end(editor);
        }
        // Ctrl+C - Cancel (clear status)
        0x03 => {
            editor.set_status("");
        }
        // Escape sequence
        0x1b => {
            if input.len() >= 3 && input[1] == b'[' {
                match input[2] {
                    b'A' => move_cursor_up(editor),       // Up
                    b'B' => move_cursor_down(editor),     // Down
                    b'C' => move_cursor_right(editor),    // Right
                    b'D' => move_cursor_left(editor),     // Left
                    b'H' => move_cursor_home(editor),     // Home
                    b'F' => move_cursor_end(editor),      // End
                    b'5' if input.len() >= 4 && input[3] == b'~' => move_cursor_page_up(editor),
                    b'6' if input.len() >= 4 && input[3] == b'~' => move_cursor_page_down(editor),
                    b'3' if input.len() >= 4 && input[3] == b'~' => delete_char_forward(editor),
                    b'1' if input.len() >= 6 && input[3] == b';' && input[4] == b'5' => {
                        // Ctrl+Arrow
                        match input[5] {
                            b'C' => move_cursor_word_right(editor),
                            b'D' => move_cursor_word_left(editor),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        // Backspace
        0x7f | 0x08 => {
            delete_char(editor);
        }
        // Enter
        b'\r' | b'\n' => {
            insert_newline(editor);
        }
        // Tab
        b'\t' => {
            for _ in 0..TAB_SIZE {
                insert_char(editor, b' ');
            }
        }
        // Printable characters
        ch if ch >= 0x20 && ch < 0x7f => {
            insert_char(editor, ch);
        }
        _ => {}
    }

    true
}

fn handle_search_key(editor: &mut Editor, input: &[u8]) -> bool {
    if input.len() == 0 {
        return true;
    }

    match input[0] {
        // Enter - Find
        b'\r' | b'\n' => {
            find_next(editor);
            editor.mode = EditorMode::Normal;
        }
        // Ctrl+C or Escape - Cancel
        0x03 | 0x1b => {
            editor.mode = EditorMode::Normal;
            editor.set_status("Search cancelled");
        }
        // Backspace
        0x7f | 0x08 => {
            if editor.search_len > 0 {
                editor.search_len -= 1;
            }
        }
        // Printable characters
        ch if ch >= 0x20 && ch < 0x7f => {
            if editor.search_len < MAX_SEARCH - 1 {
                editor.search_query[editor.search_len] = ch;
                editor.search_len += 1;
            }
        }
        _ => {}
    }

    true
}

fn handle_goto_key(editor: &mut Editor, input: &[u8]) -> bool {
    if input.len() == 0 {
        return true;
    }

    match input[0] {
        // Enter - Go
        b'\r' | b'\n' => {
            let line_num = parse_number(&editor.search_query[..editor.search_len]);
            goto_line(editor, line_num);
            editor.mode = EditorMode::Normal;
        }
        // Ctrl+C or Escape - Cancel
        0x03 | 0x1b => {
            editor.mode = EditorMode::Normal;
        }
        // Backspace
        0x7f | 0x08 => {
            if editor.search_len > 0 {
                editor.search_len -= 1;
            }
        }
        // Digits only
        ch if ch >= b'0' && ch <= b'9' => {
            if editor.search_len < MAX_SEARCH - 1 {
                editor.search_query[editor.search_len] = ch;
                editor.search_len += 1;
            }
        }
        _ => {}
    }

    true
}

fn handle_save_key(editor: &mut Editor, input: &[u8]) -> bool {
    if input.len() == 0 {
        return true;
    }

    match input[0] {
        // Enter - Save
        b'\r' | b'\n' => {
            if editor.filename_len > 0 {
                if save_file(editor) {
                    editor.set_status("File saved");
                } else {
                    editor.set_status("Error saving file");
                }
            }
            editor.mode = EditorMode::Normal;
        }
        // Ctrl+C or Escape - Cancel
        0x03 | 0x1b => {
            editor.mode = EditorMode::Normal;
            editor.set_status("Save cancelled");
        }
        // Backspace
        0x7f | 0x08 => {
            if editor.filename_len > 0 {
                editor.filename_len -= 1;
            }
        }
        // Printable characters
        ch if ch >= 0x20 && ch < 0x7f => {
            if editor.filename_len < MAX_FILENAME - 1 {
                editor.filename[editor.filename_len] = ch;
                editor.filename_len += 1;
            }
        }
        _ => {}
    }

    true
}

fn parse_number(s: &[u8]) -> usize {
    let mut result = 0usize;
    for &ch in s {
        if ch >= b'0' && ch <= b'9' {
            result = result * 10 + (ch - b'0') as usize;
        }
    }
    result
}

// =============================================================================
// HELP SCREEN
// =============================================================================

fn show_help(editor: &mut Editor) {
    clear_screen();
    move_cursor(0, 0);

    print("\n");
    print("  QEdit - BuildOS Text Editor\n");
    print("  ============================\n\n");
    print("  Keyboard Shortcuts:\n\n");
    print("  Navigation:\n");
    print("    Arrow keys    Move cursor\n");
    print("    Home/End      Start/end of line\n");
    print("    PgUp/PgDn     Page up/down\n");
    print("    Ctrl+A        Start of line\n");
    print("    Ctrl+E        End of line\n");
    print("    Ctrl+Left/Right  Word left/right\n\n");
    print("  Editing:\n");
    print("    Backspace     Delete character\n");
    print("    Delete        Delete forward\n");
    print("    Ctrl+K        Cut line\n");
    print("    Ctrl+U        Paste\n");
    print("    Ctrl+Z        Undo\n\n");
    print("  File:\n");
    print("    Ctrl+O        Save file\n");
    print("    Ctrl+X        Exit editor\n\n");
    print("  Search:\n");
    print("    Ctrl+W        Search\n");
    print("    Ctrl+N        Find next\n");
    print("    Ctrl+L        Go to line\n\n");
    print("  Other:\n");
    print("    Ctrl+G        Show this help\n");
    print("    Ctrl+C        Cancel / Clear status\n\n");
    print("  Press any key to continue...\n");

    editor.mode = EditorMode::Help;
}

// =============================================================================
// MAIN
// =============================================================================

#[no_mangle]
pub extern "C" fn _start() -> ! {
    main();
    exit(0);
}

fn main() {
    // Parse command line arguments
    // In BuildOS, we'd get these from argc/argv
    // For now, just start with empty buffer or try to read filename

    let mut editor = Editor::new();

    // Try to load file from command line (simplified - hardcoded for now)
    // In real implementation, would parse argv

    enable_raw_mode();
    clear_screen();

    editor.set_status("Welcome to QEdit. Press Ctrl+G for help.");

    // Main loop
    loop {
        render(&editor);
        if !handle_key(&mut editor) {
            break;
        }
    }

    // Cleanup
    disable_raw_mode();
    clear_screen();
    move_cursor(0, 0);
}

// =============================================================================
// PANIC HANDLER
// =============================================================================

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    disable_raw_mode();
    clear_screen();
    move_cursor(0, 0);
    print("Editor panic!\n");
    exit(1);
}
