// ===============================================================================
// BUILDOS SHELL - WITH SCRIPTING SUPPORT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Full-featured interactive command interpreter with scripting support:
// - Variables and environment
// - Control flow (if/else, while, for, case)
// - Functions
// - Pipelines and redirections
// - Command substitution
// - Job control
// - Script file execution
//
// ===============================================================================

#![no_std]
#![no_main]
#![allow(unused_variables)]
#![allow(unused_assignments)]
#![allow(unused_mut)]
#![allow(dead_code)]

use core::panic::PanicInfo;

// =============================================================================
// SYSCALL NUMBERS (matching kernel)
// =============================================================================

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_LSEEK: u64 = 8;
const SYS_DUP: u64 = 32;
const SYS_DUP2: u64 = 33;
const SYS_PIPE: u64 = 22;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_UNAME: u64 = 63;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_GETENV: u64 = 200;
const SYS_SETENV: u64 = 201;
const SYS_UNSETENV: u64 = 202;
const SYS_KILL: u64 = 62;

// File flags
const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0o100;
const O_TRUNC: u64 = 0o1000;
const O_APPEND: u64 = 0o2000;

// =============================================================================
// CONSTANTS
// =============================================================================

const MAX_VARS: usize = 256;
const MAX_VAR_NAME: usize = 64;
const MAX_VAR_VALUE: usize = 1024;
const MAX_FUNCTIONS: usize = 64;
const MAX_FUNC_BODY: usize = 4096;
const MAX_ARGS: usize = 64;
const MAX_JOBS: usize = 32;
const MAX_LINE: usize = 4096;
const MAX_PATH: usize = 512;
const MAX_ALIAS: usize = 64;
const HISTORY_SIZE: usize = 100;

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

fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    unsafe { syscall(SYS_LSEEK, fd as u64, offset as u64, whence as u64, 0, 0, 0) }
}

fn dup(fd: i32) -> i32 {
    unsafe { syscall(SYS_DUP, fd as u64, 0, 0, 0, 0, 0) as i32 }
}

fn dup2(oldfd: i32, newfd: i32) -> i32 {
    unsafe { syscall(SYS_DUP2, oldfd as u64, newfd as u64, 0, 0, 0, 0) as i32 }
}

fn pipe(fds: &mut [i32; 2]) -> i32 {
    unsafe { syscall(SYS_PIPE, fds.as_mut_ptr() as u64, 0, 0, 0, 0, 0) as i32 }
}

fn getpid() -> i32 {
    unsafe { syscall(SYS_GETPID, 0, 0, 0, 0, 0, 0) as i32 }
}

fn fork() -> i32 {
    unsafe { syscall(SYS_FORK, 0, 0, 0, 0, 0, 0) as i32 }
}

fn wait(status: &mut i32) -> i32 {
    unsafe { syscall(SYS_WAIT4, u64::MAX, status as *mut i32 as u64, 0, 0, 0, 0) as i32 }
}

fn waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    unsafe { syscall(SYS_WAIT4, pid as u64, status as *mut i32 as u64, options as u64, 0, 0, 0) as i32 }
}

fn exit(code: i32) -> ! {
    unsafe { syscall(SYS_EXIT, code as u64, 0, 0, 0, 0, 0) };
    loop {}
}

fn execve(path: &[u8], argv: u64, envp: u64) -> i32 {
    unsafe { syscall(SYS_EXECVE, path.as_ptr() as u64, argv, envp, 0, 0, 0) as i32 }
}

fn getcwd(buf: &mut [u8]) -> isize {
    unsafe { syscall(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0, 0, 0) as isize }
}

fn chdir(path: &[u8]) -> i32 {
    unsafe { syscall(SYS_CHDIR, path.as_ptr() as u64, 0, 0, 0, 0, 0) as i32 }
}

fn kill(pid: i32, sig: i32) -> i32 {
    unsafe { syscall(SYS_KILL, pid as u64, sig as u64, 0, 0, 0, 0) as i32 }
}

// =============================================================================
// OUTPUT UTILITIES
// =============================================================================

fn print(s: &str) {
    write(1, s.as_bytes());
}

fn println(s: &str) {
    print(s);
    print("\n");
}

fn eprint(s: &str) {
    write(2, s.as_bytes());
}

fn eprintln(s: &str) {
    eprint(s);
    eprint("\n");
}

fn print_bytes(buf: &[u8]) {
    write(1, buf);
}

fn eprint_bytes(buf: &[u8]) {
    write(2, buf);
}

