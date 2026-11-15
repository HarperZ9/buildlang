# Contributing

This repository owns the standalone TextMate grammar for BuildLang.

Good changes are focused and easy to verify:

- grammar fixes that reflect public BuildLang syntax,
- representative sample updates for language detection,
- documentation corrections.

Before opening a pull request, run:

```bash
python -m json.tool grammars/buildlang.tmLanguage.json
python -m json.tool language-configuration.json
git diff --check
```
