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

## Limits

- One library per project, named `main`. Symbol names are the function names, so pick a prefix.
- Exports cannot take or return heap data such as `str` or `Vec`; the check refuses them.
