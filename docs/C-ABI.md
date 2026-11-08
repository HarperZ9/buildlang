# C ABI export

BuildLang code can be linked into a C or C++ program as a static library. This is how a host
such as a game engine calls BuildLang systems: the host keeps `main`, the window and the loop, and
calls exported functions through a plain C header.

## Write the exports

Mark each function the host calls `extern "C"`, and mark each struct those functions take or
return `#[repr(C)]`. The struct's fields keep their declared order with C alignment, so the
header's definition and the compiled code agree on the layout.

```text
#[repr(C)]
struct Vec2 { x: f64, z: f64 }

#[repr(C)]
struct Fighter { id: i32, pos: Vec2, hp: f64, alive: bool }

extern "C" fn lw_damage(f: Fighter, amount: f64) -> Fighter {
    let hp = f.hp - amount;
    Fighter { id: f.id, pos: f.pos, hp: hp, alive: hp > 0.0 }
}

extern "C" fn lw_step(f: *mut Fighter, dx: f64) {
    unsafe { (*f).pos.x = (*f).pos.x + dx; }
}
```

An export may use integers, floats, `bool`, raw pointers and `#[repr(C)]` structs of those types,
with fixed arrays allowed inside struct fields. The check refuses anything else with BL0901: a
`str` has no C layout, so pass a `*const u8` and a length; an enum has no stable layout, so pass an
integer tag.

## Pass arrays

A `&[T]` has no C layout, so the host passes a pointer and a length, and the export reads them as
a slice:

```text
extern "C" fn lw_total(p: *const f32, n: usize) -> f64 {
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

- **Checked reads.** `xs[i]` is checked against `n`. An index at or past `n` stops the program
  with exit code 101 and `index out of bounds: the len is N but the index is I`, instead of
  reading past the host's array.
- **Run-time sizes.** The length is a value, so one library serves tables of any size. The
  earlier workaround, a `#[repr(C)]` struct holding one fixed array, fixed the size at compile time.
- **`unsafe`.** `slice::from_raw_parts` trusts the pointer and the length the host passed, so it
  must sit in an `unsafe` block or an `unsafe fn` (BL0382 otherwise). Everything after it is checked.
- **Inside BuildLang.** A `&[T]` parameter accepts `&arr` for a fixed array of the same element
  type, with the array's length. An array of another element type is refused rather than
  reinterpreted: `let c = [4, 9];` holds `i32`, so passing `&c` to a `&[i64]` needs
  `let c: [i64; 2] = [4, 9];`.

## Write into a host buffer

`slice::from_raw_parts_mut(p, n)` over a `*mut T` gives a `&mut [T]`, so an export can fill a buffer
the host owns, such as an audio period:

```text
extern "C" fn lw_scale(p: *mut f32, n: usize, k: f32) {
    let xs = unsafe { slice::from_raw_parts_mut(p, n) };
    let mut i: usize = 0;
    while i < xs.len() {
        xs[i] = xs[i] * k;
        i = i + 1;
    }
}
```

Writes are checked against `n` the same way reads are, and a `*const T` is refused (BL0303).

The header declares the pointer as `float*`; a C host holding a `const float*` passes it with a
cast. `compiler/tests/c_abi_slices.rs` links `tests/c-abi-slices/driver.c`, checks its output,
and checks that an index equal to the length traps.

## Build the library

```bash
buildc build --emit staticlib path/to/project
```

The project must not define `fn main`. The build writes three files to `target/debug` (or
`target/release` with `--release`):

| File | What it is |
|---|---|
| `libmain.a` (gcc, clang) or `main.lib` (MSVC) | the compiled code and the BuildLang runtime |
| `main.h` | the `#[repr(C)]` structs the exports reach, then one prototype per export, inside `extern "C"` guards |
| `main.link` | libraries the runtime needs (for example `-lm`, or `ws2_32` on Windows), one a line |

`BUILDLANG_CC` selects the C compiler; otherwise the build uses the compiler an executable build
would use.

## Link it from C

```text
#include <stdio.h>
#include "main.h"

int main(void) {
    Fighter f = { 7, { 1.5, -2.0 }, 10.0, true };
    Fighter g = lw_damage(f, 4.25);
    lw_step(&g, 0.5);
    printf("%d %.2f %.2f\n", g.id, g.pos.x, g.hp);
    return 0;
}
```

```bash
cc host.c -I target/debug target/debug/libmain.a $(cat target/debug/main.link) -o host
```

`compiler/tests/c_abi_export.rs` builds `tests/c-abi/main.bld` this way, links
`tests/c-abi/driver.c` against it, and compares the output with `tests/c-abi/driver.expected`.
The driver prints `sizeof(Fighter)` and a nested field written through a pointer, so a layout
disagreement between the header and the library changes the output.

## Keep state behind a handle

A library with state hands the host an opaque handle: a raw pointer to a struct without
`#[repr(C)]`. The struct may hold anything (`str`, `Vec`, maps), because the host never sees its
fields. The header forward-declares it, so C code can hold and pass the pointer and nothing else:

```text
struct Counter { name: str, hits: Vec<i64> }

extern "C" fn ct_new() -> *mut Counter {
    let b = Box::new(Counter { name: "c", hits: Vec::new() });
    b as *mut Counter
}

extern "C" fn ct_hit(c: *mut Counter, n: i64) -> i64 {
    unsafe {
        (*c).hits.push(n);
        (*c).hits.len() as i64
    }
}
```

The header says `typedef struct Counter Counter;` and declares `Counter* ct_new(void);`. A struct
passed by value still needs `#[repr(C)]`.

Strings cross as NUL-terminated bytes. Declare the C library functions in an `extern "C"` block
(`strlen`, `malloc`, `free`, and any other, `strtod` included) and copy: read `strlen(p)` bytes
with `slice::from_raw_parts`, and hand text out as a `malloc` buffer with a 0 byte at the end that
the handle frees on its next call. A `str` cannot be cast to a pointer (BL0383): its bytes need not
end in NUL.

`examples/lowwater-rules` is a full module written this way: raw-native's Low Water rules behind
`rules_c.h`, with a `static mut` for the last error. `compiler/tests/c_abi_rules.rs` links it.

## Limits

- One library per project, named `main`. Symbol names are the function names, so pick a prefix.
- Exports cannot take or return heap data such as `str` or `Vec` by value; the check refuses them.
  Put it in a struct behind an opaque handle instead.
- Exports cannot take or return a slice; pass a pointer and a length (see "Pass arrays").
- A slice made by `slice::from_raw_parts` lives in the function that made it. Do not return it.

## Carry a C source

An extern block can name a C file to compile with the program:

```text
extern "C" source "binding.c" {
    fn add3(x: i32) -> i32;
}
```

The path resolves against the directory of the `.bld` file that declares the block. The generated C
ends with `#include "<absolute path>"`, so the source shares the program's translation unit and needs no
extra compiler arguments in `run`, `build` or `--emit staticlib`. The prototypes BuildLang writes for the
block's functions come first, and the source's definitions must match them. `runtime/audio/bl_audio.c`
is a real-time audio binding written this way; `examples/audio` uses it.
