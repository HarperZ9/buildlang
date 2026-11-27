# Changelog

## 1.15.0 - 2026-11-26

- Aligned extension version with `buildc 1.15.0`.
- Added extension Marketplace icon (`icons/icon.png`) and language file icons (`icons/build-light.png`, `icons/build-dark.png`).
- Aligned `engines.vscode` (`^1.85.0`) with `@types/vscode` (`^1.85.0`) for clean `vsce package` builds.
- Added `wgsl`, `msl`, `riscv64`, and `rust` to the `buildlang.target` configuration enum.
- Added Marketplace discovery metadata (`keywords`, `galleryBanner`, `homepage`, `bugs`, `repository.directory`) and updated `README.md` with 1-command `install.ps1` / `install.sh` and `cargo install buildlang` setup.

## 1.0.5

- Initial full VS Code extension with `source.bld` TextMate grammar, 19 snippets, bracket matching, and `buildc lsp` client launcher.
