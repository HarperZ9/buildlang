# BuildLang Language Specification

Version 1.2 - August 2026

## 1. Lexical Structure

### 1.1 Source Encoding
Source files are UTF-8 encoded. Identifiers may contain Unicode characters following UAX #31 (`XID_Start` followed by `XID_Continue*`).

### 1.2 Whitespace and Comments
Whitespace characters (space, tab, newline, carriage return) separate tokens but are otherwise insignificant.

```build
// Line comment (to end of line)
/* Block comment (may nest) */
/// Outer doc comment
//! Inner doc comment
```

Block comments nest: `/* /* inner */ outer */` is valid.

### 1.3 Keywords
```text
fn let mut const static struct enum trait impl
if else match for while loop break continue return
true false self Self super
use mod pub
async await move unsafe extern
type where as in
effect handle perform resume
```

### 1.4 Literals

**Integer literals:**
```build lex-only
42          // decimal (inferred integer type)
42i32       // explicitly i32
0xFF        // hexadecimal
0o77        // octal
0b1010      // binary
1_000_000   // underscores for readability
```

Integer types: `i8`, `i16`, `i32`, `i64`, `i128`, `isize`, `u8`, `u16`, `u32`, `u64`, `u128`, `usize`.

Unsuffixed integer literals have type `{integer}` - a type variable constrained to resolve to a concrete integer type through inference.

**Float literals:**
```build lex-only
3.14        // inferred float type
3.14f32     // explicitly f32
2.5e10      // scientific notation
```

Float types: `f32`, `f64`. Unsuffixed float literals have type `{float}`.

**String literals:**
```build lex-only
"hello"         // str (owned string)
b"bytes"        // &'static [u8; 5]
'c'             // char
b'A'            // u8
r#"raw string"# // raw string (no escape processing)
```

**Boolean literals:** `true`, `false`.

### 1.5 Operators and Delimiters

**Arithmetic:** `+`, `-`, `*`, `/`, `%`
**Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`
**Logical:** `&&`, `||`, `!`
**Bitwise:** `&`, `|`, `^`, `~`, `<<`, `>>`
**Assignment:** `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`
**Reference:** `&`, `&mut`, `*` (deref)
**Other:** `->`, `=>`, `..`, `..=`, `?`, `|>` (pipe)

**Delimiters:** `(`, `)`, `[`, `]`, `{`, `}`, `,`, `;`, `:`, `.`

### 1.6 Operator Precedence

From lowest to highest binding power:

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `=`, `+=`, `-=`, etc. | Right |
| 2 | `..`, `..=` | None |
| 3 | `\|\|` | Left |
| 4 | `&&` | Left |
| 5 | `==`, `!=`, `<`, `>`, `<=`, `>=` | Left |
| 6 | `\|` | Left |
| 7 | `^` | Left |
| 8 | `&` | Left |
| 9 | `<<`, `>>` | Left |
| 10 | `\|>` (pipe) | Left |
| 11 | `+`, `-` | Left |
| 12 | `*`, `/`, `%` | Left |
| 13 | `as` (cast) | Left |
| 14 | `-` (unary), `!`, `~`, `&`, `&mut`, `*` | Prefix |
| 15 | `.`, `()`, `[]`, `?` | Postfix |

## 2. Type System

### 2.1 Primitive Types
- `bool` - boolean (true/false)
- `char` - Unicode scalar value
- `i8`, `i16`, `i32`, `i64`, `i128`, `isize` - signed integers
- `u8`, `u16`, `u32`, `u64`, `u128`, `usize` - unsigned integers
- `f32`, `f64` - IEEE 754 floating point
- `str` - owned string
- `()` - unit type (zero-size)
- `!` - never type (no values)

### 2.2 Compound Types
- `(T, U, V)` - tuple
- `[T; N]` - fixed-size array
- `[T]` - slice
- `&T` - shared reference
- `&mut T` - mutable reference
- `*const T`, `*mut T` - raw pointers
- `fn(T) -> U` - function pointer

### 2.3 User-Defined Types

**Structs:**
```build
struct Point { x: f64, y: f64 }        // named fields
struct Wrapper(i32);                     // tuple struct
struct Unit;                             // unit struct
```

**Enums:**
```build name=color
enum Shape { Circle(f64), Square(f64) }
enum Color { Red, Green, Blue }
```

**Type aliases:**
```build
struct ParseError { line: i32 }
type ParseResult<T> = Result<T, ParseError>;
```

### 2.4 Generics
```build
fn identity<T>(x: T) -> T { x }
struct Pair<A, B> { first: A, second: B }
```

Generic type parameters are monomorphized at each call site. Each distinct instantiation generates a separate function.

### 2.5 Trait Bounds
```build
trait Describe {
    fn describe(&self) -> str;
}

