# Editor support

Three folders. Each is self-contained and keeps its own licence file.

| Folder | What it is | Came from |
|---|---|---|
| `vscode/` | VS Code extension, version 1.0.5: grammar (scope `source.bld`), 19 snippets, bracket rules, and a client that starts `buildc lsp` | written in this repository |
| `vscode-syntax/` | VS Code extension, version 0.1.0: grammar (scope `source.build`), bracket rules and a file icon, with no code to run | `HarperZ9/buildlang-vscode`, `main` at `c7cc292`, moved here 2026-11-15 |
| `tmLanguage/` | The TextMate grammar alone, with sample `.bld` files, for Sublime Text, TextMate and other editors | `HarperZ9/buildlang-tmLanguage`, `main` at `d9c3de1`, moved here 2026-11-15 |

`vscode-syntax/syntaxes/buildlang.tmLanguage.json` and `tmLanguage/grammars/buildlang.tmLanguage.json`
are one grammar in two places, and the `editors` workflow fails if they differ. The grammar in
`vscode/` is a separate, larger one with a different scope name. Bringing the three down to one
grammar and one extension is open work.

## What is published where

- **VS Code Marketplace.** The one listing is `HarperZ9.quantalang`, version 0.1.0, published
  2026-04-12 under the language's earlier name. Its source links go to
  `github.com/HarperZ9/quantalang-vscode`, which GitHub redirects to `HarperZ9/buildlang-vscode`.
  No `HarperZ9.buildlang` listing exists. Neither extension in this folder has been published
  from here.
- **GitHub Linguist.** Not included. github-linguist/linguist#7913 was closed without a merge on
  2026-04-12.
- **npm.** Nothing is published.

The two old repositories stay in place and are not archived, because the Marketplace listing
links to one of them.
