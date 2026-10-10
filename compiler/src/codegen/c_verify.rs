// ===============================================================================
// BUILDLANG CODEGEN - GENERATED C VERIFIER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! A check over the generated C text: every function the user section calls must
//! be declared somewhere in the output or come from the C standard library.
//!
//! The lowerer falls back to a bare C call when it does not recognise a method or
//! function (`v.iter()` becomes `iter(v)`). Without this pass the program passes
//! `buildc check` and then fails inside the C compiler with an "implicit
//! declaration" error about generated code. With it, `check` and `build` reject the
//! program with the BuildLang name of the call and a fix hint.
//!
//! The scan is a small C tokenizer, not a C parser. A name counts as declared when
//! it appears as `<type> name(` (a definition or prototype) or `#define name(`. A
//! name counts as called when it appears as `name(` after the end-of-runtime marker
//! in any other position.

use std::collections::BTreeSet;

/// Marker the C backend writes after the embedded runtime.
const RUNTIME_END_MARKER: &str = "End BuildLang Runtime";

/// C keywords and operators that look like calls (`sizeof(x)`, `if (x)`).
const C_KEYWORDS: &[&str] = &[
    "if",
    "else",
    "while",
    "for",
    "do",
    "switch",
    "case",
    "return",
    "sizeof",
    "goto",
    "break",
    "continue",
    "default",
    "typedef",
    "struct",
    "union",
    "enum",
    "static",
    "const",
    "volatile",
    "extern",
    "inline",
    "register",
    "restrict",
    "_Alignof",
    "_Static_assert",
    "_Generic",
    "alignof",
    "__attribute__",
    "__declspec",
    "defined",
];

/// Functions and function-like macros from the C headers the runtime includes.
const C_STDLIB: &[&str] = &[
    // stdio
    "printf",
    "fprintf",
    "sprintf",
    "snprintf",
    "vprintf",
    "vfprintf",
    "vsnprintf",
    "puts",
    "fputs",
    "fputc",
    "putchar",
    "fflush",
    "fopen",
    "fclose",
    "fread",
    "fwrite",
    "fgets",
    "getchar",
    "fgetc",
    "fseek",
    "ftell",
    "rewind",
    "perror",
    "remove",
    "rename",
    "setvbuf",
    "fileno",
    "_fileno",
    "_setmode",
    "scanf",
    "sscanf",
    "fscanf",
    // stdlib
    "malloc",
    "calloc",
    "realloc",
    "free",
    "exit",
    "abort",
    "atexit",
    "atoi",
    "atol",
    "atoll",
    "atof",
    "strtol",
    "strtoll",
    "strtoul",
    "strtoull",
    "strtod",
    "strtof",
    "qsort",
    "bsearch",
    "rand",
    "srand",
    "getenv",
    "system",
    "labs",
    "llabs",
    "div",
    // string
    "memcpy",
    "memmove",
    "memset",
    "memcmp",
    "memchr",
    "strlen",
    "strcmp",
    "strncmp",
    "strcpy",
    "strncpy",
    "strcat",
    "strncat",
    "strchr",
    "strrchr",
    "strstr",
    "strdup",
    "strtok",
    "strerror",
    // math
    "sqrt",
    "sqrtf",
    "pow",
    "powf",
    "exp",
    "expf",
    "exp2",
    "log",
    "logf",
    "log2",
    "log10",
    "sin",
    "sinf",
    "cos",
    "cosf",
    "tan",
    "tanf",
    "asin",
    "acos",
    "atan",
    "atan2",
    "sinh",
    "cosh",
    "tanh",
    "floor",
    "floorf",
    "ceil",
    "ceilf",
    "round",
    "roundf",
    "trunc",
    "truncf",
    "fabs",
    "fabsf",
    "fmod",
    "fmodf",
    "fmin",
    "fmax",
    "fminf",
    "fmaxf",
    "cbrt",
    "hypot",
    "isnan",
    "isinf",
    "isfinite",
    "copysign",
    "abs",
    "nextafter",
    "ldexp",
    "frexp",
    "modf",
    // setjmp, assert, stdarg, time, ctype
    "setjmp",
    "longjmp",
    "assert",
    "va_start",
    "va_end",
    "va_arg",
    "va_copy",
    "time",
    "clock",
    "isdigit",
    "isalpha",
    "isspace",
    "isalnum",
    "isupper",
    "islower",
    "toupper",
    "tolower",
    // windows and posix headers the runtime includes
    "Sleep",
    "GetTickCount64",
    "QueryPerformanceCounter",
    "QueryPerformanceFrequency",
    "WSAStartup",
    "WSACleanup",
    "socket",
    "connect",
    "send",
    "recv",
    "closesocket",
    "close",
    "getaddrinfo",
    "freeaddrinfo",
    "htons",
    "inet_pton",
    "usleep",
    "opendir",
    "readdir",
    "closedir",
    "stat",
    "_stat",
    "mkdir",
    "_mkdir",
    "isatty",
    "_isatty",
    "write",
    "read",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok<'a> {
    Ident(&'a str),
    Punct(char),
    Directive(&'a str),
}

/// Tokenize C text into identifiers, punctuation and preprocessor directive names,
/// skipping comments, string literals and character literals.
fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line_start = true;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\n' {
            line_start = true;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            let quote = c;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            line_start = false;
            continue;
        }
        if c == b'#' && line_start {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            let start = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            out.push(Tok::Directive(&src[start..j]));
            i = j;
            line_start = false;
            continue;
        }
        line_start = false;
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(&src[start..i]));
            continue;
        }
        if c.is_ascii_digit() {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.') {
                i += 1;
            }
            continue;
        }
        out.push(Tok::Punct(c as char));
        i += 1;
    }
    out
}