fn print_it<T: Describe>(x: T) ~ Console {
    println!("{}", x.describe());
}

fn first_of<T>(x: T, y: T) -> T where T: Describe {
    x
}
```

### 2.6 Type Inference

BuildLang uses **bidirectional type inference**:
- **Synthesis** (bottom-up): the type of an expression is computed from its subexpressions
- **Checking** (top-down): an expected type flows down to constrain subexpressions

Unsuffixed integer literals resolve through inference: in `let x: u64 = 42`, the `42` is inferred as `u64`. Without context, integer literals default to `i32` and float literals to `f64`.

### 2.7 Type Annotations

Types may carry annotations for domain-specific checking, written `T with Category<Value>`:
```build name=rgb
struct RGB { r: f64, g: f64, b: f64 }
type LinearRGB = RGB with ColorSpace<Linear>;

fn shade(c: RGB with ColorSpace<Linear>) -> f64 {
    c.r
}
```

The unifier checks annotation compatibility: if both types carry annotations in the same category, they must match. An unannotated value is compatible with any annotation. Passing an sRGB value where a linear one is required is rejected:
```build uses=rgb fail=BL0301
fn main() ~ Console {
    let c: RGB with ColorSpace<sRGB> = RGB { r: 0.5, g: 0.5, b: 0.5 };
    println!("{}", shade(c));
}
```

## 3. Ownership and Borrowing

### 3.1 References
```build in-main
let mut x: i32 = 5;
let r: &i32 = &x;           // shared reference
let v: i32 = *r;            // dereference
let mr: &mut i32 = &mut x;  // mutable reference
*mr = v + 1;
```

### 3.2 Borrowing Rules
1. A value may have **either** one mutable reference **or** any number of shared references at any point in time (no aliasing).
2. A mutable reference cannot be created while any other reference (shared or mutable) to the same value is active.
3. References cannot outlive the value they refer to (no returning references to local variables).

### 3.3 Non-Lexical Lifetimes
Borrows expire at last use, not at scope end:
```build in-main
let mut x: i32 = 5;
let r = &x;
println!("{}", *r);  // last use of r
let mr = &mut x;     // OK: r's borrow has expired
*mr = 6;
```

### 3.4 Scope-Based Expiry
Borrows created in an inner scope expire when that scope ends:
```build in-main
let mut x: i32 = 5;
{
    let r = &mut x;
    *r = 6;
} // r's borrow expires here
let r2 = &x; // OK
println!("{}", *r2);
```

## 4. Items

### 4.1 Functions
```build
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```
The last expression in a function body is the implicit return value.

### 4.2 Structs
```build
struct Counter {
    count: i32,
}

impl Counter {
    fn get(&self) -> i32 { self.count }
    fn bump(&mut self) { self.count = self.count + 1; }
}
```

### 4.3 Enums
```build
enum Shape {
    Empty,
    Circle(f64),
    Rect { w: f64, h: f64 },
}
```

Match expressions over enum types must be exhaustive:
```build uses=color
fn name(color: Color) -> str {
    match color {
        Color::Red => "red",
        Color::Green => "green",
        Color::Blue => "blue",   // all variants covered
    }
}
```
Missing variants produce a compile-time error.

### 4.4 Traits
```build
trait Area {
    fn area(&self) -> f64;
}

struct Square { side: f64 }

impl Area for Square {
    fn area(&self) -> f64 { self.side * self.side }
}
```

### 4.5 Modules
```build parse-only
mod name {          // inline module
    pub fn foo() {}
}

mod name;           // external module (loads name.bld from disk)

