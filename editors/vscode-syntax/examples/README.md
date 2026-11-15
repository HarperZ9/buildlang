# Examples

Best-effort demo - not runtime-verified by author.

## `demo.bld`

A sample BuildLang source file that exercises the highlighting and
editor-configuration surface this extension provides: keywords, the effect
system (`effect`, `perform`, `handle`, `with`), types, numeric literals,
strings with `{...}` interpolation, line/block comments, an attribute
(`#[inline]`), a macro (`println!`), operators, and `// #region` /
`// #endregion` folding markers.

## `browser-workflow.bld`

A vocabulary fixture for Telos browser evidence and Index/Forum model-council
routing. It is editor-only documentation; this extension does not compile or
execute browser automation.

### Try it

1. Install this extension (see [../README.md](../README.md) or
   [../USAGE.md](../USAGE.md)).
2. Open `demo.bld` in VS Code.
3. Confirm the status-bar language indicator reads **BuildLang**. If not, use
   **Change Language Mode** from the Command Palette and select **BuildLang**.
4. Observe that keywords, types, strings, and comments are colorized by your
   active theme, and that the `// #region` block can be folded from the gutter.

### What this demo does and does not do

- It **does** demonstrate the editor features contributed by this extension.
- It **does not** compile or run. This extension is editor support only. To
  execute `.bld` programs, use the `buildc` toolchain from the
  [language repo](https://github.com/HarperZ9/buildlang):

  ```bash
  buildc run demo.bld
  ```

  Consult that repository's README for the authoritative list of `buildc`
  subcommands and flags.
