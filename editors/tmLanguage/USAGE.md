# Using the BuildLang TextMate Grammar

This repository is an **editor-facing TextMate grammar**, not a compiler or a
runnable program. There is no CLI to install and no library to import. "Using"
this package means installing the grammar into a TextMate-compatible editor (or
consuming it as a vendored grammar) so that `.bld` source files get syntax
highlighting.

The compiler and language semantics (including any `buildc` command) live in a
separate repository, [`HarperZ9/buildlang`](https://github.com/HarperZ9/buildlang).
This package only describes how `.bld` text is tokenized for highlighting.

## What the package provides

| File | Purpose |
|------|---------|
| `grammars/buildlang.tmLanguage.json` | The TextMate grammar (scope name `source.bld`, file type `.bld`) |
| `language-configuration.json` | Comment tokens, brackets, auto-closing/surrounding pairs, folding markers |
| `samples/` | Representative `.bld` files used for language detection and highlighting checks |

The grammar registers:

- Scope name: `source.bld`
- File extension: `.bld`

## Install / use

### VS Code

Install the [BuildLang VS Code extension](https://marketplace.visualstudio.com/items?itemName=HarperZ9.buildlang),
which bundles this grammar. No manual file copying is needed; opening any
`.bld` file is highlighted automatically.

### Sublime Text (ST4+)

Sublime Text accepts `.tmLanguage.json` files directly. Copy the grammar into
your `Packages/User` directory:

```bash
# macOS example path; adjust for your OS
cp grammars/buildlang.tmLanguage.json \
  "$HOME/Library/Application Support/Sublime Text/Packages/User/"
```

Open a `.bld` file; Sublime maps the `source.bld` scope to your color
scheme automatically.

### TextMate / other TextMate-compatible editors

Place `grammars/buildlang.tmLanguage.json` (and, where the editor supports it,
`language-configuration.json`) in the editor's grammar/bundle directory. The
exact location is editor-specific; consult that editor's grammar/bundle docs.

### github-linguist

This repository is intended to be vendored by
[`github-linguist/linguist`](https://github.com/github-linguist/linguist) under
`vendor/grammars/buildlang-tmLanguage`. Consumers of GitHub do not install
anything; the `samples/` files drive language detection.

## Validating and packaging the grammar

These are the genuine commands this repo uses (mirrored from CI in
`.github/workflows/`). They require only Python 3 (the standard library
`json.tool` module) and git.

### Validate the JSON files

```bash
python -m json.tool grammars/buildlang.tmLanguage.json > /dev/null
python -m json.tool language-configuration.json > /dev/null
```

Expected: no output and a zero exit code when the JSON is well-formed. A
malformed file prints a `JSONDecodeError` with the line/column.

Verified by author: both commands exit 0 on the current tree.

### Check for whitespace problems

```bash
git diff --check
```

Expected: no output (and exit 0) when there are no trailing-whitespace or
conflict-marker issues in the working tree.

### Build the source package (what the release workflow does)

The `release-artifacts` workflow builds `dist/buildlang-tmLanguage-0.1.0.zip`
from the publishable files using a short Python script. The committed
`dist/buildlang-tmLanguage-0.1.0.zip` is the result of that workflow.

See `.github/workflows/release-artifacts.yml` for the exact build step.

## Worked examples (highlighting)

The grammar maps BuildLang constructs to TextMate scopes. The examples below
show which scope a given token receives, taken directly from
`grammars/buildlang.tmLanguage.json`. These are illustrative of the grammar's
behavior, not output from a tokenizer run.

### Example 1 — keywords and effect/AI primitives

Source:

```build
fn handle_request() {
    let x = perform infer();
}
```

Resulting scopes (illustrative):

- `fn` -> `keyword.other.fn.bld`
- `let` -> `storage.type.bld`
- `perform` -> `keyword.other.effect.bld`
- `infer` -> `keyword.other.ai.bld`
- `handle_request` (after `fn`) -> `entity.name.function.bld`

### Example 2 — types and primitives

Source:

```build
let buf: Vec<u8> = Vec::new();
```

Resulting scopes (illustrative):

- `Vec`, `u8` -> `storage.type.primitive.bld`
- A capitalized identifier not in the primitive list (e.g. a user type
  `Color`) -> `entity.name.type.bld`

### Example 3 — numbers and string interpolation

Source:

```build
let n = 0xFF_u32;
let s = "value is {n}";
```

Resulting scopes (illustrative):

- `0xFF_u32` -> `constant.numeric.hex.bld`
- The `"..."` body -> `string.quoted.double.bld`
- `{n}` inside the string -> `meta.interpolation.bld`, with `n` as
  `variable.other.bld`

### Example 4 — comments and attributes

Source:

```build
/// Doc comment
#[derive(Clone)]
struct Color {}
```

Resulting scopes (illustrative):

- `/// Doc comment` -> `comment.line.documentation.bld`
- `#[derive(Clone)]` -> `meta.attribute.bld` (with `derive` as
  `entity.name.tag.bld`)
- `struct` -> `storage.type.bld`

To see highlighting end-to-end, open any file in `samples/` (or
`examples/demo.bld`) in an editor that has the grammar installed.