use module::item;   // import single item
use module::*;      // glob import
use module::{a, b}; // nested import
```

## 5. Expressions

### 5.1 Control Flow
```build in-main
let n = 7;
if n > 5 { println!("big"); } else { println!("small"); }
let found: Option<i32> = Some(n);
if let Some(v) = found { println!("{}", v); }
let kind = match n { 0 => "zero", _ => "other" };
let mut i = 0;
while i < 3 { i = i + 1; }
for k in 0..3 { if k == 1 { continue; } }
let first_big = loop { i = i + 1; if i > 10 { break i; } };
println!("{} {}", kind, first_big);
```

### 5.2 Closures
```build in-main
let f = |x: i32, y: i32| x + y;
let g = |x: i32| -> i32 { x * 2 };
let base = 10;
let h = move |x: i32| x + base;
println!("{} {} {}", f(1, 2), g(3), h(4));
```

### 5.3 Pattern Matching
```build
struct Point { x: i32, y: i32 }

fn classify(value: i32) -> str {
    match value {
        42 => "the answer",     // literal
        1 | 2 => "small",       // or-pattern
        x if x > 100 => "big",  // guard
        _ => "other",           // wildcard
    }
}

fn sum_pair(pair: (i32, i32)) -> i32 {
    match pair {
        (a, b) => a + b,        // tuple
    }
}

fn on_axis(p: Point) -> bool {
    match p {
        Point { x, y } => x == 0 || y == 0,  // struct
    }
}

