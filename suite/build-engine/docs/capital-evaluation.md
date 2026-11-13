# G2 Capital Replay Evaluation

`build-engine capital-replay` is an evidence-only product shell for the
reviewed Build Finance G2 disk envelope. The workflow is local, deterministic,
PAPER ONLY, SIMULATED, and model-disabled. Its synthetic output is not a
profitability claim and does not authorize publication, live readiness, or any
live order.

## Boundary

The command accepts one explicit local fixture root. Build Engine lazily loads
`build_finance.paper_core_loader.load_replay_envelope_context`, while Build
Finance remains authoritative for fixture admission, run-input verification,
the offline paper kernel, accounting, reconciliation, and closure. Build
Engine neither reconstructs missing authority nor imports the legacy broker,
`AutoTrader`, provider, account, wallet, signer, credential, or live-mode
paths.

The successful G2 evaluation contract is:

- data classification: `SYNTHETIC`
- mode: `PAPER_ONLY`
- execution: `SIMULATED`
- closure: `CLOSED`
- model inference: `DISABLED_ABSTAIN`
- each model summary: `ABSTAIN / MODEL_DISABLED`
- profitability claim: `false`

## Install the reviewed local artifact

Build Finance paper core `1.1.x` is the only compatible optional dependency.
Install the already-built local wheel without contacting a package index, then
install the current Build Engine checkout:

```powershell
python -m pip install --no-deps path/to/build-finance/.artifacts/paper-core/dist/build_finance_paper_core-1.1.0-py3-none-any.whl
python -m pip install --no-deps -e "path/to/build-engine[capital]"
```

The Build Engine `capital` extra pins
`build-finance-paper-core>=1.1.0,<1.2`. Installing from the local wheel first
satisfies that contract without a network lookup.

## Export and verify a synthetic bundle

The exporter refuses a non-empty destination and writes only deterministic
synthetic evidence. From the reviewed Build Finance source checkout:

```powershell
$Finance = "path/to/build-finance"
$Bundle = "path/to/clean/g2-evaluation"
python "$Finance/scripts/export_g2_evaluation_bundle.py" "$Bundle"
python "$Finance/scripts/export_g2_evaluation_bundle.py" --verify "$Bundle"
```

Verification must report `status: PASS`. `SHA256SUMS` is the complete sorted
checksum manifest for the exported files; `kernel-projection.json` is the
Build Finance reference projection.

## Run and retain an evaluator receipt

Create the receipt parent directory explicitly, then run the one-command demo:

```powershell
$Receipt = "path/to/output/evaluator-receipt.json"
New-Item -ItemType Directory -Force (Split-Path $Receipt) | Out-Null
build-engine capital-replay --fixture "$Bundle/g2-disk-fixture" --receipt-output "$Receipt"
```

Successful console output includes stable projection, closure, and retained
receipt IDs, a `CLOSED` status, and one exact
`ABSTAIN / MODEL_DISABLED` line per decision. The evaluator receipt contains
the same evidence summaries and excludes the absolute fixture path and other
machine-specific data.

The receipt uses canonical UTF-8 JSON with sorted keys, compact separators,
and one terminal LF. `evaluator_receipt_id` is the lowercase SHA-256 of the
same canonical object with that one self-ID field removed. Verify both the
canonical bytes and self-ID locally:

```powershell
@'
import hashlib
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = path.read_bytes()
receipt = json.loads(payload)
canonical = lambda value: json.dumps(
    value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
).encode("utf-8")
assert payload == canonical(receipt) + b"\n"
claimed = receipt.pop("evaluator_receipt_id")
assert claimed == hashlib.sha256(canonical(receipt)).hexdigest()
print(claimed)
'@ | python - "$Receipt"
```

To confirm path independence, export into two clean roots, run the command for
both fixtures, and compare the two receipt files with `Get-FileHash -Algorithm
SHA256`; hashes and bytes must match.

## Windows directory-metadata limitation

On Windows/NTFS, strict Build Finance admission or replay-envelope identity
checks can fail if the filesystem reports transiently different directory
metadata for the same root or path prefix. This has been observed even when
the exported file bytes still pass the bundle checksum verifier. The failed
replay remains failed: Build Engine performs no retry and does not weaken,
repair, or reinterpret the upstream filesystem trust decision.

Preserve the failed fixture root unchanged together with the complete command,
stderr, admission reason codes, filesystem details, and checksum-verification
output for diagnosis. Do not delete, rewrite, or silently replace that root.
Copying the same verified bytes into a fresh local root and replaying them may
be useful as a separate diagnostic run. A successful fresh-root diagnostic is
new evidence only; it does not accept, promote, or retroactively validate the
failed run or its original root.

## Expected fail-closed outcomes

- Missing or incompatible paper core: exits with
  `install with: pip install 'build-engine[capital]'` before fixture access.
- Missing, linked, malformed, non-canonical, changed, or incompatible fixture
  envelope: the Build Finance loader rejects the exact local evidence.
- Rejected source admission or an incomplete rooted evidence set: replay stops
  before kernel execution.
- Model bodies, a non-disabled model mode, or any model result other than exact
  `ABSTAIN / MODEL_DISABLED`: replay is rejected.
- Missing or unwritable receipt parent: exits with
  `unable to write evaluator receipt` after replay; no alternate path is
  inferred.

No failure mode falls back to a provider, repairs evidence, selects a default
run, accesses a credential/account/wallet, constructs a live broker, or places
an order. The evaluator receipt is non-authoritative derived evidence only.
