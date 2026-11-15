# AGENTS.md - BuildLang TextMate Grammar

## Scope

This file applies to the `buildlang-tmLanguage` repository. Root workspace
instructions still apply; this repo is a public syntax-highlighting grammar for
BuildLang.

## Product Boundary

This repository owns the standalone TextMate grammar package used by
TextMate-compatible editors and downstream tooling such as GitHub Linguist.

Language semantics, compiler behavior, backend maturity, and semantic-corpus
receipts live in `HarperZ9/buildlang`. The VS Code extension package lives in
`HarperZ9/buildlang-vscode`. Keep this grammar aligned with those repositories,
but keep this repo focused on grammar, language configuration, samples, and
public documentation.

Publishable surfaces:

- `grammars/buildlang.tmLanguage.json` - TextMate grammar definition.
- `language-configuration.json` - comments, brackets, surrounding pairs, and
  editor behavior metadata.
- `samples/` - representative `.bld` files for language detection and
  highlighting checks.
- `README.md`, `AUTHORS.md`, `LICENSE`.

Keep local-only:

- `.env`, `.env.*`, `.warden-safe-cache/`, dependency folders, generated output,
  logs, and unpublished marketplace or maintainer credentials.

## Editing Rules

- Keep keyword and sample changes synchronized with the language repo's public
  syntax surface.
- Preserve valid JSON in grammar and language-configuration files.
- Do not add compiler implementation details or unreleased semantic claims here
  unless the language repo already documents them.
- If grammar behavior changes materially, add or update a sample that exercises
  the changed construct.

## Verification

For docs or grammar-boundary changes:

```powershell
python -m json.tool grammars/buildlang.tmLanguage.json
python -m json.tool language-configuration.json
git diff --check
```

Before committing or pushing, scan changed files for credential-shaped content
and confirm `.env` remains ignored.