fn unwrap_or_zero(o: Option<i32>) -> i32 {
    match o {
        Some(v) => v,           // enum variant
        None => 0,
    }
}
```

### 5.4 Integer Arithmetic

Plain fixed-width integer `+`, `-`, and `*` are checked operations. For signed
and unsigned integer types (`i8` through `i128`, `isize`, `u8` through `u128`,
and `usize`), a result outside the destination type's range is an integer
overflow. This includes unsigned underflow in subtraction.

Compound assignment uses the same checked operation as the corresponding binary
operator: `x += y`, `x -= y`, and `x *= y` are checked `+`, `-`, and `*` followed
by assignment.

Signed unary `-` is checked. Negating the minimum value of a signed integer type
is an integer overflow.

If an integer literal is too large to represent, or a suffixed literal is outside
the suffix type's range, the compiler rejects the program. If overflow is
provable while folding an integer expression at compile time, the compiler
rejects the program with an integer-overflow diagnostic. Runtime integer
overflow aborts through the arithmetic panic path.

Integer division and remainder trap on a zero divisor. Signed integer division
and remainder also trap for the minimum value divided or remaindered by `-1`.
Floating-point arithmetic follows section 5.5.

Explicit wrapping, saturating, and checked arithmetic operations are distinct
from plain arithmetic. They are not aliases for the plain checked operators.
Primitive integer `.wrapping_add`, `.wrapping_sub`, `.wrapping_mul`,
`.saturating_add`, `.saturating_sub`, `.saturating_mul`, `.checked_add`,
`.checked_sub`, and `.checked_mul` are defined for fixed-width integer types.
The checked methods return `Option<T>`: `Some(value)` when the result is
representable, `None` on overflow or unsigned subtraction underflow.
`wrapping_mul` on `i32` is JavaScript's `Math.imul`, and on `u32` it is
`Math.imul(a, b) >>> 0`. Bitwise `^`, `&`, `|` and shifts never overflow; `>>`
on an unsigned type is a logical shift, like JavaScript's `>>>`. Other
explicit primitive integer arithmetic families remain unsupported unless a
separate API surface defines them.

### 5.5 Floating-point arithmetic

`f64` is IEEE 754 binary64 and `f32` is binary32, both rounding to nearest, ties to even. On the C backend every `+`, `-`, `*` and `/` rounds once, in the order the source writes it:

- No contraction. `a * b + c` rounds the product, then the sum. The C backend passes `-ffp-contract=off` and `-fno-fast-math` to gcc and clang and `/fp:precise` to MSVC, and the generated C carries pragmas that keep contraction off when a host compiles it with its own flags.
- No reassociation. The compiler does not fold or reorder float expressions.
- `sqrt`, `floor`, `abs`, `min` and `max` give the same bits as JavaScript's `Math.sqrt`, `Math.floor`, `Math.abs`, `Math.min` and `Math.max`. `sqrt` is correctly rounded; the rest are exact. `min` and `max` are IEEE 754 minimum and maximum: a NaN operand gives NaN, and `-0.0` counts as less than `0.0`. Both forms work: `x.min(y)` and `min(x, y)`.
- `x.mul_add(a, b)` is the one explicit fused multiply-add: one rounding, by request.
- `x.to_bits()` gives the bit pattern as `u64` (`u32` for `f32`) and `f64::from_bits(u)` builds a value from one.
- `%` on floats is C `fmod`: the result has the dividend's sign, and a zero divisor gives NaN.
- `x.fmod_or_zero(y)` and `x.div_or_zero(y)` are `%` and `/` except that a zero divisor gives `0.0`, the rule Low Water's trace format uses.
- `x.to_int32()` and `x.to_uint32()` are JavaScript's ToInt32 and ToUint32 (`x | 0` and `x >>> 0`): truncate toward zero and wrap modulo 2^32; NaN and the infinities give 0.

Text. `{}` in a format string writes a float the way Rust's `Display` does: the shortest digits that read back to the same value, always in positional notation (`1e21` prints as `1000000000000000000000`). `x.to_json()` writes the text JavaScript's `JSON.stringify` writes: the same shortest digits, exponent form below `1e-6` and from `1e21` (`1.5e-7`, `1e+21`), `-0.0` as `0`, and NaN or an infinity as `null`. On an integer, `to_json()` gives its decimal form. Traces that another implementation compares byte for byte use `to_json()`.

These are the rules Low Water's DETMATH spec asks of a port, so a BuildLang implementation of a JS algorithm gives the same bits. `tests/detmath/goldens.bld` reproduces DETMATH's twelve golden calls bit for bit.

```build in-main
let x = 0.1 * 3.0;
let b = x.to_bits();
let y = f64::from_bits(b);
println!("{:016x} {}", b, y == x);
println!("{}", (0.0).min(-0.0).to_bits() == (-0.0).to_bits());
println!("{} {} {}", (1.5e-7).to_json(), (-0.0).to_json(), (0.1 + 0.2).to_json());
```

`exp`, `ln` and `pow` on `f64` (`x.exp()`, `x.ln()`, `x.powf(y)`, `x.powi(n)`, free `exp` and `pow`, and the `**` operator) are the fdlibm 5.3 algorithms, built from the operations above and 32-bit word operations. They give the same bits on every C library and compiler, so they are specified to the bit rather than by an error bound:

- **The reference.** `tools/detmath/transcendental.mjs` is the same algorithm in JavaScript and gives the same bits. `tests/detmath/transcendental.bld` checks 1248 results against it.
- **Accuracy.** Each result is within 1 ulp of the exact value. On a corpus of 20,000 inputs, measured against 200-bit arithmetic: `exp` and `log` are correctly rounded on about 90 percent of inputs, and `pow` has a largest error of 0.79 ulp. `pow` of an integer to an integer power is exact when the result fits.
- **JavaScript engines.** `exp` and `ln` equal Node 25's `Math.exp` and `Math.log` on that corpus. `exp(1.0)` is `e` correctly rounded, as in V8. Engines do not promise these bits and newer V8 releases compute them another way, so an implementation that must agree with BuildLang imports the twin rather than calling `Math`.
- **Special cases follow JavaScript.** `pow(1.0, inf)`, `pow(1.0, NaN)` and `pow(-1.0, inf)` are NaN, where C99 gives 1.

The other transcendental functions (`sin`, `cos`, `atan2`, `cbrt` and the rest), and the `f32` versions, still call the platform's C library, which is not correctly rounded and differs between platforms. Code that must replay them bit for bit implements them from the operations above, as DETMATH does.

## 6. Algebraic Effects

### 6.1 Effect Declaration
```build name=error-effect
effect Error {
    fn raise(msg: str) -> (),
}
```

### 6.2 Performing Effects
```build name=might-fail uses=error-effect
fn might_fail() ~ Error {
    perform Error.raise("something went wrong");
}
```

### 6.3 Handling Effects
```build uses=error-effect,might-fail
fn main() ~ Console {
    let status = handle {
        might_fail();
        0
    } with {
        Error.raise(msg) => {
            println!("caught: {}", msg);
            1
        },
    };
    println!("status {}", status);
}
```

Handlers are one-shot and do not resume (setjmp/longjmp on the C backend). When a handler clause runs, its value becomes the value of the `handle` expression and the code after the `perform` does not run. In the example, `status` is `1` and `might_fail` stops at its `perform`.

## 7. Standard Library

### 7.1 Core Module (`core.bld`)
Constants: `pi()`, `e()`, `tau()`
Integer: `i32_min`, `i32_max`, `i32_abs`, `i32_clamp`
Float: `f64_min`, `f64_max`, `f64_abs`, `f64_clamp`
Predicates: `is_even`, `is_odd`, `is_positive`, `is_negative`, `is_zero`
Number theory: `gcd`, `lcm`, `factorial`, `fibonacci`, `power`

### 7.2 Math Module (`math.bld`)
Angle conversion: `deg_to_rad`, `rad_to_deg`
Interpolation: `lerp`, `smoothstep`, `inverse_lerp`

### 7.3 Built-in Functions
I/O: `println!`, `print!`, `eprintln!`
Collections: `vec_new`, `vec_push`, `vec_get`, `vec_len`, `vec_pop`
HashMap: `map_new`, `map_insert`, `map_get`, `map_contains`, `map_len`
String: `to_string_i32`, `to_string_f64`
File I/O: `read_file`, `write_file`, `file_exists`
Environment: `getenv`, `clock_ms`, `args_count`, `args_get`
Math: `sqrt`, `sin`, `cos`, `tan`, `log`, `exp`, `pow`, `abs`, `floor`, `ceil`
Assert: `assert`, `assert_eq`

### 7.4 Vectors and strings in practice

A `let` bound to `vec_new()` with no annotation takes its element type from the first value pushed into it (`v.push(x)` or `vec_push(&mut v, x)`); a later push of another type is a check error (BL0901). With no push before the vector is used it holds `i32`, so annotate it there: `let mut v: Vec<str> = vec_new();`. The free functions accept the vector by value or borrowed (`vec_push(&mut v, x)`, `vec_len(&v)`), and the method forms (`v.push(x)`, `v.len()`, `v[i]`) work the same way. Strings split with `split`, iterate with `chars`, and compare with `<` and `==`. `s.substring(start, len)` takes a byte offset and a length: `"hello".substring(1, 3)` is `"ell"`. The C backend has no `.parse()` yet; the check rejects it with BL0901, so convert digits by hand:

```build
fn parse_int(s: str) -> i64 {
    let mut n: i64 = 0;
    for c in s.chars() {
        n = n * 10 + (c as i64 - 48);
    }
    n
}