fn print_num(mut n: i64) {
    if n == 0 {
        print("0");
        return;
    }

    if n < 0 {
        print("-");
        n = -n;
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
// STRING UTILITIES
// =============================================================================

fn str_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

fn str_starts_with(s: &[u8], prefix: &[u8]) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    str_eq(&s[..prefix.len()], prefix)
}

fn trim(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();

    while start < end && is_whitespace(s[start]) {
        start += 1;
    }

    while end > start && is_whitespace(s[end - 1]) {
        end -= 1;
    }

    &s[start..end]
}

fn is_whitespace(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n' || c == b'\r'
}

fn is_digit(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

fn is_alpha(c: u8) -> bool {
    (c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z')
}

fn is_alnum(c: u8) -> bool {
    is_alpha(c) || is_digit(c)
}

fn is_identifier_char(c: u8) -> bool {
    is_alnum(c) || c == b'_'
}

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let (negative, start) = if s[0] == b'-' {
        (true, 1)
    } else if s[0] == b'+' {
        (false, 1)
    } else {
        (false, 0)
    };

    if start >= s.len() {
        return None;
    }

    let mut result: i64 = 0;
    for i in start..s.len() {
        if !is_digit(s[i]) {
            return None;
        }
        result = result * 10 + (s[i] - b'0') as i64;
    }

    if negative {
        Some(-result)
    } else {
        Some(result)
    }
}

fn copy_slice(dst: &mut [u8], src: &[u8]) -> usize {
    let len = dst.len().min(src.len());
    dst[..len].copy_from_slice(&src[..len]);
    len
}

// =============================================================================
// SHELL VARIABLE
// =============================================================================

struct Variable {
    name: [u8; MAX_VAR_NAME],
    name_len: usize,
    value: [u8; MAX_VAR_VALUE],
    value_len: usize,
    exported: bool,
    readonly: bool,
}

impl Variable {
    const fn new() -> Self {
        Self {
            name: [0; MAX_VAR_NAME],
            name_len: 0,
            value: [0; MAX_VAR_VALUE],
            value_len: 0,
            exported: false,
            readonly: false,
        }
    }

    fn set_name(&mut self, name: &[u8]) {
        self.name_len = copy_slice(&mut self.name, name);
    }

    fn set_value(&mut self, value: &[u8]) {
        self.value_len = copy_slice(&mut self.value, value);
    }

    fn get_name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    fn get_value(&self) -> &[u8] {
        &self.value[..self.value_len]
    }

    fn is_empty(&self) -> bool {
        self.name_len == 0
    }
}

// =============================================================================
// SHELL FUNCTION
// =============================================================================

struct Function {
    name: [u8; MAX_VAR_NAME],
    name_len: usize,
    body: [u8; MAX_FUNC_BODY],
    body_len: usize,
}

impl Function {
    const fn new() -> Self {
        Self {
            name: [0; MAX_VAR_NAME],
            name_len: 0,
            body: [0; MAX_FUNC_BODY],
            body_len: 0,
        }
    }

    fn set_name(&mut self, name: &[u8]) {
        self.name_len = copy_slice(&mut self.name, name);
    }

    fn set_body(&mut self, body: &[u8]) {
        self.body_len = copy_slice(&mut self.body, body);
    }

    fn get_name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    fn get_body(&self) -> &[u8] {
        &self.body[..self.body_len]
    }

    fn is_empty(&self) -> bool {
        self.name_len == 0
    }
}

// =============================================================================
// SHELL ALIAS
// =============================================================================

struct Alias {
    name: [u8; MAX_VAR_NAME],
    name_len: usize,
    value: [u8; MAX_VAR_VALUE],
    value_len: usize,
}

impl Alias {
    const fn new() -> Self {
        Self {
            name: [0; MAX_VAR_NAME],
            name_len: 0,
            value: [0; MAX_VAR_VALUE],
            value_len: 0,
        }
    }

    fn set_name(&mut self, name: &[u8]) {
        self.name_len = copy_slice(&mut self.name, name);
    }

    fn set_value(&mut self, value: &[u8]) {
        self.value_len = copy_slice(&mut self.value, value);
    }

    fn get_name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    fn get_value(&self) -> &[u8] {
        &self.value[..self.value_len]
    }

    fn is_empty(&self) -> bool {
        self.name_len == 0
    }
}

// =============================================================================
// JOB CONTROL
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobStatus {
    Running,
    Stopped,
    Done,
    Terminated,
}

struct Job {
    pid: i32,
    job_id: i32,
    command: [u8; 256],
    command_len: usize,
    status: JobStatus,
}

impl Job {
    const fn new() -> Self {
        Self {
            pid: 0,
            job_id: 0,
            command: [0; 256],
            command_len: 0,
            status: JobStatus::Done,
        }
    }

    fn is_active(&self) -> bool {
        self.pid > 0 && (self.status == JobStatus::Running || self.status == JobStatus::Stopped)
    }
}

// =============================================================================
// COMMAND HISTORY
// =============================================================================

struct History {
    entries: [[u8; MAX_LINE]; HISTORY_SIZE],
    lengths: [usize; HISTORY_SIZE],
    head: usize,
    count: usize,
}

impl History {
    const fn new() -> Self {
        Self {
            entries: [[0; MAX_LINE]; HISTORY_SIZE],
            lengths: [0; HISTORY_SIZE],
            head: 0,
            count: 0,
        }
    }

    fn add(&mut self, cmd: &[u8]) {
        if cmd.is_empty() {
            return;
        }

        let idx = self.head;
        self.lengths[idx] = copy_slice(&mut self.entries[idx], cmd);
        self.head = (self.head + 1) % HISTORY_SIZE;
        if self.count < HISTORY_SIZE {
            self.count += 1;
        }
    }

    fn get(&self, n: usize) -> Option<&[u8]> {
        if n >= self.count {
            return None;
        }

        let idx = if self.head >= n + 1 {
            self.head - n - 1
        } else {
            HISTORY_SIZE - (n + 1 - self.head)
        };

        Some(&self.entries[idx][..self.lengths[idx]])
    }
}

// =============================================================================
// TOKENIZER
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenType {
    Word,
    Pipe,           // |
    And,            // &&
    Or,             // ||
    Semicolon,      // ;
    Background,     // &
    RedirectIn,     // <
    RedirectOut,    // >
    RedirectAppend, // >>
    RedirectErr,    // 2>
    HereDoc,        // <<
    Newline,
    LParen,         // (
    RParen,         // )
    LBrace,         // {
    RBrace,         // }
    If,
    Then,
    Else,
    Elif,
    Fi,
    While,
    Until,
    For,
    Do,
    Done,
    Case,
    Esac,
    In,
    Function,
    Return,
    Break,
    Continue,
    Export,
    Readonly,
    Local,
    Unset,
    Eof,
}

struct Token {
    typ: TokenType,
    value: [u8; MAX_LINE],
    value_len: usize,
}

impl Token {
    const fn new() -> Self {
        Self {
            typ: TokenType::Eof,
            value: [0; MAX_LINE],
            value_len: 0,
        }
    }

    fn set_value(&mut self, val: &[u8]) {
        self.value_len = copy_slice(&mut self.value, val);
    }

    fn get_value(&self) -> &[u8] {
        &self.value[..self.value_len]
    }
}

struct Tokenizer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    fn peek_char(&self) -> Option<u8> {
        if self.pos < self.input.len() {
            Some(self.input[self.pos])
        } else {
            None
        }
    }

    fn next_char(&mut self) -> Option<u8> {
        if self.pos < self.input.len() {
            let ch = self.input[self.pos];
            self.pos += 1;
            Some(ch)
        } else {
            None
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == b' ' || ch == b'\t' {
                self.next_char();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        if self.peek_char() == Some(b'#') {
            while let Some(ch) = self.next_char() {
                if ch == b'\n' {
                    break;
                }
            }
        }
    }

    fn next_token(&mut self) -> Token {
        let mut token = Token::new();

        self.skip_whitespace();
        self.skip_comment();

        let ch = match self.peek_char() {
            Some(c) => c,
            None => {
                token.typ = TokenType::Eof;
                return token;
            }
        };

        // Check operators
        match ch {
            b'\n' => {
                self.next_char();
                token.typ = TokenType::Newline;
                return token;
            }
            b'|' => {
                self.next_char();
                if self.peek_char() == Some(b'|') {
                    self.next_char();
                    token.typ = TokenType::Or;
                } else {
                    token.typ = TokenType::Pipe;
                }
                return token;
            }
            b'&' => {
                self.next_char();
                if self.peek_char() == Some(b'&') {
                    self.next_char();
                    token.typ = TokenType::And;
                } else {
                    token.typ = TokenType::Background;
                }
                return token;
            }
            b';' => {
                self.next_char();
                token.typ = TokenType::Semicolon;
                return token;
            }
            b'<' => {
                self.next_char();
                if self.peek_char() == Some(b'<') {
                    self.next_char();
                    token.typ = TokenType::HereDoc;
                } else {
                    token.typ = TokenType::RedirectIn;
                }
                return token;
            }
            b'>' => {
                self.next_char();
                if self.peek_char() == Some(b'>') {
                    self.next_char();
                    token.typ = TokenType::RedirectAppend;
                } else {
                    token.typ = TokenType::RedirectOut;
                }
                return token;
            }
            b'(' => {
                self.next_char();
                token.typ = TokenType::LParen;
                return token;
            }
            b')' => {
                self.next_char();
                token.typ = TokenType::RParen;
                return token;
            }
            b'{' => {
                self.next_char();
                token.typ = TokenType::LBrace;
                return token;
            }
            b'}' => {
                self.next_char();
                token.typ = TokenType::RBrace;
                return token;
            }
            b'2' if self.input.get(self.pos + 1) == Some(&b'>') => {
                self.next_char();
                self.next_char();
                token.typ = TokenType::RedirectErr;
                return token;
            }
            _ => {}
        }

        // Read a word (possibly quoted)
        let mut value_buf = [0u8; MAX_LINE];
        let mut value_len = 0;
        let mut in_single_quote = false;
        let mut in_double_quote = false;

        loop {
            let ch = match self.peek_char() {
                Some(c) => c,
                None => break,
            };

            if in_single_quote {
                if ch == b'\'' {
                    self.next_char();
                    in_single_quote = false;
                } else {
                    value_buf[value_len] = ch;
                    value_len += 1;
                    self.next_char();
                }
            } else if in_double_quote {
                if ch == b'"' {
                    self.next_char();
                    in_double_quote = false;
                } else if ch == b'\\' {
                    self.next_char();
                    if let Some(escaped) = self.next_char() {
                        let c = match escaped {
                            b'n' => b'\n',
                            b't' => b'\t',
                            b'r' => b'\r',
                            b'\\' => b'\\',
                            b'"' => b'"',
                            b'$' => b'$',
                            _ => escaped,
                        };
                        value_buf[value_len] = c;
                        value_len += 1;
                    }
                } else {
                    value_buf[value_len] = ch;
                    value_len += 1;
                    self.next_char();
                }
            } else {
                match ch {
                    b'\'' => {
                        self.next_char();
                        in_single_quote = true;
                    }
                    b'"' => {
                        self.next_char();
                        in_double_quote = true;
                    }
                    b'\\' => {
                        self.next_char();
                        if let Some(escaped) = self.next_char() {
                            value_buf[value_len] = escaped;
                            value_len += 1;
                        }
                    }
                    b' ' | b'\t' | b'\n' | b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')' | b'{' | b'}' => {
                        break;
                    }
                    _ => {
                        value_buf[value_len] = ch;
                        value_len += 1;
                        self.next_char();
                    }
                }
            }

            if value_len >= MAX_LINE - 1 {
                break;
            }
        }

        token.set_value(&value_buf[..value_len]);

        // Check for keywords
        let value = token.get_value();
        token.typ = match value {
            b"if" => TokenType::If,
            b"then" => TokenType::Then,
            b"else" => TokenType::Else,
            b"elif" => TokenType::Elif,
            b"fi" => TokenType::Fi,
            b"while" => TokenType::While,
            b"until" => TokenType::Until,
            b"for" => TokenType::For,
            b"do" => TokenType::Do,
            b"done" => TokenType::Done,
            b"case" => TokenType::Case,
            b"esac" => TokenType::Esac,
            b"in" => TokenType::In,
            b"function" => TokenType::Function,
            b"return" => TokenType::Return,
            b"break" => TokenType::Break,
            b"continue" => TokenType::Continue,
            b"export" => TokenType::Export,
            b"readonly" => TokenType::Readonly,
            b"local" => TokenType::Local,
            b"unset" => TokenType::Unset,
            _ => TokenType::Word,
        };

        token
    }
}

// =============================================================================
// SHELL STATE
// =============================================================================

struct Shell {
    variables: [Variable; MAX_VARS],
    functions: [Function; MAX_FUNCTIONS],
    aliases: [Alias; MAX_ALIAS],
    jobs: [Job; MAX_JOBS],
    history: History,
    last_exit_code: i32,
    next_job_id: i32,
    interactive: bool,
    script_depth: i32,
    loop_depth: i32,
    should_break: bool,
    should_continue: bool,
    should_return: bool,
    return_value: i32,
}

impl Shell {
    const fn new() -> Self {
        const VAR_INIT: Variable = Variable::new();
        const FUNC_INIT: Function = Function::new();
        const ALIAS_INIT: Alias = Alias::new();
        const JOB_INIT: Job = Job::new();

        Self {
            variables: [VAR_INIT; MAX_VARS],
            functions: [FUNC_INIT; MAX_FUNCTIONS],
            aliases: [ALIAS_INIT; MAX_ALIAS],
            jobs: [JOB_INIT; MAX_JOBS],
            history: History::new(),
            last_exit_code: 0,
            next_job_id: 1,
            interactive: true,
            script_depth: 0,
            loop_depth: 0,
            should_break: false,
            should_continue: false,
            should_return: false,
            return_value: 0,
        }
    }

    fn init(&mut self) {
        // Set default variables
        self.set_var(b"SHELL", b"/bin/qsh");
        self.set_var(b"PATH", b"/bin:/usr/bin");
        self.set_var(b"HOME", b"/home");
        self.set_var(b"PS1", b"build> ");
        self.set_var(b"PS2", b"> ");
        self.set_var(b"IFS", b" \t\n");

        // Set PID
        let mut pid_buf = [0u8; 16];
        let pid = getpid();
        let pid_len = format_int(pid as i64, &mut pid_buf);
        self.set_var(b"$", &pid_buf[..pid_len]);

        // Set shell options
        self.set_var(b"HISTSIZE", b"100");
    }

    // Variable management
    fn get_var(&self, name: &[u8]) -> Option<&[u8]> {
        // Special variables
        match name {
            b"?" => {
                // Return last exit code - handled specially
                return None;
            }
            b"$" | b"0" | b"#" | b"*" | b"@" => {
                for var in &self.variables {
                    if !var.is_empty() && str_eq(var.get_name(), name) {
                        return Some(var.get_value());
                    }
                }
                return None;
            }
            _ => {}
        }

        for var in &self.variables {
            if !var.is_empty() && str_eq(var.get_name(), name) {
                return Some(var.get_value());
            }
        }
        None
    }

    fn set_var(&mut self, name: &[u8], value: &[u8]) -> bool {
        // Check if variable exists
        for var in &mut self.variables {
            if !var.is_empty() && str_eq(var.get_name(), name) {
                if var.readonly {
                    return false;
                }
                var.set_value(value);
                return true;
            }
        }

        // Find empty slot
        for var in &mut self.variables {
            if var.is_empty() {
                var.set_name(name);
                var.set_value(value);
                return true;
            }
        }

        false
    }

    fn export_var(&mut self, name: &[u8]) {
        for var in &mut self.variables {
            if !var.is_empty() && str_eq(var.get_name(), name) {
                var.exported = true;
                return;
            }
        }
    }

    fn unset_var(&mut self, name: &[u8]) -> bool {
        for var in &mut self.variables {
            if !var.is_empty() && str_eq(var.get_name(), name) {
                if var.readonly {
                    return false;
                }
                var.name_len = 0;
                var.value_len = 0;
                return true;
            }
        }
        true
    }

    // Function management
    fn get_function(&self, name: &[u8]) -> Option<&[u8]> {
        for func in &self.functions {
            if !func.is_empty() && str_eq(func.get_name(), name) {
                return Some(func.get_body());
            }
        }
        None
    }

    fn set_function(&mut self, name: &[u8], body: &[u8]) -> bool {
        // Check if function exists
        for func in &mut self.functions {
            if !func.is_empty() && str_eq(func.get_name(), name) {
                func.set_body(body);
                return true;
            }
        }

        // Find empty slot
        for func in &mut self.functions {
            if func.is_empty() {
                func.set_name(name);
                func.set_body(body);
                return true;
            }
        }

        false
    }

    fn unset_function(&mut self, name: &[u8]) -> bool {
        for func in &mut self.functions {
            if !func.is_empty() && str_eq(func.get_name(), name) {
                func.name_len = 0;
                func.body_len = 0;
                return true;
            }
        }
        true
    }

    // Alias management
    fn get_alias(&self, name: &[u8]) -> Option<&[u8]> {
        for alias in &self.aliases {
            if !alias.is_empty() && str_eq(alias.get_name(), name) {
                return Some(alias.get_value());
            }
        }
        None
    }

    fn set_alias(&mut self, name: &[u8], value: &[u8]) -> bool {
        // Check if alias exists
        for alias in &mut self.aliases {
            if !alias.is_empty() && str_eq(alias.get_name(), name) {
                alias.set_value(value);
                return true;
            }
        }

        // Find empty slot
        for alias in &mut self.aliases {
            if alias.is_empty() {
                alias.set_name(name);
                alias.set_value(value);
                return true;
            }
        }

        false
    }

    fn unset_alias(&mut self, name: &[u8]) -> bool {
        for alias in &mut self.aliases {
            if !alias.is_empty() && str_eq(alias.get_name(), name) {
                alias.name_len = 0;
                alias.value_len = 0;
                return true;
            }
        }
        true
    }

    // Job control
    fn add_job(&mut self, pid: i32, command: &[u8]) -> i32 {
        for job in &mut self.jobs {
            if !job.is_active() {
                job.pid = pid;
                job.job_id = self.next_job_id;
                job.command_len = copy_slice(&mut job.command, command);
                job.status = JobStatus::Running;
                self.next_job_id += 1;
                return job.job_id;
            }
        }
        -1
    }

    fn update_jobs(&mut self) {
        for job in &mut self.jobs {
            if job.status == JobStatus::Running {
                let mut status: i32 = 0;
                let result = waitpid(job.pid, &mut status, 1); // WNOHANG
                if result > 0 {
                    job.status = JobStatus::Done;
                }
            }
        }
    }

    // Variable expansion
    fn expand_variables(&self, input: &[u8], output: &mut [u8]) -> usize {
        let mut out_len = 0;
        let mut i = 0;

        while i < input.len() && out_len < output.len() - 1 {
            if input[i] == b'$' {
                i += 1;
                if i >= input.len() {
                    output[out_len] = b'$';
                    out_len += 1;
                    continue;
                }

                let (var_name, var_len) = if input[i] == b'{' {
                    // ${VAR} form
                    i += 1;
                    let start = i;
                    while i < input.len() && input[i] != b'}' {
                        i += 1;
                    }
                    let name = &input[start..i];
                    if i < input.len() {
                        i += 1; // skip }
                    }
                    (name, name.len())
                } else if input[i] == b'?' {
                    // $? - last exit code
                    i += 1;
                    let mut buf = [0u8; 16];
                    let len = format_int(self.last_exit_code as i64, &mut buf);
                    for j in 0..len {
                        if out_len < output.len() - 1 {
                            output[out_len] = buf[j];
                            out_len += 1;
                        }
                    }
                    continue;
                } else if is_digit(input[i]) {
                    // $0, $1, etc - positional parameters
                    let ch = input[i];
                    i += 1;
                    (&input[i - 1..i], 1)
                } else if input[i] == b'$' || input[i] == b'#' || input[i] == b'*' || input[i] == b'@' {
                    // Special variables
                    let ch = input[i];
                    i += 1;
                    (&input[i - 1..i], 1)
                } else if is_identifier_char(input[i]) {
                    // $VAR form
                    let start = i;
                    while i < input.len() && is_identifier_char(input[i]) {
                        i += 1;
                    }
                    (&input[start..i], i - start)
                } else {
                    output[out_len] = b'$';
                    out_len += 1;
                    continue;
                };

                // Look up variable
                if let Some(value) = self.get_var(var_name) {
                    for &ch in value {
                        if out_len < output.len() - 1 {
                            output[out_len] = ch;
                            out_len += 1;
                        }
                    }
                }
            } else if input[i] == b'~' && (i == 0 || input[i - 1] == b' ' || input[i - 1] == b':') {
                // Tilde expansion
                i += 1;
                if i >= input.len() || input[i] == b'/' || input[i] == b' ' {
                    // Expand to HOME
                    if let Some(home) = self.get_var(b"HOME") {
                        for &ch in home {
                            if out_len < output.len() - 1 {
                                output[out_len] = ch;
                                out_len += 1;
                            }
                        }
                    }
                } else {
                    output[out_len] = b'~';
                    out_len += 1;
                }
            } else {
                output[out_len] = input[i];
                out_len += 1;
                i += 1;
            }
        }

        out_len
    }
}

fn format_int(mut n: i64, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let negative = n < 0;
    if negative {
        n = -n;
    }

    let mut temp = [0u8; 20];
    let mut i = 0;

    while n > 0 {
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    let mut pos = 0;
    if negative {
        buf[pos] = b'-';
        pos += 1;
    }

    while i > 0 {
        i -= 1;
        buf[pos] = temp[i];
        pos += 1;
    }

    pos
}

// =============================================================================
// COMMAND EXECUTION
// =============================================================================

struct Command<'a> {
    args: [&'a [u8]; MAX_ARGS],
    argc: usize,
    stdin_redirect: Option<&'a [u8]>,
    stdout_redirect: Option<&'a [u8]>,
    stdout_append: bool,
    stderr_redirect: Option<&'a [u8]>,
    background: bool,
}

impl<'a> Command<'a> {
    fn new() -> Self {
        Self {
            args: [&[]; MAX_ARGS],
            argc: 0,
            stdin_redirect: None,
            stdout_redirect: None,
            stdout_append: false,
            stderr_redirect: None,
            background: false,
        }
    }

    fn add_arg(&mut self, arg: &'a [u8]) {
        if self.argc < MAX_ARGS {
            self.args[self.argc] = arg;
            self.argc += 1;
        }
    }

    fn get_args(&self) -> &[&'a [u8]] {
        &self.args[..self.argc]
    }
}

fn execute_command(shell: &mut Shell, cmd: &Command) -> i32 {
    if cmd.argc == 0 {
        return 0;
    }

    let command = cmd.args[0];

    // Check for builtins first
    if let Some(exit_code) = execute_builtin(shell, cmd) {
        return exit_code;
    }

    // Check for functions
    if let Some(body) = shell.get_function(command) {
        let body_copy = {
            let mut buf = [0u8; MAX_FUNC_BODY];
            let len = copy_slice(&mut buf, body);
            (buf, len)
        };

        // Set positional parameters
        let old_args = save_positional_params(shell);
        set_positional_params(shell, cmd);

        // Execute function body
        let result = execute_script(shell, &body_copy.0[..body_copy.1]);

        // Restore positional parameters
        restore_positional_params(shell, old_args);

        return result;
    }

    // External command
    execute_external(shell, cmd)
}

fn execute_builtin(shell: &mut Shell, cmd: &Command) -> Option<i32> {
    let command = cmd.args[0];

    match command {
        b"exit" | b"quit" => {
            let code = if cmd.argc > 1 {
                parse_int(cmd.args[1]).unwrap_or(0) as i32
            } else {
                shell.last_exit_code
            };
            exit(code);
        }
        b"cd" => {
            let path = if cmd.argc > 1 {
                cmd.args[1]
            } else {
                shell.get_var(b"HOME").unwrap_or(b"/home")
            };

            let mut path_buf = [0u8; MAX_PATH];
            let len = copy_slice(&mut path_buf, path);
            path_buf[len] = 0;

            if chdir(&path_buf) < 0 {
                eprint("cd: ");
                eprint_bytes(path);
                eprintln(": No such directory");
                Some(-1)
            } else {
                Some(0)
            }
        }
        b"pwd" => {
            let mut buf = [0u8; MAX_PATH];
            let len = getcwd(&mut buf);
            if len > 0 {
                print_bytes(&buf[..len as usize]);
                println("");
            } else {
                println("/");
            }
            Some(0)
        }
        b"echo" => {
            let mut first = true;
            let mut newline = true;
            let mut start_idx = 1;

            // Check for -n flag
            if cmd.argc > 1 && str_eq(cmd.args[1], b"-n") {
                newline = false;
                start_idx = 2;
            }

            for i in start_idx..cmd.argc {
                if !first {
                    print(" ");
                }
                first = false;

                // Expand variables
                let mut expanded = [0u8; MAX_LINE];
                let len = shell.expand_variables(cmd.args[i], &mut expanded);
                print_bytes(&expanded[..len]);
            }

            if newline {
                println("");
            }
            Some(0)
        }
        b"export" => {
            if cmd.argc == 1 {
                // List exported variables
                for var in &shell.variables {
                    if !var.is_empty() && var.exported {
                        print("export ");
                        print_bytes(var.get_name());
                        print("=");
                        print_bytes(var.get_value());
                        println("");
                    }
                }
            } else {
                for i in 1..cmd.argc {
                    let arg = cmd.args[i];
                    // Check for NAME=VALUE
                    if let Some(eq_pos) = arg.iter().position(|&c| c == b'=') {
                        let name = &arg[..eq_pos];
                        let value = &arg[eq_pos + 1..];
                        shell.set_var(name, value);
                        shell.export_var(name);
                    } else {
                        shell.export_var(arg);
                    }
                }
            }
            Some(0)
        }
        b"unset" => {
            for i in 1..cmd.argc {
                shell.unset_var(cmd.args[i]);
            }
            Some(0)
        }
        b"set" => {
            if cmd.argc == 1 {
                // List all variables
                for var in &shell.variables {
                    if !var.is_empty() {
                        print_bytes(var.get_name());
                        print("=");
                        print_bytes(var.get_value());
                        println("");
                    }
                }
            }
            Some(0)
        }
        b"alias" => {
            if cmd.argc == 1 {
                // List aliases
                for alias in &shell.aliases {
                    if !alias.is_empty() {
                        print("alias ");
                        print_bytes(alias.get_name());
                        print("='");
                        print_bytes(alias.get_value());
                        println("'");
                    }
                }
            } else {
                for i in 1..cmd.argc {
                    let arg = cmd.args[i];
                    if let Some(eq_pos) = arg.iter().position(|&c| c == b'=') {
                        let name = &arg[..eq_pos];
                        let value = &arg[eq_pos + 1..];
                        shell.set_alias(name, value);
                    }
                }
            }
            Some(0)
        }
        b"unalias" => {
            for i in 1..cmd.argc {
                shell.unset_alias(cmd.args[i]);
            }
            Some(0)
        }
        b"source" | b"." => {
            if cmd.argc < 2 {
                eprintln("source: filename argument required");
                return Some(1);
            }
            Some(source_file(shell, cmd.args[1]))
        }
        b"type" => {
            for i in 1..cmd.argc {
                let name = cmd.args[i];
                if is_builtin(name) {
                    print_bytes(name);
                    println(" is a shell builtin");
                } else if shell.get_function(name).is_some() {
                    print_bytes(name);
                    println(" is a shell function");
                } else if shell.get_alias(name).is_some() {
                    print_bytes(name);
                    print(" is aliased to '");
                    print_bytes(shell.get_alias(name).unwrap());
                    println("'");
                } else {
                    print_bytes(name);
                    println(" not found");
                }
            }
            Some(0)
        }
        b"help" => {
            cmd_help();
            Some(0)
        }
        b"version" => {
            cmd_version();
            Some(0)
        }
        b"clear" => {
            print("\x1b[2J\x1b[H");
            Some(0)
        }
        b"true" => Some(0),
        b"false" => Some(1),
        b":" => Some(0), // null command
        b"return" => {
            let code = if cmd.argc > 1 {
                parse_int(cmd.args[1]).unwrap_or(0) as i32
            } else {
                shell.last_exit_code
            };
            shell.should_return = true;
            shell.return_value = code;
            Some(code)
        }
        b"break" => {
            if shell.loop_depth > 0 {
                shell.should_break = true;
                Some(0)
            } else {
                eprintln("break: only meaningful in a loop");
                Some(1)
            }
        }
        b"continue" => {
            if shell.loop_depth > 0 {
                shell.should_continue = true;
                Some(0)
            } else {
                eprintln("continue: only meaningful in a loop");
                Some(1)
            }
        }
        b"read" => {
            if cmd.argc < 2 {
                eprintln("read: variable name required");
                return Some(1);
            }

            let mut buf = [0u8; MAX_LINE];
            let mut len = 0;

            loop {
                let mut ch = [0u8; 1];
                let n = read(0, &mut ch);
                if n <= 0 || ch[0] == b'\n' {
                    break;
                }
                if len < MAX_LINE - 1 {
                    buf[len] = ch[0];
                    len += 1;
                }
            }

            shell.set_var(cmd.args[1], &buf[..len]);
            Some(0)
        }
        b"test" | b"[" => {
            Some(evaluate_test(shell, cmd))
        }
        b"jobs" => {
            shell.update_jobs();
            for job in &shell.jobs {
                if job.is_active() {
                    print("[");
                    print_num(job.job_id as i64);
                    print("] ");
                    match job.status {
                        JobStatus::Running => print("Running    "),
                        JobStatus::Stopped => print("Stopped    "),
                        _ => {}
                    }
                    print_bytes(&job.command[..job.command_len]);
                    if job.status == JobStatus::Running {
                        print(" &");
                    }
                    println("");
                }
            }
            Some(0)
        }
        b"fg" => {
            let job_id = if cmd.argc > 1 {
                parse_int(cmd.args[1]).unwrap_or(1) as i32
            } else {
                1
            };

            for job in &mut shell.jobs {
                if job.job_id == job_id && job.is_active() {
                    print_bytes(&job.command[..job.command_len]);
                    println("");
                    let mut status: i32 = 0;
                    waitpid(job.pid, &mut status, 0);
                    job.status = JobStatus::Done;
                    return Some(status >> 8);
                }
            }
            eprintln("fg: no such job");
            Some(1)
        }
        b"bg" => {
            let job_id = if cmd.argc > 1 {
                parse_int(cmd.args[1]).unwrap_or(1) as i32
            } else {
                1
            };

            for job in &mut shell.jobs {
                if job.job_id == job_id && job.status == JobStatus::Stopped {
                    kill(job.pid, 18); // SIGCONT
                    job.status = JobStatus::Running;
                    print("[");
                    print_num(job.job_id as i64);
                    print("] ");
                    print_bytes(&job.command[..job.command_len]);
                    println(" &");
                    return Some(0);
                }
            }
            eprintln("bg: no such job");
            Some(1)
        }
        b"history" => {
            for i in 0..shell.history.count {
                if let Some(entry) = shell.history.get(shell.history.count - i - 1) {
                    print("  ");
                    print_num((i + 1) as i64);
                    print("  ");
                    print_bytes(entry);
                    println("");
                }
            }
            Some(0)
        }
        b"eval" => {
            if cmd.argc < 2 {
                return Some(0);
            }
            // Concatenate all arguments
            let mut buf = [0u8; MAX_LINE];
            let mut len = 0;
            for i in 1..cmd.argc {
                if i > 1 && len < MAX_LINE - 1 {
                    buf[len] = b' ';
                    len += 1;
                }
                let arg_len = copy_slice(&mut buf[len..], cmd.args[i]);
                len += arg_len;
            }
            Some(execute_line(shell, &buf[..len]))
        }
        b"exec" => {
            if cmd.argc < 2 {
                return Some(0);
            }
            // Build path
            let mut path = [0u8; MAX_PATH];
            let mut path_len = 0;

            // Check if absolute path
            if cmd.args[1].starts_with(b"/") {
                path_len = copy_slice(&mut path, cmd.args[1]);
            } else {
                // Search in PATH
                path_len = copy_slice(&mut path, b"/bin/");
                path_len += copy_slice(&mut path[path_len..], cmd.args[1]);
            }
            path[path_len] = 0;

            execve(&path, 0, 0);
            eprintln("exec: command not found");
            Some(127)
        }
        b"shift" => {
            let n = if cmd.argc > 1 {
                parse_int(cmd.args[1]).unwrap_or(1) as usize
            } else {
                1
            };
            // Shift positional parameters (simplified - would need proper implementation)
            Some(0)
        }
        b"wait" => {
            if cmd.argc > 1 {
                if let Some(pid) = parse_int(cmd.args[1]) {
                    let mut status: i32 = 0;
                    waitpid(pid as i32, &mut status, 0);
                    return Some(status >> 8);
                }
            } else {
                // Wait for all background jobs
                let mut status: i32 = 0;
                while wait(&mut status) > 0 {}
            }
            Some(0)
        }
        b"kill" => {
            if cmd.argc < 2 {
                eprintln("kill: usage: kill [-signal] pid");
                return Some(1);
            }

            let mut sig = 15; // SIGTERM
            let mut pid_idx = 1;

            if cmd.args[1].starts_with(b"-") {
                if let Some(s) = parse_int(&cmd.args[1][1..]) {
                    sig = s as i32;
                }
                pid_idx = 2;
            }

            for i in pid_idx..cmd.argc {
                if let Some(pid) = parse_int(cmd.args[i]) {
                    kill(pid as i32, sig);
                }
            }
            Some(0)
        }
        _ => None,
    }
}

fn is_builtin(name: &[u8]) -> bool {
    matches!(
        name,
        b"exit" | b"quit" | b"cd" | b"pwd" | b"echo" | b"export" |
        b"unset" | b"set" | b"alias" | b"unalias" | b"source" | b"." |
        b"type" | b"help" | b"version" | b"clear" | b"true" | b"false" |
        b":" | b"return" | b"break" | b"continue" | b"read" | b"test" |
        b"[" | b"jobs" | b"fg" | b"bg" | b"history" | b"eval" | b"exec" |
        b"shift" | b"wait" | b"kill"
    )
}

fn execute_external(shell: &mut Shell, cmd: &Command) -> i32 {
    let command = cmd.args[0];

    // Build path to binary
    let mut path = [0u8; MAX_PATH];
    let mut path_len = 0;

    if command.contains(&b'/') {
        // Absolute or relative path
        path_len = copy_slice(&mut path, command);
    } else {
        // Search in PATH
        // For now, just check /bin
        path_len = copy_slice(&mut path, b"/bin/");
        path_len += copy_slice(&mut path[path_len..], command);
    }
    path[path_len] = 0;

    // Fork and exec
    let pid = fork();

    if pid < 0 {
        eprintln("shell: fork failed");
        return -1;
    } else if pid == 0 {
        // Child process

        // Handle redirections
        if let Some(file) = cmd.stdin_redirect {
            let mut file_path = [0u8; MAX_PATH];
            let len = copy_slice(&mut file_path, file);
            file_path[len] = 0;

            let fd = open(&file_path, O_RDONLY, 0);
            if fd >= 0 {
                dup2(fd, 0);
                close(fd);
            }
        }

        if let Some(file) = cmd.stdout_redirect {
            let mut file_path = [0u8; MAX_PATH];
            let len = copy_slice(&mut file_path, file);
            file_path[len] = 0;

            let flags = if cmd.stdout_append {
                O_WRONLY | O_CREAT | O_APPEND
            } else {
                O_WRONLY | O_CREAT | O_TRUNC
            };

            let fd = open(&file_path, flags, 0o644);
            if fd >= 0 {
                dup2(fd, 1);
                close(fd);
            }
        }

        if let Some(file) = cmd.stderr_redirect {
            let mut file_path = [0u8; MAX_PATH];
            let len = copy_slice(&mut file_path, file);
            file_path[len] = 0;

            let fd = open(&file_path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
            if fd >= 0 {
                dup2(fd, 2);
                close(fd);
            }
        }

        // Execute
        let result = execve(&path, 0, 0);
        if result < 0 {
            eprint("shell: ");
            eprint_bytes(command);
            eprintln(": command not found");
        }
        exit(127);
    } else {
        // Parent process
        if cmd.background {
            // Add to jobs list
            let mut cmd_str = [0u8; 256];
            let mut len = 0;
            for (i, arg) in cmd.get_args().iter().enumerate() {
                if i > 0 && len < 255 {
                    cmd_str[len] = b' ';
                    len += 1;
                }
                len += copy_slice(&mut cmd_str[len..], arg);
            }

            let job_id = shell.add_job(pid, &cmd_str[..len]);
            print("[");
            print_num(job_id as i64);
            print("] ");
            print_num(pid as i64);
            println("");
            return 0;
        } else {
            let mut status: i32 = 0;
            wait(&mut status);
            return status >> 8;
        }
    }
}

// =============================================================================
// PIPELINE EXECUTION
// =============================================================================

fn execute_pipeline(shell: &mut Shell, commands: &[Command]) -> i32 {
    if commands.is_empty() {
        return 0;
    }

    if commands.len() == 1 {
        return execute_command(shell, &commands[0]);
    }

    let mut last_status = 0;
    let mut prev_read_fd: i32 = -1;

    for (i, cmd) in commands.iter().enumerate() {
        let is_last = i == commands.len() - 1;

        // Create pipe for all but last command
        let mut pipe_fds = [-1i32; 2];
        if !is_last {
            if pipe(&mut pipe_fds) < 0 {
                eprintln("pipe failed");
                return -1;
            }
        }

        let pid = fork();

        if pid < 0 {
            eprintln("fork failed");
            return -1;
        } else if pid == 0 {
            // Child

            // Connect to previous pipe's read end
            if prev_read_fd >= 0 {
                dup2(prev_read_fd, 0);
                close(prev_read_fd);
            }

            // Connect to current pipe's write end
            if !is_last {
                close(pipe_fds[0]); // Close read end
                dup2(pipe_fds[1], 1);
                close(pipe_fds[1]);
            }

            // Execute
            let exit_code = execute_command(shell, cmd);
            exit(exit_code);
        } else {
            // Parent

            // Close previous pipe's read end
            if prev_read_fd >= 0 {
                close(prev_read_fd);
            }

            // Save current pipe's read end for next iteration
            if !is_last {
                close(pipe_fds[1]); // Close write end
                prev_read_fd = pipe_fds[0];
            }

            // Wait for last command
            if is_last {
                let mut status: i32 = 0;
                waitpid(pid, &mut status, 0);
                last_status = status >> 8;
            }
        }
    }

    last_status
}

// =============================================================================
// CONTROL FLOW
// =============================================================================

fn execute_if(shell: &mut Shell, input: &[u8]) -> (i32, usize) {
    // Parse: if condition; then commands; [elif condition; then commands;]... [else commands;] fi
    let mut pos = 0;
    let mut result = 0;

    // Skip "if"
    while pos < input.len() && !is_whitespace(input[pos]) {
        pos += 1;
    }
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Find "then"
    let condition_start = pos;
    let mut condition_end = pos;
    while condition_end < input.len() {
        if input[condition_end..].starts_with(b"then") {
            break;
        }
        condition_end += 1;
    }

    // Execute condition
    let condition = trim(&input[condition_start..condition_end]);
    let cond_result = execute_line(shell, condition);

    // Skip "then"
    pos = condition_end + 4;
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Find matching fi/else/elif
    let mut depth = 1;
    let mut body_start = pos;
    let mut else_start = 0;
    let mut fi_pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with(b"if ") || input[pos..].starts_with(b"if\t") {
            depth += 1;
        } else if input[pos..].starts_with(b"fi") && (pos + 2 >= input.len() || !is_alnum(input[pos + 2])) {
            depth -= 1;
            if depth == 0 {
                fi_pos = pos;
                break;
            }
        } else if depth == 1 && (input[pos..].starts_with(b"else") || input[pos..].starts_with(b"elif")) {
            if else_start == 0 {
                else_start = pos;
            }
        }
        pos += 1;
    }

    let body_end = if else_start > 0 { else_start } else { fi_pos };
    let body = trim(&input[body_start..body_end]);

    if cond_result == 0 {
        // Condition true - execute then block
        result = execute_script(shell, body);
    } else if else_start > 0 {
        // Condition false - execute else block
        let else_body = trim(&input[else_start + 4..fi_pos]);
        result = execute_script(shell, else_body);
    }

    (result, fi_pos + 2)
}

fn execute_while(shell: &mut Shell, input: &[u8]) -> (i32, usize) {
    let mut pos = 0;
    let mut result = 0;

    // Skip "while"
    while pos < input.len() && !is_whitespace(input[pos]) {
        pos += 1;
    }
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Find "do"
    let condition_start = pos;
    let mut condition_end = pos;
    while condition_end < input.len() {
        if input[condition_end..].starts_with(b"do") && (condition_end + 2 >= input.len() || !is_alnum(input[condition_end + 2])) {
            break;
        }
        condition_end += 1;
    }

    let condition = trim(&input[condition_start..condition_end]);

    // Skip "do"
    pos = condition_end + 2;
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Find "done"
    let body_start = pos;
    let mut depth = 1;
    let mut done_pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with(b"while ") || input[pos..].starts_with(b"for ") || input[pos..].starts_with(b"until ") {
            depth += 1;
        } else if input[pos..].starts_with(b"done") && (pos + 4 >= input.len() || !is_alnum(input[pos + 4])) {
            depth -= 1;
            if depth == 0 {
                done_pos = pos;
                break;
            }
        }
        pos += 1;
    }

    let body = trim(&input[body_start..done_pos]);

    // Execute loop
    shell.loop_depth += 1;
    loop {
        let cond_result = execute_line(shell, condition);
        if cond_result != 0 {
            break;
        }

        result = execute_script(shell, body);

        if shell.should_break {
            shell.should_break = false;
            break;
        }
        if shell.should_continue {
            shell.should_continue = false;
            continue;
        }
        if shell.should_return {
            break;
        }
    }
    shell.loop_depth -= 1;

    (result, done_pos + 4)
}

fn execute_for(shell: &mut Shell, input: &[u8]) -> (i32, usize) {
    let mut pos = 0;
    let mut result = 0;

    // Skip "for"
    while pos < input.len() && !is_whitespace(input[pos]) {
        pos += 1;
    }
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Get variable name
    let var_start = pos;
    while pos < input.len() && is_identifier_char(input[pos]) {
        pos += 1;
    }
    let var_name = &input[var_start..pos];

    // Skip whitespace
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Skip "in"
    if input[pos..].starts_with(b"in") {
        pos += 2;
    }
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Get word list until "do"
    let list_start = pos;
    while pos < input.len() {
        if input[pos..].starts_with(b"do") && (pos + 2 >= input.len() || !is_alnum(input[pos + 2])) {
            break;
        }
        pos += 1;
    }
    let word_list = trim(&input[list_start..pos]);

    // Skip "do"
    pos += 2;
    while pos < input.len() && is_whitespace(input[pos]) {
        pos += 1;
    }

    // Find "done"
    let body_start = pos;
    let mut depth = 1;
    let mut done_pos = 0;

    while pos < input.len() {
        if input[pos..].starts_with(b"while ") || input[pos..].starts_with(b"for ") || input[pos..].starts_with(b"until ") {
            depth += 1;
        } else if input[pos..].starts_with(b"done") && (pos + 4 >= input.len() || !is_alnum(input[pos + 4])) {
            depth -= 1;
            if depth == 0 {
                done_pos = pos;
                break;
            }
        }
        pos += 1;
    }

    let body = trim(&input[body_start..done_pos]);

    // Split word list and iterate
    shell.loop_depth += 1;
    let mut word_pos = 0;
    while word_pos < word_list.len() {
        // Skip whitespace
        while word_pos < word_list.len() && is_whitespace(word_list[word_pos]) {
            word_pos += 1;
        }
        if word_pos >= word_list.len() {
            break;
        }

        // Get next word
        let word_start = word_pos;
        while word_pos < word_list.len() && !is_whitespace(word_list[word_pos]) {
            word_pos += 1;
        }
        let word = &word_list[word_start..word_pos];

        // Set variable
        shell.set_var(var_name, word);

        // Execute body
        result = execute_script(shell, body);

        if shell.should_break {
            shell.should_break = false;
            break;
        }
        if shell.should_continue {
            shell.should_continue = false;
            continue;
        }
        if shell.should_return {
            break;
        }
    }
    shell.loop_depth -= 1;

    (result, done_pos + 4)
}

// =============================================================================
// TEST COMMAND ([ ])
// =============================================================================

fn evaluate_test(shell: &Shell, cmd: &Command) -> i32 {
    if cmd.argc < 2 {
        return 1;
    }

    let mut idx = 1;
    let end_idx = if str_eq(cmd.args[0], b"[") && cmd.argc > 1 && str_eq(cmd.args[cmd.argc - 1], b"]") {
        cmd.argc - 1
    } else {
        cmd.argc
    };

    // Simple test expressions
    if end_idx - idx == 1 {
        // [ string ] - true if string is non-empty
        return if cmd.args[idx].is_empty() { 1 } else { 0 };
    }

    if end_idx - idx == 2 {
        let op = cmd.args[idx];
        let arg = cmd.args[idx + 1];

        match op {
            b"-n" => return if !arg.is_empty() { 0 } else { 1 },
            b"-z" => return if arg.is_empty() { 0 } else { 1 },
            b"-e" | b"-f" | b"-d" => {
                // File existence test (simplified)
                let mut path = [0u8; MAX_PATH];
                let len = copy_slice(&mut path, arg);
                path[len] = 0;
                let fd = open(&path, O_RDONLY, 0);
                if fd >= 0 {
                    close(fd);
                    return 0;
                }
                return 1;
            }
            b"!" => {
                // Negation
                return if arg.is_empty() { 0 } else { 1 };
            }
            _ => {}
        }
    }

    if end_idx - idx == 3 {
        let left = cmd.args[idx];
        let op = cmd.args[idx + 1];
        let right = cmd.args[idx + 2];

        match op {
            b"=" | b"==" => return if str_eq(left, right) { 0 } else { 1 },
            b"!=" => return if !str_eq(left, right) { 0 } else { 1 },
            b"-eq" => {
                if let (Some(l), Some(r)) = (parse_int(left), parse_int(right)) {
                    return if l == r { 0 } else { 1 };
                }
            }
            b"-ne" => {
                if let (Some(l), Some(r)) = (parse_int(left), parse_int(right)) {
                    return if l != r { 0 } else { 1 };
                }
            }
            b"-lt" => {
                if let (Some(l), Some(r)) = (parse_int(left), parse_int(right)) {
                    return if l < r { 0 } else { 1 };
                }
            }
            b"-le" => {
                if let (Some(l), Some(r)) = (parse_int(left), parse_int(right)) {
                    return if l <= r { 0 } else { 1 };
                }
            }
            b"-gt" => {
                if let (Some(l), Some(r)) = (parse_int(left), parse_int(right)) {
                    return if l > r { 0 } else { 1 };
                }
            }
            b"-ge" => {
                if let (Some(l), Some(r)) = (parse_int(left), parse_int(right)) {
                    return if l >= r { 0 } else { 1 };
                }
            }
            _ => {}
        }
    }

    1 // Default: false
}

// =============================================================================
// SCRIPT EXECUTION
// =============================================================================

fn execute_script(shell: &mut Shell, script: &[u8]) -> i32 {
    let mut result = 0;
    let mut pos = 0;

    shell.script_depth += 1;

    while pos < script.len() {
        // Skip whitespace and empty lines
        while pos < script.len() && (is_whitespace(script[pos]) || script[pos] == b';') {
            pos += 1;
        }

        if pos >= script.len() {
            break;
        }

        // Skip comments
        if script[pos] == b'#' {
            while pos < script.len() && script[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }

        // Check for control structures
        if script[pos..].starts_with(b"if ") || script[pos..].starts_with(b"if\t") {
            let (r, consumed) = execute_if(shell, &script[pos..]);
            result = r;
            pos += consumed;
        } else if script[pos..].starts_with(b"while ") || script[pos..].starts_with(b"while\t") {
            let (r, consumed) = execute_while(shell, &script[pos..]);
            result = r;
            pos += consumed;
        } else if script[pos..].starts_with(b"for ") || script[pos..].starts_with(b"for\t") {
            let (r, consumed) = execute_for(shell, &script[pos..]);
            result = r;
            pos += consumed;
        } else {
            // Find end of line/command
            let line_start = pos;
            let mut line_end = pos;

            while line_end < script.len() {
                if script[line_end] == b'\n' || script[line_end] == b';' {
                    break;
                }
                // Handle quoted strings
                if script[line_end] == b'\'' {
                    line_end += 1;
                    while line_end < script.len() && script[line_end] != b'\'' {
                        line_end += 1;
                    }
                } else if script[line_end] == b'"' {
                    line_end += 1;
                    while line_end < script.len() && script[line_end] != b'"' {
                        if script[line_end] == b'\\' {
                            line_end += 1;
                        }
                        line_end += 1;
                    }
                }
                line_end += 1;
            }

            let line = trim(&script[line_start..line_end]);
            if !line.is_empty() {
                result = execute_line(shell, line);
            }

            pos = line_end + 1;
        }

        if shell.should_return || shell.should_break || shell.should_continue {
            break;
        }
    }

    shell.script_depth -= 1;
    shell.last_exit_code = result;
    result
}

fn execute_line(shell: &mut Shell, line: &[u8]) -> i32 {
    let trimmed = trim(line);
    if trimmed.is_empty() {
        return 0;
    }

    // Expand variables first
    let mut expanded = [0u8; MAX_LINE];
    let expanded_len = shell.expand_variables(trimmed, &mut expanded);
    let input = &expanded[..expanded_len];

    // Check for variable assignment
    if let Some(eq_pos) = input.iter().position(|&c| c == b'=') {
        if eq_pos > 0 && input[..eq_pos].iter().all(|&c| is_identifier_char(c)) {
            let name = &input[..eq_pos];
            let value = &input[eq_pos + 1..];
            shell.set_var(name, value);
            return 0;
        }
    }

    // Parse and execute commands
    parse_and_execute(shell, input)
}

fn parse_and_execute(shell: &mut Shell, input: &[u8]) -> i32 {
    let mut tokenizer = Tokenizer::new(input);
    let mut commands: [Command; 16] = [
        Command::new(), Command::new(), Command::new(), Command::new(),
        Command::new(), Command::new(), Command::new(), Command::new(),
        Command::new(), Command::new(), Command::new(), Command::new(),
        Command::new(), Command::new(), Command::new(), Command::new(),
    ];
    let mut cmd_count = 0;
    let mut current_cmd = 0;

    // Temporary storage for token values
    let mut token_values: [[u8; MAX_LINE]; 64] = [[0; MAX_LINE]; 64];
    let mut token_lens: [usize; 64] = [0; 64];
    let mut token_count = 0;

    loop {
        let token = tokenizer.next_token();

        match token.typ {
            TokenType::Word => {
                // Store token value
                if token_count < 64 {
                    token_lens[token_count] = copy_slice(&mut token_values[token_count], token.get_value());
                    let value_ref = &token_values[token_count][..token_lens[token_count]];
                    commands[current_cmd].add_arg(unsafe { core::mem::transmute(value_ref) });
                    token_count += 1;
                }
            }
            TokenType::Pipe => {
                cmd_count = current_cmd + 1;
                current_cmd += 1;
                if current_cmd >= 16 {
                    break;
                }
            }
            TokenType::RedirectIn => {
                let file_token = tokenizer.next_token();
                if token_count < 64 {
                    token_lens[token_count] = copy_slice(&mut token_values[token_count], file_token.get_value());
                    commands[current_cmd].stdin_redirect = Some(unsafe {
                        core::mem::transmute(&token_values[token_count][..token_lens[token_count]] as &[u8])
                    });
                    token_count += 1;
                }
            }
            TokenType::RedirectOut => {
                let file_token = tokenizer.next_token();
                if token_count < 64 {
                    token_lens[token_count] = copy_slice(&mut token_values[token_count], file_token.get_value());
                    commands[current_cmd].stdout_redirect = Some(unsafe {
                        core::mem::transmute(&token_values[token_count][..token_lens[token_count]] as &[u8])
                    });
                    token_count += 1;
                }
            }
            TokenType::RedirectAppend => {
                let file_token = tokenizer.next_token();
                if token_count < 64 {
                    token_lens[token_count] = copy_slice(&mut token_values[token_count], file_token.get_value());
                    commands[current_cmd].stdout_redirect = Some(unsafe {
                        core::mem::transmute(&token_values[token_count][..token_lens[token_count]] as &[u8])
                    });
                    commands[current_cmd].stdout_append = true;
                    token_count += 1;
                }
            }
            TokenType::RedirectErr => {
                let file_token = tokenizer.next_token();
                if token_count < 64 {
                    token_lens[token_count] = copy_slice(&mut token_values[token_count], file_token.get_value());
                    commands[current_cmd].stderr_redirect = Some(unsafe {
                        core::mem::transmute(&token_values[token_count][..token_lens[token_count]] as &[u8])
                    });
                    token_count += 1;
                }
            }
            TokenType::Background => {
                commands[current_cmd].background = true;
            }
            TokenType::And => {
                // Execute left side, if success execute right
                cmd_count = current_cmd + 1;
                let result = execute_pipeline(shell, &commands[..cmd_count]);
                if result != 0 {
                    return result;
                }
                // Reset for right side
                for i in 0..16 {
                    commands[i] = Command::new();
                }
                current_cmd = 0;
                cmd_count = 0;
                token_count = 0;
            }
            TokenType::Or => {
                // Execute left side, if failure execute right
                cmd_count = current_cmd + 1;
                let result = execute_pipeline(shell, &commands[..cmd_count]);
                if result == 0 {
                    return result;
                }
                // Reset for right side
                for i in 0..16 {
                    commands[i] = Command::new();
                }
                current_cmd = 0;
                cmd_count = 0;
                token_count = 0;
            }
            TokenType::Eof | TokenType::Newline | TokenType::Semicolon => {
                break;
            }
            _ => {
                // Handle other tokens as words
                if token_count < 64 {
                    token_lens[token_count] = copy_slice(&mut token_values[token_count], token.get_value());
                    let value_ref = &token_values[token_count][..token_lens[token_count]];
                    commands[current_cmd].add_arg(unsafe { core::mem::transmute(value_ref) });
                    token_count += 1;
                }
            }
        }
    }

    cmd_count = current_cmd + 1;
    if commands[0].argc > 0 {
        let result = execute_pipeline(shell, &commands[..cmd_count]);
        shell.last_exit_code = result;
        return result;
    }

    0
}

fn source_file(shell: &mut Shell, path: &[u8]) -> i32 {
    let mut file_path = [0u8; MAX_PATH];
    let len = copy_slice(&mut file_path, path);
    file_path[len] = 0;

    let fd = open(&file_path, O_RDONLY, 0);
    if fd < 0 {
        eprint("source: ");
        eprint_bytes(path);
        eprintln(": No such file");
        return 1;
    }

    let mut script = [0u8; 65536];
    let mut total_read = 0;

    loop {
        let n = read(fd, &mut script[total_read..]);
        if n <= 0 {
            break;
        }
        total_read += n as usize;
        if total_read >= script.len() - 1 {
            break;
        }
    }

    close(fd);

    execute_script(shell, &script[..total_read])
}

// =============================================================================
// POSITIONAL PARAMETERS
// =============================================================================

fn save_positional_params(_shell: &Shell) -> [[u8; MAX_VAR_VALUE]; 10] {
    [[0; MAX_VAR_VALUE]; 10]
}

fn set_positional_params(shell: &mut Shell, cmd: &Command) {
    // Set $# (argument count)
    let mut buf = [0u8; 16];
    let len = format_int((cmd.argc - 1) as i64, &mut buf);
    shell.set_var(b"#", &buf[..len]);

    // Set $1, $2, etc.
    for i in 1..cmd.argc.min(10) {
        let mut name = [0u8; 2];
        name[0] = b'0' + i as u8;
        shell.set_var(&name[..1], cmd.args[i]);
    }
}

fn restore_positional_params(_shell: &mut Shell, _saved: [[u8; MAX_VAR_VALUE]; 10]) {
    // Restore would go here
}

// =============================================================================
// BUILTIN HELP
// =============================================================================

fn cmd_help() {
    println("");
    println("BuildOS Shell v2.0.0 - Scripting Shell");
    println("");
    println("Built-in Commands:");
    println("  help          - Show this help message");
    println("  version       - Show shell version");
    println("  exit [n]      - Exit shell with status n");
    println("  cd [dir]      - Change directory");
    println("  pwd           - Print working directory");
    println("  echo [-n] ... - Print arguments");
    println("  export [var]  - Export variable to environment");
    println("  unset var     - Remove variable");
    println("  set           - Show all variables");
    println("  alias [n=v]   - Define or show aliases");
    println("  unalias name  - Remove alias");
    println("  source file   - Execute commands from file");
    println("  type name     - Show command type");
    println("  read var      - Read line into variable");
    println("  test expr     - Evaluate conditional expression");
    println("  [ expr ]      - Same as test");
    println("  true          - Return success");
    println("  false         - Return failure");
    println("  return [n]    - Return from function");
    println("  break         - Exit loop");
    println("  continue      - Continue loop");
    println("  jobs          - List background jobs");
    println("  fg [job]      - Bring job to foreground");
    println("  bg [job]      - Resume job in background");
    println("  history       - Show command history");
    println("  eval cmd      - Execute arguments as command");
    println("  exec cmd      - Replace shell with command");
    println("  wait [pid]    - Wait for background jobs");
    println("  kill [-sig] pid - Send signal to process");
    println("  clear         - Clear screen");
    println("");
    println("Control Structures:");
    println("  if cmd; then ...; [elif cmd; then ...;] [else ...;] fi");
    println("  while cmd; do ...; done");
    println("  for var in words; do ...; done");
    println("");
    println("Operators:");
    println("  cmd1 | cmd2   - Pipeline");
    println("  cmd1 && cmd2  - AND (run cmd2 if cmd1 succeeds)");
    println("  cmd1 || cmd2  - OR (run cmd2 if cmd1 fails)");
    println("  cmd &         - Run in background");
    println("  cmd < file    - Redirect stdin");
    println("  cmd > file    - Redirect stdout");
    println("  cmd >> file   - Append stdout");
    println("  cmd 2> file   - Redirect stderr");
    println("");
    println("Variables:");
    println("  var=value     - Set variable");
    println("  $var, ${var}  - Expand variable");
    println("  $?            - Last exit code");
    println("  $$            - Shell PID");
    println("  $#            - Number of arguments");
    println("  $1-$9         - Positional parameters");
    println("");
}

fn cmd_version() {
    println("BuildOS Shell v2.0.0");
    println("Operating System Shell with Scripting Support");
    println("Copyright 2024-2025 Zain Dana Harper");
}

// =============================================================================
// LINE EDITING
// =============================================================================

struct LineEditor {
    buffer: [u8; MAX_LINE],
    len: usize,
    cursor: usize,
    history_pos: usize,
}

impl LineEditor {
    fn new() -> Self {
        Self {
            buffer: [0; MAX_LINE],
            len: 0,
            cursor: 0,
            history_pos: 0,
        }
    }

    fn clear(&mut self) {
        self.buffer = [0; MAX_LINE];
        self.len = 0;
        self.cursor = 0;
    }

    fn insert(&mut self, ch: u8) {
        if self.len >= MAX_LINE - 1 {
            return;
        }

        // Shift characters right
        for i in (self.cursor..self.len).rev() {
            self.buffer[i + 1] = self.buffer[i];
        }

        self.buffer[self.cursor] = ch;
        self.cursor += 1;
        self.len += 1;
    }

    fn delete(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Shift characters left
        for i in self.cursor - 1..self.len - 1 {
            self.buffer[i] = self.buffer[i + 1];
        }

        self.cursor -= 1;
        self.len -= 1;
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.len {
            self.cursor += 1;
        }
    }

    fn move_home(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = self.len;
    }

    fn get_line(&self) -> &[u8] {
        &self.buffer[..self.len]
    }

    fn set_line(&mut self, line: &[u8]) {
        self.len = copy_slice(&mut self.buffer, line);
        self.cursor = self.len;
    }

    fn redraw(&self) {
        // Move cursor to start, clear line, print buffer
        print("\r\x1b[K");
        print("build> ");
        print_bytes(&self.buffer[..self.len]);

        // Move cursor to correct position
        if self.cursor < self.len {
            let back = self.len - self.cursor;
            for _ in 0..back {
                print("\x1b[D");
            }
        }
    }
}

fn read_line(shell: &mut Shell, editor: &mut LineEditor) -> bool {
    editor.clear();
    editor.history_pos = 0;

    // Print prompt
    if let Some(ps1) = shell.get_var(b"PS1") {
        print_bytes(ps1);
    } else {
        print("build> ");
    }

    let mut escape_seq = [0u8; 8];
    let mut escape_len = 0;
    let mut in_escape = false;

    loop {
        let mut ch = [0u8; 1];
        let n = read(0, &mut ch);

        if n <= 0 {
            return false;
        }

        if in_escape {
            escape_seq[escape_len] = ch[0];
            escape_len += 1;

            if escape_len >= 2 {
                // Parse escape sequence
                if escape_seq[0] == b'[' {
                    match escape_seq[1] {
                        b'A' => {
                            // Up arrow - history previous
                            if let Some(entry) = shell.history.get(editor.history_pos) {
                                editor.set_line(entry);
                                editor.history_pos += 1;
                                editor.redraw();
                            }
                        }
                        b'B' => {
                            // Down arrow - history next
                            if editor.history_pos > 0 {
                                editor.history_pos -= 1;
                                if let Some(entry) = shell.history.get(editor.history_pos) {
                                    editor.set_line(entry);
                                } else {
                                    editor.clear();
                                }
                                editor.redraw();
                            }
                        }
                        b'C' => {
                            // Right arrow
                            if editor.cursor < editor.len {
                                editor.move_right();
                                print("\x1b[C");
                            }
                        }
                        b'D' => {
                            // Left arrow
                            if editor.cursor > 0 {
                                editor.move_left();
                                print("\x1b[D");
                            }
                        }
                        b'H' => {
                            // Home
                            editor.move_home();
                            editor.redraw();
                        }
                        b'F' => {
                            // End
                            editor.move_end();
                            editor.redraw();
                        }
                        b'3' if escape_len >= 3 && escape_seq[2] == b'~' => {
                            // Delete key
                            if editor.cursor < editor.len {
                                // Shift characters left
                                for i in editor.cursor..editor.len - 1 {
                                    editor.buffer[i] = editor.buffer[i + 1];
                                }
                                editor.len -= 1;
                                editor.redraw();
                            }
                        }
                        _ => {}
                    }
                }

                in_escape = false;
                escape_len = 0;
            }
            continue;
        }

        match ch[0] {
            b'\n' | b'\r' => {
                println("");
                return true;
            }
            0x1b => {
                // Escape sequence start
                in_escape = true;
                escape_len = 0;
            }
            0x7f | 0x08 => {
                // Backspace
                if editor.cursor > 0 {
                    editor.delete();
                    editor.redraw();
                }
            }
            0x03 => {
                // Ctrl+C
                println("^C");
                editor.clear();
                return true;
            }
            0x04 => {
                // Ctrl+D
                if editor.len == 0 {
                    println("");
                    return false;
                }
            }
            0x01 => {
                // Ctrl+A - home
                editor.move_home();
                editor.redraw();
            }
            0x05 => {
                // Ctrl+E - end
                editor.move_end();
                editor.redraw();
            }
            0x0b => {
                // Ctrl+K - kill to end of line
                editor.len = editor.cursor;
                editor.redraw();
            }
            0x15 => {
                // Ctrl+U - kill whole line
                editor.clear();
                editor.redraw();
            }
            0x17 => {
                // Ctrl+W - kill word
                while editor.cursor > 0 && editor.buffer[editor.cursor - 1] == b' ' {
                    editor.delete();
                }
                while editor.cursor > 0 && editor.buffer[editor.cursor - 1] != b' ' {
                    editor.delete();
                }
                editor.redraw();
            }
            0x0c => {
                // Ctrl+L - clear screen
                print("\x1b[2J\x1b[H");
                if let Some(ps1) = shell.get_var(b"PS1") {
                    print_bytes(ps1);
                } else {
                    print("build> ");
                }
                print_bytes(editor.get_line());
            }
            0x09 => {
                // Tab - completion (simplified)
                // TODO: implement proper completion
            }
            _ => {
                if ch[0] >= 0x20 && ch[0] < 0x7f {
                    editor.insert(ch[0]);
                    // Redraw from cursor
                    print_bytes(&editor.buffer[editor.cursor - 1..editor.len]);
                    // Move cursor back if not at end
                    for _ in editor.cursor..editor.len {
                        print("\x1b[D");
                    }
                }
            }
        }
    }
}

// =============================================================================
// SHELL MAIN
// =============================================================================

#[no_mangle]
pub extern "C" fn _start() -> ! {
    main();
    exit(0);
}

fn main() {
    let mut shell = Shell::new();
    shell.init();

    // Check for script argument
    // (would need command line argument parsing)

    // Clear screen and show banner
    print("\x1b[2J\x1b[H");
    println("");
    println("=================================================");
    println("  BuildOS Shell v2.0.0");
    println("  Type 'help' for available commands");
    println("=================================================");
    println("");

    let mut editor = LineEditor::new();

    // Main shell loop
    loop {
        shell.update_jobs();

        if !read_line(&mut shell, &mut editor) {
            break;
        }

        let line = editor.get_line();
        if line.is_empty() {
            continue;
        }

        // Add to history
        shell.history.add(line);

        // Execute
        let result = execute_line(&mut shell, line);
        shell.last_exit_code = result;
    }

    println("Goodbye!");
}

// =============================================================================
// PANIC HANDLER
// =============================================================================

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    eprintln("");
    eprintln("!!! SHELL PANIC !!!");

    if let Some(location) = info.location() {
        eprint("Location: ");
        eprintln(location.file());
    }

    exit(1);
}
