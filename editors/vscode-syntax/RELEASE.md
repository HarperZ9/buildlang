# Release Checklist

## 0.1.0 Candidate

- [ ] Confirm `README.md`, `LICENSE`, `AUTHORS.md`, `CONTRIBUTING.md`, and
  `CHANGELOG.md` are present.
- [ ] Run `python -m json.tool package.json`.
- [ ] Run `python -m json.tool language-configuration.json`.
- [ ] Run `python -m json.tool syntaxes/buildlang.tmLanguage.json`.
- [ ] Run `npm pack --dry-run`.
- [ ] Run `npx --yes @vscode/vsce@3.9.2 package --no-dependencies`.
- [ ] Run `public-surface-sweeper . --summary`.
- [ ] Create a signed `v0.1.0` tag when signing is configured, or an
  annotated `v0.1.0` tag when it is not.
- [ ] Publish to the VS Code Marketplace only after publisher credentials and
  token handling are configured outside the repository.

This repository does not auto-publish to a package registry or marketplace.
