# BuildLang

**Algebraic effects as a first-class feature. Better error handling than Rust. Better concurrency than Go. Mathematical purity with practical power.**

## Why BuildLang?

Every programming language handles side effects differently - and most do it badly.

- **Rust** makes you wrap everything in `Result<T, E>` and chain `?` operators. Safe, but verbose.
- **Go** makes you check `if err != nil` after every function call. Simple, but tedious.
- **Haskell** uses monads. Powerful, but impenetrable to most programmers.
- **Java/Python** use exceptions. Easy, but invisible control flow that breaks reasoning.

BuildLang uses **algebraic effects** - a system that is:
- **As safe as Rust** - effects are tracked in the type system
- **As simple as Go** - just call functions, no ceremony
- **As powerful as Haskell** - full effect composition and abstraction
- **As practical as Python** - you don't need a PhD to use it

## How Effects Work

An **effect** is a declaration of WHAT can happen:
```build
effect FileSystem {
    fn read(path: str) -> str,
    fn write(path: str, content: str) -> (),
}
```

A function that uses an effect declares it in its type:
```build
fn process_config() ~ FileSystem -> Config {
    let content = perform FileSystem.read("config.toml");
    parse_config(content)
}
```

A **handler** decides HOW the effect executes:
```build
handle { process_config() } with {
    FileSystem.read(path) => |resume| {
        let data = actual_read_file(path);
        resume(data)
    },
    FileSystem.write(path, content) => |resume| {
        actual_write_file(path, content);
        resume(())
    },
}
```

A handler clause resumes only from tail position. When every exit of the clause is `resume(value)` (the clause body, the last expression of a block, each `if` branch, each `match` arm), the clause runs at the `perform` site as a function: `perform` returns `value` and the code after the `perform` runs. A clause without `resume` ends the `handle`: its value becomes the value of the `handle` expression and the code after the `perform` does not run (setjmp/longjmp on the C backend). A `resume` anywhere else, and a clause that resumes on some paths only, are refused at check time. Full one-shot continuations, with `resume` in any position, are a later step.

## Why This Matters

**Testing**: Swap the real file system handler for a mock - no dependency injection frameworks needed.

**Concurrency**: async/await is just an effect. Swap single-threaded for multi-threaded by changing the handler.

**Error handling**: No `Result`, no `?`, no `try/catch`. Errors are effects. Handle them where you want, not where they occur.

**Purity**: Functions with no effects are provably pure. The type system guarantees it.

## Language Features

- Algebraic effects with handler-based control flow
- Hindley-Milner type inference with effect rows
- Higher-kinded types for generic abstractions
- Const generics for compile-time computation
- Multi-target compilation (C, WASM, native)
- Effect polymorphism and row polymorphism
- Zero-cost abstraction through effect elimination
