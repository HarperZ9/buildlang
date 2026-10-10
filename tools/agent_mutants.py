"""Generate the M1 mutation set: small programs in the shapes agents write.

Each program is complete (`fn main() ~ Console`), uses one or two constructs a
model reaches for when it writes Rust-like code, and is varied over element types
and spellings. The property under test is not that each program compiles: it is
that `buildc check` rejects every program the C backend cannot compile.

Usage: python tools/agent_mutants.py tests/agent-mutants
"""
import itertools
import sys
from pathlib import Path

ELEMS = {
    "i32": ("vec![3, 1, 2]", "0", "1"),
    "i64": ("vec![3i64, 1i64, 2i64]", "0i64", "1i64"),
    "f64": ("vec![3.5, 1.5, 2.0]", "0.0", "1.0"),
    "str": ('vec!["c", "a", "b"]', '""', '"x"'),
    "String": ('vec![String::from("c"), String::from("a"), String::from("b")]', 'String::new()',
               'String::from("x")'),
    "usize": ("vec![3usize, 1usize, 2usize]", "0usize", "1usize"),
}

# Collection idioms: {v} is a vector literal, {z} a zero, {one} a unit value.
VEC_IDIOMS = {
    "iter_for": "let v = {v};\n    for x in v.iter() {{ println!(\"{{}}\", x); }}",
    "into_iter_for": "let v = {v};\n    for x in v.into_iter() {{ println!(\"{{}}\", x); }}",
    "plain_for": "let v = {v};\n    for x in v {{ println!(\"{{}}\", x); }}",
    "ref_for": "let v = {v};\n    for x in &v {{ println!(\"{{}}\", x); }}",
    "index_loop": "let v = {v};\n    let mut i = 0;\n    while i < v.len() {{ println!(\"{{}}\", v[i]); i += 1; }}",
    "range_for": "let v = {v};\n    for i in 0..v.len() {{ println!(\"{{}}\", v[i]); }}",
    "len": "let v = {v};\n    println!(\"{{}}\", v.len());",
    "push_pop": "let mut v = {v};\n    v.push({one});\n    let last = v.pop();\n    println!(\"{{}}\", v.len());",
    "first_last": "let v = {v};\n    println!(\"{{}} {{}}\", v[0], v[v.len() - 1]);",
    "sort": "let mut v = {v};\n    v.sort();\n    println!(\"{{}}\", v[0]);",
    "reverse": "let mut v = {v};\n    v.reverse();\n    println!(\"{{}}\", v[0]);",
    "contains": "let v = {v};\n    if v.contains(&{one}) {{ println!(\"yes\"); }}",
    "map_collect": "let v = {v};\n    let w: Vec<_> = v.iter().map(|x| x.clone()).collect();\n    println!(\"{{}}\", w.len());",
    "filter_count": "let v = {v};\n    let n = v.iter().filter(|x| **x != {z}).count();\n    println!(\"{{}}\", n);",
    "enumerate": "let v = {v};\n    for (i, x) in v.iter().enumerate() {{ println!(\"{{}} {{}}\", i, x); }}",
    "slice": "let v = {v};\n    let s = &v[1..3];\n    println!(\"{{}}\", s.len());",
    "get_option": "let v = {v};\n    match v.get(0) {{ Some(x) => println!(\"{{}}\", x), None => println!(\"none\") }}",
    "vec_new_typed": "let mut v: Vec<{t}> = Vec::new();\n    v.push({one});\n    println!(\"{{}}\", v.len());",
    "vec_new_builtin": "let mut v: Vec<{t}> = vec_new();\n    vec_push(v, {one});\n    println!(\"{{}}\", vec_len(v));",
    "vec_new_untyped": "let mut v = vec_new();\n    vec_push(v, {one});\n    println!(\"{{}}\", vec_len(v));",
    "swap": "let mut v = {v};\n    v.swap(0, 1);\n    println!(\"{{}}\", v[0]);",
    "insert_remove": "let mut v = {v};\n    v.insert(0, {one});\n    v.remove(1);\n    println!(\"{{}}\", v.len());",
    "extend": "let mut v = {v};\n    let w = {v};\n    v.extend(w);\n    println!(\"{{}}\", v.len());",
    "join_debug": "let v = {v};\n    println!(\"{{:?}}\", v);",
    "fn_param": "let v = {v};\n    println!(\"{{}}\", count(&v));\n}}\nfn count(v: &Vec<{t}>) -> usize {{\n    v.len()",
    "sum": "let v = {v};\n    let s: {t} = v.iter().sum();\n    println!(\"{{}}\", s);",
}

