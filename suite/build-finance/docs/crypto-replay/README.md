# Crypto replay T01-T03 offline evidence

This directory documents the offline `build_finance.crypto_replay` prerequisite island.

Current scope is deliberately narrow:

- T01 canonical JSON, primary contract schemas, primary bundle, and synthetic contract vectors.
- T02 closed supporting/attachment schemas, source-tree identity, run-input reconstruction, and schema-lock evidence.
- T03 explicit local-fixture capture, pure Jupiter fixture parsing, source-admission receipts, and replay subpackage confinement tests.

It does not implement T04 or any live/paper engine behavior. It has no live sensor, provider call, model call, feature generation, risk decisioning, fill simulation beyond existing contract schemas, portfolio mutation, broker integration, wallet/signing behavior, trading execution, real fixture admission, profitability evidence, or publication approval.

## Install and import

The package metadata keeps the replay runtime inside the existing `build-finance` distribution. Core package dependencies remain the existing numerical stack:

```powershell
python -m pip install .
python -I -c "import build_finance.crypto_replay"
```

`jsonschema` is test/dev-only. Importing `build_finance.crypto_replay` submodules must not require provider, broker, wallet, network, or test-only dependencies.

## Local fixture contract

T03 accepts only an explicit local `Path` passed to `capture_local_fixture(root)`.

- No default path, environment lookup, URL, socket, subprocess, provider SDK, broker, wallet, or signer is used.
- Capture preserves exact bytes for manifests, rights manifests, terms records, witnesses, and payloads.
- Admission emits total `SourceAdmissionReceipt` records and non-authoritative `ParsedSourceCandidate` values.
- Admission never emits `RawEvent`, order intents, fills, strategy signals, positions, or trading authority.
- Synthetic fixtures are test-only and do not satisfy P2 promotion.

## Gate commands

```powershell
python -m build_finance.crypto_replay.schema_codegen --scope full --check
python -m pytest -p no:cacheprovider tests/crypto_replay -q
python -m pytest -p no:cacheprovider tests/crypto_replay/test_t03_confinement.py -q
```

Task 7 package verification builds one fresh wheel and one fresh sdist, then inspects those exact artifacts:

```powershell
$artifact = Join-Path $env:TEMP ("bf-crypto-replay-" + (git rev-parse --short=12 HEAD))
New-Item -ItemType Directory -Path "$artifact\dist" | Out-Null
$env:PIP_NO_INDEX = '1'
$env:PIP_DISABLE_PIP_VERSION_CHECK = '1'
python -m build --no-isolation --outdir "$artifact\dist"
$wheels = @(Get-ChildItem "$artifact\dist\*.whl")
$sdists = @(Get-ChildItem "$artifact\dist\*.tar.gz")
python scripts/verify_crypto_replay_artifacts.py --wheel $wheels[0].FullName --sdist $sdists[0].FullName
```

Then install the same wheel into a clean venv with `--no-index --no-deps` and import every `build_finance.crypto_replay` submodule.

## Promotion state

See `promotion-status.json`. Current machine state is intentionally blocked:

- P0: `BLOCKED`
- P1: `PASS`
- P2: `FAIL_ZERO_ADMITTED_FIXTURE`
- P5: `PASS` for the offline whole-repository software gate; see `evidence/P5-whole-repository-pass.json`.
- replay subpackage confinement: `PASS`
- real fixture manifest: `null`
- next authorized node: `null`

## Troubleshooting

- If schema generation fails, run `python -m build_finance.crypto_replay.schema_codegen --scope full --check` first and inspect the generated-resource diff.
- If package verification fails, rebuild one fresh artifact pair and run `scripts/verify_crypto_replay_artifacts.py` against the exact wheel/sdist paths printed by the build step.
- If clean-wheel imports fail, confirm the wheel includes `build_finance/crypto_replay/resources/**` and that no replay module imports broker, provider, wallet, network, GUI, or test-only packages.
