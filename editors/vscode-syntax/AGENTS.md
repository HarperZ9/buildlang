# AGENTS.md - BuildLang VS Code Extension

## Scope

This file applies to the `buildlang-vscode` repository. Root workspace
instructions still apply; this repo is a public editor-support package for
BuildLang.

## Product Boundary

This repository owns the VS Code extension surface: language registration,
syntax grammar wiring, language configuration, README, icon assets, changelog,
and packaged `.vsix` releases.

The compiler, language semantics, backend maturity, and semantic corpus live in
`HarperZ9/buildlang`. Keep this extension aligned with that repo, but do not
invent language behavior here.

Publishable surfaces:

- `package.json` - VS Code extension manifest.
- `language-configuration.json` - brackets, comments, indentation, and editor
  behavior.
- `syntaxes/buildlang.tmLanguage.json` - syntax highlighting grammar.
- `images/`, `README.md`, `CHANGELOG.md`, `AUTHORS.md`, `LICENSE`, and reviewed
  `.vsix` packages.

Keep local-only:

- `.env`, `.env.*`, `.warden-safe-cache/`, `node_modules/`, temporary build
  output, logs, and unpublished marketplace credentials.

## Editing Rules

- Keep grammar changes synchronized with the language repo's documented syntax.
- Do not publish marketplace tokens, publisher credentials, or local VS Code
  profile state.
- Validate JSON files after edits.
- If extension behavior grows beyond syntax/configuration, add explicit tests or
  a reproducible packaging check before shipping.

## Verification

For docs or manifest-boundary changes:

```powershell
python -m json.tool package.json
python -m json.tool language-configuration.json
python -m json.tool syntaxes/buildlang.tmLanguage.json
git diff --check
```

Before committing or pushing, scan changed files for credential-shaped content
and confirm `.env` remains ignored.
