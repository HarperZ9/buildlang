# G2 synthetic evaluation bundle

This source-checkout workflow exports deterministic **SYNTHETIC** inputs and
executes the Build Finance kernel in **PAPER ONLY / SIMULATED** mode. It does
not contact a provider, access an account or credential, create a live order,
or make a profitability claim. Model inference is disabled; the expected
model evidence is `ABSTAIN / MODEL_DISABLED`.

The exporter accepts only a missing or empty destination. Choose a fresh local
directory for every run.

## Export

From the Build Finance repository root:

```powershell
python scripts/export_g2_evaluation_bundle.py .artifacts/g2-evaluation
```

The command writes `g2-disk-fixture/`, `kernel-projection.json`, and a sorted
`SHA256SUMS`. It runs production capture, admission, the reviewed disk loader,
verified-input construction, and the offline kernel before sealing checksums.

## Inspect

```powershell
Get-ChildItem -Recurse .artifacts/g2-evaluation
Get-Content -Raw .artifacts/g2-evaluation/kernel-projection.json
Get-Content .artifacts/g2-evaluation/SHA256SUMS
```

The projection must report `SYNTHETIC`, `PAPER_ONLY`, `SIMULATED`, a `CLOSED`
closure, and two `ABSTAIN / MODEL_DISABLED` rows. These are deterministic
evaluation results, not trading performance or evidence of profitability.

## Replay

Export to a second fresh destination. The exporter performs the complete
positive replay and fails unless the synthetic run closes cleanly:

```powershell
python scripts/export_g2_evaluation_bundle.py .artifacts/g2-evaluation-replay
Compare-Object (Get-Content .artifacts/g2-evaluation/SHA256SUMS) (Get-Content .artifacts/g2-evaluation-replay/SHA256SUMS)
```

No `Compare-Object` output means the exported file hashes are identical.

## Verify

Verify strict checksum closure using the reusable source-checkout verifier. It
rejects malformed, unsafe, unsorted, duplicate, missing, or extra paths before
validating every digest. Manifest bytes and member hashes are consumed with
bounded streaming reads from retained no-follow descriptors; root, directory
prefix, and final-member identities must remain stable through open and read:

```powershell
python scripts/export_g2_evaluation_bundle.py --verify .artifacts/g2-evaluation
```

The source-only evaluation format is intentionally small: at most 64 filesystem
files including `SHA256SUMS`, 16 KiB of checksum-manifest bytes, 1 MiB per
sealed member, and 8 MiB total. The synthetic bundle has 42 sealed members plus
the checksum manifest. Exceeding any limit is a verification failure, not a
request to allocate or follow a replacement path.

The dedicated installable paper-core artifacts can be rebuilt and checked with:

```powershell
python scripts/build_paper_core_artifacts.py --out-dir .artifacts/paper-core/dist
python scripts/verify_live_paper_artifacts.py --wheel .artifacts/paper-core/dist/build_finance_paper_core-1.1.0-py3-none-any.whl --sdist .artifacts/paper-core/dist/build_finance_paper_core-1.1.0.tar.gz
```

## Cleanup

Delete only the explicit local export directories after inspection:

```powershell
Remove-Item -Recurse -Force .artifacts/g2-evaluation, .artifacts/g2-evaluation-replay
```

Cleanup does not authorize publishing, uploading, provider access, credential
use, account access, wallet or signer use, or live execution.