STRING_IDIOMS = {
    "split_for": 'let s = "a b c";\n    for w in s.split(" ") { println!("{}", w); }',
    "split_whitespace": 'let s = "a b c";\n    for w in s.split_whitespace() { println!("{}", w); }',
    "lines": 'let s = "1\\n2";\n    for l in s.lines() { println!("{}", l); }',
    "chars": 'let s = "abc";\n    for c in s.chars() { println!("{}", c); }',
    "bytes_index": 'let s = "abc";\n    let b = s.as_bytes();\n    println!("{}", b[0]);',
    "trim": 'let s = "  x  ";\n    println!("{}", s.trim());',
    "parse_i32": 'let s = "42";\n    let n: i32 = s.parse().unwrap();\n    println!("{}", n);',
    "parse_turbofish": 'let s = "42";\n    let n = s.parse::<i64>().unwrap();\n    println!("{}", n);',
    "parse_match": 'let s = "42";\n    match s.parse::<i32>() { Ok(n) => println!("{}", n), Err(_) => println!("bad") }',
    "to_uppercase": 'let s = "abc";\n    println!("{}", s.to_uppercase());',
    "contains": 'let s = "hello";\n    if s.contains("ell") { println!("yes"); }',
    "starts_with": 'let s = "hello";\n    if s.starts_with("he") { println!("yes"); }',
    "replace": 'let s = "hello";\n    println!("{}", s.replace("l", "L"));',
    "len": 'let s = "hello";\n    println!("{}", s.len());',
    "is_empty": 'let s = "";\n    if s.is_empty() { println!("empty"); }',
    "push_str": 'let mut s = String::new();\n    s.push_str("ab");\n    println!("{}", s);',
    "push_char": "let mut s = String::new();\n    s.push('a');\n    println!(\"{}\", s);",
    "concat_ref": 'let a = String::from("a");\n    let b = String::from("b");\n    let c = a + &b;\n    println!("{}", c);',
    "format": 'let n = 3;\n    let s = format!("n={}", n);\n    println!("{}", s);',
    "format_width": 'let n = 3.14159;\n    println!("{:.2}", n);',
    "format_pad": 'let n = 7;\n    println!("{:>4}", n);',
    "compare_lt": 'let a = "apple";\n    let b = "banana";\n    if a < b { println!("lt"); }',
    "compare_eq": 'let a = "x";\n    if a == "x" { println!("eq"); }',
    "to_string_int": 'let n = 5;\n    let s = n.to_string();\n    println!("{}", s);',
    "string_from": 'let s = String::from("hi");\n    println!("{}", s);',
    "find": 'let s = "hello";\n    match s.find("l") { Some(i) => println!("{}", i), None => println!("no") }',
    "char_digit": "let c = '7';\n    let d = c as i32 - '0' as i32;\n    println!(\"{}\", d);",
    "index_char": 'let s = "abc";\n    println!("{}", s[0]);',
    "substring": 'let s = "hello";\n    println!("{}", &s[1..3]);',
    "split_collect": 'let s = "a,b";\n    let parts: Vec<&str> = s.split(",").collect();\n    println!("{}", parts.len());',
}