fn main() ~ Console {
    // Give a vector its element type where it is created.
    let mut words: Vec<str> = vec_new();
    for w in "b a c".split(" ") {
        words.push(w);
    }
    println!("{} words, first {}", words.len(), words[0]);

    // Strings compare with < and ==.
    if words[1] < words[0] {
        println!("{} sorts before {}", words[1], words[0]);
    }

    // There is no `.parse()` on the C backend yet: convert digits by hand.
    let mut total: i64 = 0;
    for line in "12\n30\n7".split("\n") {
        total = total + parse_int(line);
    }
    println!("total {}", total);
}
```

### 7.5 Boxes and recursive types

`Box<T>` is an owned pointer to a `T` on the heap. `Box::new(x)` moves `x` into a new box and `*b` reads the value back. A struct or enum that holds itself must hold itself through a `Box`, because a type that contains itself by value has no finite size; the check rejects it with BL0384 and names the cycle. A cycle through `Option` or `Result` counts too, since both store their payload inline. `Vec<T>` also breaks a cycle.

```build
enum Tree {
    Leaf,
    Node(Box<Tree>, i64, Box<Tree>),
}

fn insert(t: Tree, v: i64) -> Tree {
    match t {
        Tree::Leaf => Tree::Node(Box::new(Tree::Leaf), v, Box::new(Tree::Leaf)),
        Tree::Node(l, x, r) => {
            if v < x {
                Tree::Node(Box::new(insert(*l, v)), x, r)
            } else {
                Tree::Node(l, x, Box::new(insert(*r, v)))
            }
        },
    }
}

fn size(t: Tree) -> i64 {
    match t {
        Tree::Leaf => 0,
        Tree::Node(l, _, r) => size(*l) + 1 + size(*r),
    }
}

