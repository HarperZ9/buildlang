---
name: buildlang
description: Write, check and fix BuildLang (.bld) programs with buildc. Use when a task asks for BuildLang code, when a file ends in .bld, or when buildc reports a BLxxxx diagnostic.
---

# Writing BuildLang

BuildLang is a Rust-like systems language. `buildc` compiles it to C and
native binaries. Its distinctive rule: a function's signature lists every
capability it uses, and the checker rejects code that uses one it did not
declare.

## The loop

1. Write the program.
2. Run `buildc check FILE --error-format=json`. It prints one JSON report on
   stdout (schema `buildlang.diagnostics/v1`). `ok: true` means the program
   type-checks and the C backend can compile it.
3. For each diagnostic, read `code`, `span`, `help` and `suggestions`.
   `buildc explain BL0403` prints what a code means.
4. Apply a suggestion only after reading it. When it has `edits`, each edit
   replaces the bytes `span.byte_start..span.byte_end` with `replacement`
   (equal offsets mean an insertion); apply them from the end of the file
   backwards, and apply an edit repeated across diagnostics once. A suggestion
   with `widens_authority: true` adds a capability or `unsafe`; it is never
   marked `MachineApplicable`. Decide whether the program should have that
   authority before you add it.
5. Run `buildc run FILE` to execute.

## Capabilities

Ten capabilities: `Console`, `FileSystem`, `Network`, `Process`,
`Environment`, `Clock`, `Foreign`, `Gpu`, `Random`, `Model`. Join them with
`+`, never with a comma. A caller declares what its callees use.

```build
fn load(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ Console + FileSystem {
    let text = load("notes.txt");
    println!("{} bytes", text.len());
}
```

## What the C backend supports today

- An unannotated `let mut v = vec_new();` takes its element type from the
  first value pushed (`v.push(x)` or `vec_push(&mut v, x)`). Write the type
  (`let mut v: Vec<str> = vec_new();`) when nothing is pushed before the
  vector is used, or when you want the checker to hold you to it.
- Strings: `split`, `chars`, `len`, `==`, `<`, and `+` between strings.
- No `.parse()` and no `unwrap` on parsed values: convert digits by hand.
- No `Box<T>` yet, so a type cannot contain itself. Build trees and lists as a
  `Vec` of nodes that refer to each other by index.
- Effect handlers are one-shot: the handler's value becomes the value of the
  `handle` expression, and code after the `perform` does not run.
- `println!` and `format!` take a string literal first; pass values through
  `{}` placeholders.

```build
fn digits(s: str) -> i64 {
    let mut n: i64 = 0;
    for c in s.chars() {
        n = n * 10 + (c as i64 - 48);
    }
    n
}

fn main() ~ Console {
    let mut words: Vec<str> = vec_new();
    for w in "12 30 7".split(" ") {
        words.push(w);
    }
    let mut total: i64 = 0;
    for w in words {
        total = total + digits(w);
    }
    println!("{}", total);
}
```

## References

- `SPECIFICATION.md`: the language.
- `docs/EFFECTS_GUIDE.md`: capabilities, effects and handlers.
- `docs/ERROR_CODES.md`: every diagnostic code.
- `docs/schemas/buildlang.diagnostics.v1.schema.json`: the JSON report.