fn is_keyword(name: &str) -> bool {
    C_KEYWORDS.contains(&name)
}

/// Names declared in `src` as functions, prototypes or function-like macros.
fn declared_names(toks: &[Tok<'_>]) -> BTreeSet<String> {
    let mut declared = BTreeSet::new();
    for (k, tok) in toks.iter().enumerate() {
        let Tok::Ident(name) = tok else { continue };
        if !matches!(toks.get(k + 1), Some(Tok::Punct('('))) {
            continue;
        }
        let prev = if k == 0 { None } else { toks.get(k - 1) };
        let is_decl = match prev {
            Some(Tok::Ident(p)) => !is_keyword(p) || matches!(*p, "struct" | "static" | "const"),
            Some(Tok::Punct('*')) => {
                // `void* name(` is a declaration; `x = *name(` is a call.
                matches!(toks.get(k.wrapping_sub(2)), Some(Tok::Ident(t)) if !is_keyword(t))
            }
            Some(Tok::Directive("define")) => true,
            _ => false,
        };
        if is_decl {
            declared.insert((*name).to_string());
        }
    }
    // Also count `#define NAME` object-like macros and typedef'd function pointers.
    for (k, tok) in toks.iter().enumerate() {
        if let Tok::Directive("define") = tok {
            if let Some(Tok::Ident(name)) = toks.get(k + 1) {
                declared.insert((*name).to_string());
            }
        }
    }
    declared
}

/// Prefixes of names a macro builds by token pasting (`bl_iadd_##SUFFIX(`). A
/// called name that starts with one of these is generated by a macro expansion.
fn pasted_prefixes(toks: &[Tok<'_>]) -> Vec<String> {
    let mut prefixes = Vec::new();
    for w in toks.windows(4) {
        if let [Tok::Ident(p), Tok::Punct('#'), Tok::Punct('#'), Tok::Ident(_)] = w {
            prefixes.push((*p).to_string());
        }
    }
    prefixes
}

/// Names called from the user section of the generated C.
fn called_names(toks: &[Tok<'_>]) -> BTreeSet<String> {
    let mut called = BTreeSet::new();
    for (k, tok) in toks.iter().enumerate() {
        let Tok::Ident(name) = tok else { continue };
        if is_keyword(name) || !matches!(toks.get(k + 1), Some(Tok::Punct('('))) {
            continue;
        }
        // `int32_t (*)(...)` and `T (*name)(...)` are function-pointer types.
        if matches!(toks.get(k + 2), Some(Tok::Punct('*'))) {
            continue;
        }
        let prev = if k == 0 { None } else { toks.get(k - 1) };
        let looks_like_decl = match prev {
            Some(Tok::Ident(p)) => !is_keyword(p),
            Some(Tok::Directive(_)) => true,
            Some(Tok::Punct('*')) => {
                matches!(toks.get(k.wrapping_sub(2)), Some(Tok::Ident(t)) if !is_keyword(t))
            }
            _ => false,
        };
        // A member call through a struct field (`v.fn(` or `p->fn(`) is a
        // function pointer, not a free function.
        let is_member = matches!(prev, Some(Tok::Punct('.')))
            || (matches!(prev, Some(Tok::Punct('>')))
                && matches!(toks.get(k.wrapping_sub(2)), Some(Tok::Punct('-'))));
        if !looks_like_decl && !is_member {
            called.insert((*name).to_string());
        }
    }
    called
}

/// Names of local variables and parameters that hold function pointers are called
/// like functions. Collect identifiers declared as variables (`T name;`,
/// `T name =`, `T (*name)(`), so a call through them is not reported.
fn variable_names(toks: &[Tok<'_>]) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    for (k, tok) in toks.iter().enumerate() {
        if let Tok::Ident(name) = tok {
            let prev_is_type = matches!(k.checked_sub(1).and_then(|p| toks.get(p)),
                Some(Tok::Ident(p)) if !is_keyword(p));
            let next = toks.get(k + 1);
            if prev_is_type && matches!(next, Some(Tok::Punct(';' | '=' | ',' | ')' | '['))) {
                vars.insert((*name).to_string());
            }
            // `(*name)` function pointer declarator.
            if matches!(
                k.checked_sub(1).and_then(|p| toks.get(p)),
                Some(Tok::Punct('*'))
            ) && matches!(
                k.checked_sub(2).and_then(|p| toks.get(p)),
                Some(Tok::Punct('('))
            ) && matches!(next, Some(Tok::Punct(')')))
            {
                vars.insert((*name).to_string());
            }
        }
    }
    vars
}

/// Return the names the user section of `c_source` calls without any declaration.
pub fn undeclared_calls(c_source: &str) -> Vec<String> {
    let user_start = c_source
        .find(RUNTIME_END_MARKER)
        .map(|pos| c_source[pos..].find('\n').map_or(pos, |nl| pos + nl))
        .unwrap_or(0);
    let all_toks = tokenize(c_source);
    let user_toks = tokenize(&c_source[user_start..]);
    let declared = declared_names(&all_toks);
    let vars = variable_names(&all_toks);
    let prefixes = pasted_prefixes(&all_toks);
    called_names(&user_toks)
        .into_iter()
        .filter(|n| {
            !declared.contains(n)
                && !vars.contains(n)
                && !C_STDLIB.contains(&n.as_str())
                && !prefixes.iter().any(|p| n.starts_with(p.as_str()))
        })
        .collect()
}

/// A fix hint for a call the C backend cannot compile, keyed by the BuildLang name.
pub fn hint_for(name: &str) -> String {
    match name {
        "iter" | "into_iter" | "iter_mut" => "iterators are not supported by the C backend yet; \
             loop with an index: `let mut i = 0; while i < v.len() { let x = v[i]; ... i += 1; }`"
            .to_string(),
        "parse" | "unwrap" | "expect" => "string parsing is not supported by the C backend yet; \
             convert digits by hand (`let d = c as i64 - 48;`) and keep the value in a plain integer"
            .to_string(),
        "split" | "lines" | "split_whitespace" | "chars" | "trim" | "bytes" => format!(
            "`{name}` is not supported by the C backend yet; walk the string by index with \
             `s.len()` and `s[i]`"
        ),
        "sort" | "sort_by" | "reverse" | "dedup" => format!(
            "`{name}` is not supported on vectors by the C backend yet; write the loop \
             (for example an insertion sort over indices)"
        ),
        _ => format!(
            "no function or method named `{name}` exists for the C backend; check the \
             spelling, define it as a `fn`, or declare it in an `extern \"C\"` block"
        ),
    }
}

/// Byte range of the first call to `name` in `source`, as a method (`.name(`) or a
/// plain call (`name(`), skipping `fn name(` definitions.
pub fn locate_call(source: &str, name: &str) -> Option<(u32, u32)> {
    let bytes = source.as_bytes();
    let mut from = 0;
    while let Some(rel) = source[from..].find(name) {
        let start = from + rel;
        let end = start + name.len();
        from = end;
        let before_ok = start == 0 || {
            let b = bytes[start - 1];
            !(b.is_ascii_alphanumeric() || b == b'_')
        };
        let after = source[end..].trim_start();
        let after_ok = after.starts_with('(') || after.starts_with("::<");
        let is_def = source[..start].trim_end().ends_with("fn");
        let in_comment = source[..start]
            .rfind('\n')
            .map_or(&source[..start], |nl| &source[nl..start])
            .contains("//");
        if before_ok && after_ok && !is_def && !in_comment {
            return Some((start as u32, end as u32));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_call_with_no_declaration() {
        let c = "static int helper(int x) { return x; }\n// End BuildLang Runtime\n\
                 int32_t main(void) { int32_t a = helper(1); _3 = iter(v); return 0; }";
        assert_eq!(undeclared_calls(c), vec!["iter".to_string()]);
    }

    #[test]
    fn accepts_runtime_stdlib_macros_and_function_pointers() {
        let c = "#define BL_CHECK(x) (x)\nstatic void* build_alloc(size_t n);\n\
                 // End BuildLang Runtime\nint32_t f(int32_t (*cb)(int32_t)) { return cb(1); }\n\
                 int32_t main(void) { printf(\"%d\", 1); void* p = build_alloc(4); \
                 if (BL_CHECK(1)) { return f(0); } return (int32_t)sqrt(4.0); }";
        assert!(undeclared_calls(c).is_empty(), "{:?}", undeclared_calls(c));
    }

    #[test]
    fn accepts_names_built_by_token_pasting() {
        let c = "#define DEF(S, T) static T bl_iadd_##S(T a, T b) { return a + b; }
                 DEF(i32, int32_t)
// End BuildLang Runtime
                 int32_t main(void) { return bl_iadd_i32(1, 2); }";
        assert!(undeclared_calls(c).is_empty(), "{:?}", undeclared_calls(c));
    }

    #[test]
    fn ignores_calls_inside_strings_and_comments() {
        let c = "// End BuildLang Runtime\nint32_t main(void) { /* iter(x) */ \
                 printf(\"parse(x)\"); return 0; }";
        assert!(undeclared_calls(c).is_empty());
    }

    #[test]
    fn locates_method_calls_and_skips_definitions() {
        let src = "fn iter() {}\nfn main() { // v.iter()\n  for x in v.iter() {} }";
        let (start, _) = locate_call(src, "iter").unwrap();
        assert_eq!(&src[start as usize..start as usize + 4], "iter");
        assert!(src[..start as usize].ends_with("v."));
    }
}