fn main() ~ Console {
    let t = insert(insert(insert(Tree::Leaf, 2), 1), 3);
    println!("{} nodes", size(t));
}
```

```build fail=BL0384
enum Chain {
    End,
    Link(i64, Chain),
}
```

Limits in this release: boxed memory is not freed until the program exits; `Rc::new` and `Arc::new` are rejected (BL0383) because reference counting does not exist yet; `Box<dyn Trait>` is not supported.

### 7.6 JSON

`mod json;` loads the JSON module from the standard library. `json_parse(text)` returns `Result<Json, str>`, where `Json` is an enum (`Null`, `Bool`, `Num`, `Str`, `Arr`, `Obj`). It accepts exactly the JSON grammar: a trailing comma, a leading zero, a bare word or text after the value is an `Err` with the byte offset. A repeated key keeps its first position and its last value, as `JSON.parse` does.

Typed reads go through checked accessors that name the key in their error: `field_f64`, `field_i64` (refuses a fraction), `field_str`, `field_bool`, `field_items` (an array as `Vec<Json>`), and `field_f64_or` for an optional number. A struct reader is a function that returns `Result<T, str>` and uses `?` on each field.

```build
mod json;

struct Hit {
    frame: i64,
    damage: f64,
}

fn read_hit(j: Json) -> Result<Hit, str> {
    Ok(Hit { frame: field_i64(j, "frame")?, damage: field_f64(j, "damage")? })
}

fn main() ~ Console {
    match json_parse("{\"frame\": 4}") {
        Ok(j) => match read_hit(j) {
            Ok(h) => println!("{} {}", h.frame, h.damage),
            Err(e) => println!("{}", e),
        },
        Err(e) => println!("{}", e),
    }
}
```

That program prints ``json: missing key `damage` ``. The writer builds compact text with no spaces: `json_quote(s)` escapes a string as `JSON.stringify` does, `x.to_json()` writes a number, `json_arr(values)` and `json_obj(keys, values)` join already-written values with keys in the order given, and `json_obj_sorted(keys, values)` sorts the keys by byte order. `json_stringify(j)` and `json_stringify_sorted(j)` write a parsed value back. One record a line with fixed key order is a JSON-lines trace that another implementation can compare byte for byte.

A module function can be called inside a macro's arguments, as in `println!("{}", json_stringify(j))`. From 1.10.0 the merge renames those calls with the module's other calls; 1.9.0 reported them as undefined names and needed a `let` first.

### 7.7 Maps

`HashMap<str, T>` holds any value type `T`: integers of every width, floats, `bool`, `str`, tuples, structs, enums, `Vec<T>` and other maps. Values wider than eight bytes are stored in their own heap copy, so they come back whole. `m.insert(k, v)` adds or replaces, `m.get(k)` returns the value (or the zero value when the key is missing; `match m.get(k) { Some(v) => ..., None => ... }` tells the two apart), `m.contains_key(k)`, `m.remove(k)`, `m.len()` and `m.is_empty()` do what their names say, and `m.keys()` returns a `Vec<str>`. The methods work on a map held directly, in a struct field, or through `&HashMap` and `&mut HashMap`.

`keys()` returns the keys in insertion order. Replacing a value keeps the key's place, and a key that is removed and inserted again moves to the end, as in a JavaScript `Map`. The order is the same on every run, so a trace that walks a map is byte-stable.

```build
use std::collections::HashMap;

struct Tool {
    reach: f64,
    weight: f64,
}

fn heaviest(tools: &HashMap<str, Tool>) -> str {
    let mut best = "";
    let mut most = -1.0;
    for k in tools.keys() {
        let t = tools.get(k);
        if t.weight > most {
            most = t.weight;
            best = k;
        }
    }
    best
}

