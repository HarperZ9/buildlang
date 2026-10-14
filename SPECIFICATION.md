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
Floating-point arithmetic keeps the target backend's IEEE behavior.

Explicit wrapping, saturating, and checked arithmetic operations are distinct
from plain arithmetic. They are not aliases for the plain checked operators.
Primitive integer `.wrapping_add`, `.wrapping_sub`, `.wrapping_mul`,
`.saturating_add`, `.saturating_sub`, `.saturating_mul`, `.checked_add`,
`.checked_sub`, and `.checked_mul` are defined for fixed-width integer types.
The checked methods return `Option<T>`: `Some(value)` when the result is
representable, `None` on overflow or unsigned subtraction underflow. Other
explicit primitive integer arithmetic families remain unsupported unless a
separate API surface defines them.

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

Give a vector its element type where it is created: `vec_new()` with no annotation makes a vector of `i32`, and pushing a string into it is a check error (BL0901). The free functions accept the vector by value or borrowed (`vec_push(&mut v, x)`, `vec_len(&v)`), and the method forms (`v.push(x)`, `v.len()`, `v[i]`) work the same way. Strings split with `split`, iterate with `chars`, and compare with `<` and `==`. The C backend has no `.parse()` yet; the check rejects it with BL0901, so convert digits by hand:

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
    for line in "3\n4\n5".split("\n") {
        total = total + parse_int(line);
    }
    println!("total {}", total);
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
