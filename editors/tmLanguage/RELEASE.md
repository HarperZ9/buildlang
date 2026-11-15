# Release Checklist

## 0.1.0 Candidate

- [ ] Confirm `README.md`, `LICENSE`, `AUTHORS.md`, `CONTRIBUTING.md`, and
  `CHANGELOG.md` are present.
- [ ] Run `python -m json.tool grammars/buildlang.tmLanguage.json`.
- [ ] Run `python -m json.tool language-configuration.json`.
- [ ] Build `dist/buildlang-tmLanguage-0.1.0.zip`.
- [ ] Run `public-surface-sweeper . --summary`.
- [ ] Create a signed `v0.1.0` tag when signing is configured, or an
  annotated `v0.1.0` tag when it is not.

This repository does not auto-publish to a package registry.
