# Contributing

QuantaLang is a public compiler and language-surface repository. Contributions
should be focused, reproducible, and clear about backend maturity.

Good contribution shapes:

- parser, typechecker, MIR, CLI, or backend fixes with targeted tests,
- semantic-corpus examples with expected output and receipt coverage,
- documentation corrections that match implemented compiler behavior,
- editor or packaging updates aligned with the public language syntax.

Before opening a pull request, run the smallest verification slice that covers
the change. For release-readiness or test-count changes, run:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml --quiet
git diff --check
```

Keep private corpora, credentials, local logs, unpublished experiments, and
protected operational material out of this repository.
