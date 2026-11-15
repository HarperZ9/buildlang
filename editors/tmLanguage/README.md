<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/buildlang-tmLanguage/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/buildlang-tmLanguage/main/docs/art/hero-light.svg" alt="buildlang-tmLanguage: TextMate grammar that highlights BuildLang .bld files in editors. A fine lattice of lines bulges outward around a bright core, as if seen through a lens, inside a ring." width="100%">
</picture>

# buildlang-tmLanguage

TextMate grammar that highlights BuildLang .bld files in editors.

[![version: 0.1.0](https://img.shields.io/badge/version-0.1.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/buildlang-tmLanguage/releases/latest)
[![CI](https://github.com/HarperZ9/buildlang-tmLanguage/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/buildlang-tmLanguage/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/buildlang-tmLanguage/blob/main/LICENSE)

Official TextMate grammar for **[BuildLang](https://github.com/HarperZ9/buildlang)**.
This is the small editor-facing layer that teaches syntax highlighters how to
read `.bld` files without carrying compiler behavior or backend claims.

## What this is

A language definition that tells syntax-highlighting engines how to render `.bld` source files. The same grammar powers:

- The BuildLang VS Code extension
- Syntax highlighting on GitHub (via [github-linguist](https://github.com/github-linguist/linguist))
- Any TextMate-compatible editor (Sublime Text, TextMate, Atom, etc.)

## Files

| Path | Purpose |
|------|---------|
| `grammars/buildlang.tmLanguage.json` | The grammar definition |
| `language-configuration.json` | Comment tokens, brackets, surrounding-pair rules |
| `samples/` | Representative `.bld` files used by linguist for language detection |

## BuildLang in brief

BuildLang is an effects-oriented systems language. Keywords in this grammar fall into several categories:

- **Classical systems keywords** - `fn`, `struct`, `enum`, `trait`, `impl`, `let`, `mut`, `pub`, `mod`, `use`, `if`, `else`, `match`, `loop`, `while`, `for`, `in`, `return`, `break`, `continue`, `ref`, `move`, `unsafe`, `extern`, `async`, `await`, `dyn`, `where`, `typeof`, `sizeof`, `true`, `false`, `Self`, `self`, `crate`, `super`
- **Effect system** - `with`, `effect`, `handle`, `resume`, `perform`
- **AI/neural primitives** - `ai`, `neural`, `infer`
- **Module system** - `module` (ecosystem-level), alongside standard `mod`
- **Macros** - `macro`, `macro_rules`
- **Reserved** - `abstract`, `become`, `do`, `final`, `override`, `priv`, `try`, `yield`, `union`, `default`, `auto`, `box`

Primitive types recognized: `i8`, `i16`, `i32`, `i64`, `i128`, `isize`, `u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `f32`, `f64`, `bool`, `char`, `str`, `String`, and common standard types (`Vec`, `Option`, `Result`, `Box`, `Rc`, `Arc`, `HashMap`, `HashSet`, `BTreeMap`, `BTreeSet`).

## Usage

This is a TextMate grammar package, not a compiler or runnable program: there is
no CLI to run and nothing to import. "Using" it means installing the grammar into
a TextMate-compatible editor (or consuming it via github-linguist) so `.bld`
files get syntax highlighting. See **[USAGE.md](USAGE.md)** for install steps per
editor, the grammar's scope name and file type, the JSON-validation/packaging
commands, and worked highlighting examples.

The compiler and any `buildc` command live in the separate
[`HarperZ9/buildlang`](https://github.com/HarperZ9/buildlang) repository, not
here.

The browser evidence example is an editor fixture only. It documents Telos
workflow vocabulary for highlighting and examples; this package does not compile
or execute browser automation.

## Using this grammar

### VS Code

Install the [BuildLang VS Code extension](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang) - it bundles this grammar. The Marketplace listing still carries the earlier name, QuantaLang.

### Sublime Text / TextMate

Place the grammar file in the appropriate packages directory. Sublime Text accepts `.tmLanguage.json` directly since ST4.

### github-linguist

This repository is included as a submodule by `github-linguist/linguist` under `vendor/grammars/buildlang-tmLanguage`. Consumers do not need to install anything.

## License

MIT. See [LICENSE](LICENSE).

---

Built by **[Zain Dana Harper](https://harperz9.github.io)** in Seattle: evidence-first tools that leave a re-checkable artifact behind. The full workbench is at [Project Telos](https://harperz9.github.io).