fn main() ~ Console {
    let mut tools: HashMap<str, Tool> = HashMap::new();
    tools.insert("pipe", Tool { reach: 1.75, weight: 2.0 });
    tools.insert("axe", Tool { reach: 1.25, weight: 3.5 });
    println!("{}", heaviest(&tools));
}
```

## 8. Code Generation

### 8.1 Compilation Targets
- **C99** (primary): Portable, debuggable, uses system C compiler for native output
- **HLSL**: DirectX shaders and ReShade effects
- **GLSL**: OpenGL and Vulkan shaders
- **SPIR-V**: GPU compute and graphics (binary format)
- **LLVM IR**: Textual LLVM intermediate representation
- **WebAssembly**: Web and edge deployment (text format)
- **x86-64**: Experimental native machine code
- **ARM64**: Experimental native machine code

### 8.2 C Backend
References compile to pointers. Structs compile to C structs. Enums compile to tagged unions. The runtime library (~190 C functions) is embedded in every output.

### 8.3 C ABI export

An `extern "C" fn` definition exports a function under its own name with the C calling convention. Its parameters and return type must have a C layout: integers, floats, `bool`, raw pointers, and structs marked `#[repr(C)]` whose fields are those types or fixed arrays of them. Any other type in an export signature, such as `str`, `Vec`, an enum or a struct without `#[repr(C)]`, is refused at check time with BL0901 and a fix.

```build name=c-abi-export
#[repr(C)]
struct Vec2 {
    x: f64,
    z: f64,
}

extern "C" fn lw_len2(v: Vec2) -> f64 {
    v.x * v.x + v.z * v.z
}

extern "C" fn lw_shift(v: *mut Vec2, dx: f64) {
    unsafe {
        (*v).x = (*v).x + dx;
    }
}
```

```build fail=BL0901
extern "C" fn lw_name(s: str) -> i32 {
    0
}
```

A slice has no C layout, so an array crosses the boundary as a pointer and a length. Inside the export, `slice::from_raw_parts(p, n)` reads them as a `&[T]` of `n` elements. Indexing that slice is checked against `n` and stops the program with exit code 101 when the index is out of range. The call trusts `p` and `n`, so it needs an `unsafe` block or an `unsafe fn`; elsewhere it is refused with BL0382. A `&[T]` parameter in an export signature is refused with BL0901, and its fix names this form.

```build name=c-abi-slice
extern "C" fn lw_sum(p: *const f32, n: usize) -> f64 {
    let xs = unsafe { slice::from_raw_parts(p, n) };
    let mut s = 0.0;
    let mut i: usize = 0;
    while i < xs.len() {
        s = s + xs[i] as f64;
        i = i + 1;
    }
    s
}
```

```build fail=BL0382
extern "C" fn lw_first(p: *const f32, n: usize) -> f32 {
    let xs = slice::from_raw_parts(p, n);
    xs[0]
}
```

`buildc build --emit staticlib DIR` compiles a project with no `fn main` into a static library (`libmain.a` with gcc or clang, `main.lib` with MSVC) in `DIR/target/debug`, next to a generated header `main.h` and a file `main.link`. The header defines every `#[repr(C)]` struct the exports reach, in an order C accepts, and declares each export inside `extern "C"` guards for C++ hosts. `main.link` lists the libraries the BuildLang runtime needs, one flag a line; the host adds them to its link line. A project that defines `fn main` is refused, because the host owns `main`. `BUILDLANG_CC` picks the C compiler. `--emit header` writes only the header. See `docs/C-ABI.md` for a full host example.

## 9. Grammar Summary (EBNF)

```ebnf
module     = item* ;
item       = function | struct_def | enum_def | trait_def | impl_def
           | type_alias | const_def | static_def | mod_def | use_def ;
function   = "fn" IDENT generics? "(" params? ")" ("->" type)? block ;
struct_def = "struct" IDENT generics? ( "{" fields "}" | "(" types ")" | ";" ) ;
enum_def   = "enum" IDENT generics? "{" variants "}" ;
block      = "{" stmt* expr? "}" ;
stmt       = local | expr_stmt | item ;
local      = "let" "mut"? pattern (":" type)? ("=" expr)? ";" ;
expr       = literal | ident | path | unary | binary | call | method_call
           | field | index | if | match | for | while | loop | block
           | closure | ref | deref | return | break | continue ;
pattern    = "_" | ident | literal | tuple | struct | enum | or | slice ;
type       = path | "&" lifetime? "mut"? type | "[" type ";" expr "]"
           | "(" types ")" | "fn" "(" types ")" "->" type ;
```

## 10. Conformance

A conforming implementation must:
1. Accept all programs that conform to this specification
2. Reject programs that violate the borrowing rules (Section 3.2)
3. Reject non-exhaustive match expressions over enum types (Section 4.3)
4. Produce correct output for all built-in operations on primitive types
5. Implement the precedence table in Section 1.6