CONTROL_IDIOMS = {
    "option_unwrap": "let x: Option<i32> = Some(3);\n    println!(\"{}\", x.unwrap());",
    "option_unwrap_or": "let x: Option<i32> = None;\n    println!(\"{}\", x.unwrap_or(7));",
    "option_match": "let x: Option<i32> = Some(3);\n    match x { Some(v) => println!(\"{}\", v), None => println!(\"none\") }",
    "if_let": "let x: Option<i32> = Some(3);\n    if let Some(v) = x { println!(\"{}\", v); }",
    "result_match": "let r: Result<i32, String> = Ok(3);\n    match r { Ok(v) => println!(\"{}\", v), Err(e) => println!(\"{}\", e) }",
    "question": "match half(4) { Some(v) => println!(\"{}\", v), None => println!(\"none\") }\n}\nfn half(x: i32) -> Option<i32> {\n    let y = check(x)?;\n    Some(y / 2)\n}\nfn check(x: i32) -> Option<i32> {\n    if x > 0 { Some(x) } else { None }",
    "while_let": "let mut v = vec![1, 2];\n    while let Some(x) = v.pop() { println!(\"{}\", x); }",
    "loop_break_value": "let mut i = 0;\n    let r = loop { i += 1; if i == 3 { break i * 2; } };\n    println!(\"{}\", r);",
    "match_guard": "let n = 5;\n    match n { x if x > 3 => println!(\"big\"), _ => println!(\"small\") }",
    "match_range": "let n = 5;\n    match n { 0..=3 => println!(\"low\"), _ => println!(\"high\") }",
    "tuple": "let t = (1, \"a\");\n    println!(\"{} {}\", t.0, t.1);",
    "tuple_destructure": "let (a, b) = (1, 2);\n    println!(\"{}\", a + b);",
    "array": "let a = [1, 2, 3];\n    println!(\"{}\", a[1]);",
    "array_len": "let a = [1, 2, 3];\n    println!(\"{}\", a.len());",
    "closure": "let add = |a: i32, b: i32| a + b;\n    println!(\"{}\", add(1, 2));",
    "closure_capture": "let k = 3;\n    let f = |x: i32| x * k;\n    println!(\"{}\", f(2));",
    "cast": "let x = 3.7;\n    let n = x as i32;\n    println!(\"{}\", n);",
    "shadow": "let x = 1;\n    let x = x + 1;\n    println!(\"{}\", x);",
    "const": "println!(\"{}\", LIMIT);\n}\nconst LIMIT: i32 = 10;\nfn unused() {",
    "abs_min_max": "let a = -3;\n    println!(\"{} {}\", a.abs(), std::cmp::max(a, 2));",
    "checked_add": "let a: i32 = 2147483647;\n    match a.checked_add(1) { Some(v) => println!(\"{}\", v), None => println!(\"overflow\") }",
    "hashmap": "let mut m = HashMap::new();\n    m.insert(\"a\", 1);\n    println!(\"{}\", m.len());",
    "hashmap_get": "let mut m: HashMap<String, i32> = HashMap::new();\n    m.insert(String::from(\"a\"), 1);\n    match m.get(\"a\") { Some(v) => println!(\"{}\", v), None => println!(\"none\") }",
    "hashmap_entry": "let mut m: HashMap<String, i32> = HashMap::new();\n    *m.entry(String::from(\"a\")).or_insert(0) += 1;\n    println!(\"{}\", m.len());",
    "struct_method": "let p = Point { x: 1, y: 2 };\n    println!(\"{}\", p.sum());\n}\nstruct Point { x: i32, y: i32 }\nimpl Point {\n    fn sum(&self) -> i32 { self.x + self.y }\n}\nfn unused() {",
    "struct_new": "let p = Point::new(1, 2);\n    println!(\"{}\", p.x);\n}\nstruct Point { x: i32, y: i32 }\nimpl Point {\n    fn new(x: i32, y: i32) -> Point { Point { x, y } }\n}\nfn unused() {",
    "enum_data": "let s = Shape::Rect(2, 3);\n    let a = match s { Shape::Square(n) => n * n, Shape::Rect(w, h) => w * h };\n    println!(\"{}\", a);\n}\nenum Shape { Square(i32), Rect(i32, i32) }\nfn unused() {",
    "enum_vec": "let v = vec![Shape::Square(2), Shape::Rect(2, 3)];\n    let mut t = 0;\n    for s in v.iter() { t += area(s); }\n    println!(\"{}\", t);\n}\nenum Shape { Square(i32), Rect(i32, i32) }\nfn area(s: &Shape) -> i32 { match s { Shape::Square(n) => n * n, Shape::Rect(w, h) => w * h } }\nfn unused() {",
    "trait_impl": "let c = Circle { r: 2.0 };\n    println!(\"{}\", c.area());\n}\ntrait Area { fn area(&self) -> f64; }\nstruct Circle { r: f64 }\nimpl Area for Circle { fn area(&self) -> f64 { 3.0 * self.r * self.r } }\nfn unused() {",
    "generic_fn": "println!(\"{}\", largest(3, 7));\n}\nfn largest<T: PartialOrd>(a: T, b: T) -> T { if a > b { a } else { b } }\nfn unused() {",
    "impl_display": "let p = P { x: 1 };\n    println!(\"{}\", p);\n}\nstruct P { x: i32 }\nimpl std::fmt::Display for P {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"P({})\", self.x) }\n}\nfn unused() {",
    "recursion": "println!(\"{}\", fact(5));\n}\nfn fact(n: i64) -> i64 { if n <= 1 { 1 } else { n * fact(n - 1) } }\nfn unused() {",
    "mut_ref_param": "let mut x = 1;\n    bump(&mut x);\n    println!(\"{}\", x);\n}\nfn bump(x: &mut i32) { *x += 1; }\nfn unused() {",
    "box": "let b = Box::new(5);\n    println!(\"{}\", *b);",
    "assert": "let x = 2;\n    assert!(x == 2);\n    assert_eq!(x, 2);\n    println!(\"ok\");",
    "exit": "println!(\"bye\");\n    std::process::exit(0);",
    "read_file_expect": "let s = std::fs::read_to_string(\"input.txt\").expect(\"read\");\n    println!(\"{}\", s.len());",
    "read_file_builtin": "let s = read_file(\"input.txt\");\n    println!(\"{}\", s.len());",
    "env_args": "let args: Vec<String> = std::env::args().collect();\n    println!(\"{}\", args.len());",
}

