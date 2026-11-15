# Using the BuildLang VS Code Extension

This extension provides **editor support** for BuildLang source files: syntax
highlighting and language configuration (brackets, comments, auto-closing
pairs, folding, and a file icon) for files ending in `.bld`.

It is intentionally small. It does **not** include or run the BuildLang
compiler. Building and running `.bld` programs is done with the `buildc`
toolchain from the [language repo](https://github.com/HarperZ9/buildlang).

---

## Install

From the Marketplace: **View -> Extensions**, search **"BuildLang"**, click
**Install**.

From a packaged `.vsix` (for example the one built in this repo):

```bash
code --install-extension buildlang-0.1.0.vsix
```

To build the `.vsix` yourself from a clone of this repository:

```bash
npx --yes @vscode/vsce@3.9.2 package --no-dependencies
```

This produces `buildlang-<version>.vsix` in the repository root.

---

## What you get

The capabilities below are declared in `package.json`, the TextMate grammar
(`syntaxes/buildlang.tmLanguage.json`), and `language-configuration.json`:

- **Syntax highlighting** for `.bld` files, covering:
  - 61 language keywords, including the effect system (`with`, `effect`,
    `handle`, `resume`, `perform`) and AI primitives (`ai`, `neural`,
    `infer`);
  - primitive and standard-library types (`i8` through `u128`, `f32`/`f64`,
    `bool`, `char`, `str`, `String`, `Vec`, `Option`, `Result`, and more);
  - numeric literals (hex, octal, binary, float, integer, with type suffixes);
  - strings with escape sequences and `{...}` interpolation, raw strings, and
    character literals;
  - line (`//`, `///`) and block (`/* */`, `/** */`) comments;
  - attributes (`#[...]`, `#![...]`), lifetimes (`'a`, `'static`), macros
    (`name!`), and operators.
- **Editor configuration** from `language-configuration.json`:
  - bracket matching and auto-closing for `{}`, `[]`, `()`, and `"`;
  - comment toggling with the editor's default shortcut (`Ctrl+/`), using
    `//` for lines and `/* */` for blocks;
  - surrounding-pair wrapping (for example, selecting text and typing `"`);
  - folding on `// #region` / `// #endregion` markers.
- A **file icon** for `.bld` files in the Explorer.

There are no contributed commands, settings, snippets, or tasks. Activation is
automatic when you open a `.bld` file.

---

## Verify it is active

1. Open or create a file named `hello.bld`.
2. Look at the VS Code status bar (bottom right). The language indicator should
   read **BuildLang**.
3. If it does not, click the language indicator and choose **BuildLang** from
   the language picker, or run **"Change Language Mode"** from the Command
   Palette and select **BuildLang**.

Once the language mode is BuildLang, keywords, types, strings, and comments
are colorized according to your active color theme.

---

## Worked examples

The snippets below use real BuildLang syntax. Highlighting colors depend on
your color theme; the descriptions of what gets highlighted are *illustrative*
(not captured from a specific theme).

### 1. Hello world

`hello.bld`:

```build
fn main() ~ Console {
    println!("Hello from BuildLang!");
}
```

Expected highlighting (illustrative):

- `fn` -> keyword
- `main` -> function name
- `Console` -> type
- `println!` -> macro
- `"Hello from BuildLang!"` -> string literal

### 2. Types, mutation, and control flow

`ledger.bld`:

```build
fn add_month(balance: i32, interest: i32) -> i32 {
    balance + interest
}

fn main() ~ Console {
    let mut balance: i32 = 100;
    let mut month: i32 = 0;

    while month < 3 {
        balance = add_month(balance, 5);
        month = month + 1;
    }

    println!("balance: {}", balance);
}
```

Expected highlighting (illustrative):

- `fn`, `let`, `mut`, `while` -> keywords
- `i32` -> primitive type
- `add_month` -> function name at definition and call sites
- `100`, `0`, `3`, `5` -> numeric literals
- `{}` inside the string -> interpolation placeholder

### 3. The effect system

`effects_greeting.bld`:

```build
effect Greeting {
    fn greet(name: str) -> (),
}

fn welcome() ~ Greeting {
    perform Greeting.greet("teammate");
}

fn main() ~ Console {
    handle {
        welcome()
    } with {
        Greeting.greet(name) => {
            println!("Hello, {}!", name)
        },
    }
}
```

Expected highlighting (illustrative):

- `effect`, `perform`, `handle`, `with` -> effect-system keywords
- `Greeting`, `Console` -> types
- `str` -> primitive type
- `=>` -> operator

### 4. Comment toggling and folding

```build
// #region helpers
/// Doc comment for a helper.
fn double(x: i32) -> i32 {
    x * 2 // inline comment
}
// #endregion
```

- With the cursor on (or a selection over) any line, the comment-toggle
  shortcut (`Ctrl+/`) inserts or removes a leading `//`.
- The `// #region` / `// #endregion` markers create a foldable section in the
  editor gutter.

---

## Running `.bld` programs

This extension does not compile or run code. To execute the examples above,
use the `buildc` toolchain from the
[language repo](https://github.com/HarperZ9/buildlang). For example, after
installing that toolchain on your `PATH`:

```bash
buildc run hello.bld
```

Refer to the language repo's README for the full, authoritative list of
`buildc` subcommands, targets, and flags.
