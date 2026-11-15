# Contributing

This repository is the public VS Code extension surface for BuildLang syntax
highlighting and editor configuration.

Useful contributions are small and verifiable:

- grammar and language-configuration fixes that match the public BuildLang
  syntax surface,
- documentation corrections,
- packaging fixes that do not add marketplace credentials or private state.

Before opening a pull request, run:

```bash
python -m json.tool package.json
python -m json.tool language-configuration.json
python -m json.tool syntaxes/buildlang.tmLanguage.json
npm pack --dry-run
git diff --check
```