# Effects an idiom needs beyond Console.
EXTRA_EFFECTS = {
    "read_file_expect": " + FileSystem",
    "read_file_builtin": " + FileSystem",
    "env_args": " + Environment",
    "exit": " + Process",
}

HEADERS = {
    "hashmap": "use std::collections::HashMap;\n",
    "hashmap_get": "use std::collections::HashMap;\n",
    "hashmap_entry": "use std::collections::HashMap;\n",
}


def program(name, body):
    effects = "~ Console" + EXTRA_EFFECTS.get(name, "")
    header = HEADERS.get(name, "")
    return f"{header}fn main() {effects} {{\n    {body}\n}}\n"


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    count = 0
    for (idiom, tmpl), (t, (v, z, one)) in itertools.product(VEC_IDIOMS.items(), ELEMS.items()):
        body = tmpl.format(v=v, z=z, one=one, t=t)
        (out / f"vec_{idiom}_{t}.bld").write_text(program(idiom, body), encoding="utf-8")
        count += 1
    for idiom, body in itertools.chain(STRING_IDIOMS.items(), CONTROL_IDIOMS.items()):
        prefix = "str" if idiom in STRING_IDIOMS else "ctl"
        (out / f"{prefix}_{idiom}.bld").write_text(program(idiom, body), encoding="utf-8")
        count += 1
    print(count)


if __name__ == "__main__":
    main()
