"""Adds `switch` and numeric `#define` to the port tool. Run from the repo root."""
import io


def rw(path, edits):
    s = io.open(path, newline="").read()
    nl = "\r\n" if "\r\n" in s else "\n"
    for old, new in edits:
        old = old.replace("\n", nl)
        new = new.replace("\n", nl)
        assert s.count(old) == 1, (path, old[:60], s.count(old))
        s = s.replace(old, new)
    io.open(path, "w", newline="").write(s)


SWITCH = r'''    if w == "switch" { c_switch(t, st, sy, f, out, steps); return; }
    if w == "goto" || w == "struct" || w == "typedef" || w == "union" || w == "enum" || w == "sizeof" {
'''

SWITCH_FN = r'''// `switch` becomes a `loop` that runs once, so `break` keeps its meaning, with one `if` for
// each group of labels. Every group has to end in `break` or `return`: a case that falls through
// into the next is refused, and so is `continue` inside a switch (it would restart the `loop`).
pub fn c_switch(t: Toks, st: Vec<i32>, sy: Syms, f: Fns, out: Vec<String>, steps: Vec<String>) ~ Console {
    advance(st); expect(t, st, String::from("("));
    let e: String = expr(t, st, sy, f, 0);
    let et: i32 = st[1];
    expect(t, st, String::from(")"));
    if !(et == 1 || et == 2 || et == 5 || et == 6) { refuse(t, st, String::from("a switch on a value that is not an integer")); }
    expect(t, st, String::from("{"));
    st[9] = st[9] + 1;
    let v: String = "sw_" + st[9].to_string();
    emit(out, st, String::from("{"));
    st[3] = st[3] + 1;
    emit(out, st, "let " + v + ": " + tname(et) + " = " + e + ";");
    emit(out, st, String::from("loop {"));
    push_step(steps, st, String::from("!"));
    st[3] = st[3] + 1;
    let mut seen_default: bool = false;
    while cur(t, st) != "}" && cur_kind(t, st) != 0 {
        if !(cur(t, st) == "case" || cur(t, st) == "default") {
            refuse(t, st, String::from("a statement in a switch before its first case"));
            skip_construct(t, st);
            continue;
        }
        let mut cond: String = String::new();
        let mut is_default: bool = false;
        while cur(t, st) == "case" || cur(t, st) == "default" {
            if seen_default { refuse(t, st, String::from("a case after default")); }
            if cur(t, st) == "default" {
                advance(st);
                is_default = true;
            } else {
                advance(st);
                let c: String = expr(t, st, sy, f, 0);
                if cond.len() > 0 { cond = cond + " || "; }
                cond = cond + v + " == " + conv(c, st[1], et);
            }
            expect(t, st, String::from(":"));
        }
        if is_default { seen_default = true; emit(out, st, String::from("{")); } else { emit(out, st, "if " + cond + " {"); }
        st[3] = st[3] + 1;
        let mut ends: bool = false;
        while cur(t, st) != "case" && cur(t, st) != "default" && cur(t, st) != "}" && cur_kind(t, st) != 0 {
            ends = cur(t, st) == "break" || cur(t, st) == "return";
            statement(t, st, sy, f, out, steps);
        }
        st[3] = st[3] - 1;
        emit(out, st, String::from("}"));
        if !ends && cur(t, st) != "}" { refuse(t, st, String::from("a case that falls through into the next")); }
    }
    advance(st);
    emit(out, st, String::from("break;"));
    st[3] = st[3] - 1;
    st[7] = st[7] - 1;
    emit(out, st, String::from("}"));
    st[3] = st[3] - 1;
    emit(out, st, String::from("}"));
}

pub fn statement('''

rw(
    "port/stmts.bld",
    [
        (
            '''    if w == "switch" || w == "goto" || w == "struct" || w == "typedef" || w == "union" || w == "enum" || w == "sizeof" {
''',
            SWITCH,
        ),
        ("pub fn statement(", SWITCH_FN),
        (
            '''            if s == "?" { refuse(t, st, String::from("continue inside do-while")); }
''',
            '''            if s == "?" { refuse(t, st, String::from("continue inside do-while")); }
            else if s == "!" { refuse(t, st, String::from("continue inside switch")); }
''',
        ),
    ],
)

DEFINE_FN = r'''// `#define NAME 10` becomes a constant. The value has to be one number, with an optional minus
// and optional parentheses; a macro with parameters or any other text is refused. A `#define`
// with no value (an include guard) carries nothing and is dropped.
fn c_define(t: Toks, st: Vec<i32>, sy: Syms, out: Vec<String>, w: String) ~ Console {
    let d: Toks = Toks { kind: Vec::new(), text: Vec::new(), line: Vec::new() };
    lex(w.substring(7, w.len() - 7), d.kind, d.text, d.line);
    if d.kind.len() < 2 || d.kind[0] != 1 { refuse(t, st, String::from("a #define without a name")); return; }
    let name: String = d.text[0];
    let after: usize = 7 + w.substring(7, w.len() - 7).len() - w.substring(7, w.len() - 7).trim().len() + name.len();
    if after < w.len() && w.substring(after, 1) == "(" { refuse(t, st, String::from("a macro with parameters")); return; }
    let mut a: usize = 1;
    let mut b: usize = d.kind.len() - 1;
    if a == b { return; }
    if b - a >= 2 && d.text[a] == "(" && d.text[b - 1] == ")" { a = a + 1; b = b - 1; }
    let mut sign: String = String::new();
    if b - a == 2 && d.text[a] == "-" { sign = String::from("-"); a = a + 1; }
    let k: i32 = d.kind[a];
    let mut ty: i32 = 0;
    if k == 2 { ty = 1; }
    if k == 8 { ty = 2; }
    if k == 9 { ty = 6; }
    if k == 10 { ty = 4; }
    if k == 3 { ty = 3; }
    if b - a != 1 || ty == 0 || (ty == 6 && sign.len() > 0) { refuse(t, st, String::from("a #define whose value is not one number")); return; }
    emit(out, st, "const " + name + ": " + tname(ty) + " = " + sign + d.text[a] + ";");
    add_sym(sy, st, name, ty);
}

pub fn translate('''

rw(
    "port/file.bld",
    [
        ("pub fn translate(", DEFINE_FN),
        (
            '''            if w.starts_with("#include") { advance(st); continue; }
''',
            '''            if w.starts_with("#include") { advance(st); continue; }
            if w.starts_with("#define") { c_define(t, st, sy, out, w); advance(st); continue; }
''',
        ),
        (
            "    let st: Vec<i32> = vec![0, 0, 0, 0, 0, 0, 0, 0, 0];\n    let sy: Syms",
            "    let st: Vec<i32> = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0];\n    let sy: Syms",
        ),
        ("use stmts::*;\n", "use stmts::*;\nuse lex::*;\n"),
    ],
)
print("applied")
