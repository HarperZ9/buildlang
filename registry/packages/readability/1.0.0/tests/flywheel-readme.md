<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/flywheel/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/flywheel/main/docs/art/hero-light.svg" alt="flywheel: Run any model, keep a receipt you can recheck offline. Streamlines of fine lines spiral inward around a bright core." width="100%">
</picture>

# flywheel

Run AI tasks on any hosted or local model, check answers against the sources that decide them, and keep a record of every step.

```
python -m pip install flywheel-verify
flywheel gate
```

[![version: 1.9.0](https://img.shields.io/badge/version-1.9.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://pypi.org/project/flywheel-verify/)
[![CI](https://github.com/HarperZ9/flywheel/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/flywheel/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-FSL--1.1--MIT-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/flywheel/blob/main/LICENSE)
![python 3.11+](https://img.shields.io/badge/python-3.11%2B-e6e1d6?style=flat-square&labelColor=1a1712)

Docs: https://harperz9.github.io/flywheel/docs/

## What it does

Flywheel is a local harness for AI work.

- It puts any model, hosted or local, behind one OpenAI-compatible endpoint on
  your machine.
- `flywheel code` runs a coding agent in your terminal. By default it asks
  before it writes or runs anything.
- Before an agent runs a shell command, a capability check reads the command
  and blocks network egress and other risky actions. The reason goes back to
  the model.
- `flywheel check-output` checks the values in an answer against the source
  that decides them: a tax table, a formulary, a court calendar, a script you
  supply. It says which fields agree, which disagree and which nothing could
  confirm, and whether the answer may ship.
- Every step lands on a hash-chained ledger. A sealed result can be checked
  again later, offline, with the Python standard library alone.

It also runs as a Windows desktop app with an assistant called Rowan, as a
GitHub Action, and as an MCP host for a set of companion tools.

## First run

Python 3.11 or newer. The engine has no third-party runtime dependencies.

```text
$ python -m pip install flywheel-verify
$ flywheel gate
  verify: verdict=PASS, output_hash=93e4b6c6b7a05c82, attribution=CANDIDATE
  seal: envelope_hash=1993af18b980c95d, claim_hash=23450831b0b42121, path=gate_envelope.json
  rewitness: result=MATCH
verdict=PASS rewitness=MATCH
```

`flywheel gate` runs a built-in task, checks the result, seals it into
`artifacts/gate/gate_envelope.json` and checks the seal again. It needs no
model and no network.

Check an answer against its source, using the Form 1040 example in
[examples/output-validation](examples/output-validation/README.md):

```text
$ flywheel check-output --contract form-1040.contract.json --answer answer-as-filed.json --allow-commands
FAIL  1 of 2 fields confirmed
HOLD  blocked by: tax
  FAIL          tax: the value disagrees with irs-2025-tax-table-single
  PASS          taxable_income: the answer cites the return
```

The answer took its tax from the rate schedule. The form requires the tax
table, so the check holds it. Exit codes: 0 confirmed, 1 disagrees, 3 unchecked.

Start a coding session in the current folder, or the local gateway and its
browser shell on `http://127.0.0.1:8799`:

```text
$ flywheel code
$ flywheel up
```

`flywheel code` uses a local model server when one is running, or a provider
key you have set. `flywheel doctor` reports what is missing.

## How it compares

- **LiteLLM** and **OpenRouter** route requests to many models. Flywheel does
  too, and keeps a sealed record of each run on your machine.
- **Claude Code**, **Aider** and **OpenHands** are coding agents. `flywheel code`
  is one too, with a shell capability check before each command and a
  hash-chained ledger of every step.
- **Inspect AI**, **promptfoo** and **Guardrails AI** score or validate model
  output. `check-output` validates a value against a named authority and can
  emit the check as a Lean 4 file for a kernel to run.

Flywheel puts those pieces in one local tool. It has no evaluation task library
of its own and no hosted gateway.

## Limits

- `check-output` uses the authorities you supply. The finance, medicine and law
  packs ship field shapes and arithmetic, and no domain data.
- The capability check uses a hand-curated map of executables. An executable
  it has never seen is admitted and recorded as unknown.
- Request content goes to the model provider you pick, under its terms. With a
  local model it stays on your machine.
- The desktop app ships for Windows only.
- In the Windows app's installed-app check, 15 of 17 lanes reach the class
  the check expects for them. index needs Git to reach it, and the pinned
  telos version has not been through that check yet
  ([per-lane table](project-docs/lanes/LANES.md)).

## Install

```
pip install flywheel-verify
flywheel up
```

`flywheel-verify` is the PyPI name (the bare `flywheel` name is an unrelated
package). The installed command is `flywheel`.

One-line install into its own environment:
`curl -fsSL https://raw.githubusercontent.com/HarperZ9/flywheel/main/install.sh | sh`
on macOS and Linux, or
`irm https://raw.githubusercontent.com/HarperZ9/flywheel/main/install.ps1 | iex`
in PowerShell.

Homebrew (macOS, Linux) and Scoop (Windows), from the formula and manifest in
this repository:

```
brew tap HarperZ9/flywheel https://github.com/HarperZ9/flywheel
brew install flywheel
```

```
scoop bucket add flywheel https://github.com/HarperZ9/flywheel
scoop install flywheel
```

**Windows desktop app.** Download the installer from the
[latest release](https://github.com/HarperZ9/flywheel/releases/latest) and
check it against the attached checksums. It bundles the engine and needs no
Python. Its assistant, Rowan, turns a plain-words request into a task the app
runs and records; see [Meet Rowan](GETTING-STARTED.md#meet-rowan).

**Models.** No model download is required. Point Flywheel at any hosted
provider you hold a key for; the roster shows whether a key is present and
never prints it. Ollama works over HTTP with no extras. The published
[14B](https://huggingface.co/zaindanaharper/flywheel-local-coder-14b) and
[32B](https://huggingface.co/zaindanaharper/flywheel-local-coder-32b) weights
are separate downloads, and the local serving and training stack installs with
`pip install "flywheel-verify[local]"`. Receipt signing and egress monitoring
have their own extras (`[signing]`, `[monitor]`).

**Subscription sign-in.** `flywheel auth login <provider>` runs a stepwise
flow (PKCE where the provider documents it, the provider's own tool where it
does not) and stores the token in the OS credential store, where the router
picks it up. See [GETTING-STARTED.md](GETTING-STARTED.md).

**From source:**

```
git clone https://github.com/HarperZ9/flywheel.git
cd flywheel
pip install -e .
python scripts/run_harness_cli.py app --port 8799
```

**Elsewhere.** For the GitHub Action, see [docs/GITHUB-ACTION.md](docs/GITHUB-ACTION.md).
Inside another agent host, the
[Flywheel Evidence Task skill](plugins/flywheel-evidence-task/README.md)
installs on its own, with Codex and Claude plugin manifests.

## How a run works

<p align="center"><img src="docs/art/run-lifecycle.svg" alt="Eight stages: a task is routed to a model, the model asks for a tool, the capability check either allows it or refuses it with the reason returned to the model, the tool runs, a receipt is written, the ledger is sealed, and a later recheck reports match, changed, or unverifiable." width="100%"></p>

A task is routed to a model. When the model asks for a tool, the capability
check allows or refuses the request. A refusal goes back to the model with its
reason, and the model can try another route. The ledger keeps the request, the
refusal and the reason alongside what ran.

## What the capability check decides

<p align="center"><img src="docs/art/what-runs.svg" alt="Seven shell commands and what the capability check does with each of them. A dangerous word inside quotes is a printed string and runs, while the same word reached through a command substitution is found and blocked. A pipe records both of its capability classes even though one of them names the decision. An environment prefix does not hide the executable behind it. A command that will not parse is escalated to a person rather than admitted. The marked row is the honest gap: an executable the curated map has never seen is admitted, and written down as unknown." width="100%"></p>

The check reads a shell command the way a shell reads it, then names what the
command can do. A denied word counts only in the executable position, so quoted
text prints and a command substitution gets walked into. The map of names is
curated by hand, and an executable it has never seen is admitted and recorded
as unknown.

The [animated explainer](https://harperz9.github.io/repo-explainers/flywheel.html)
steps through seven real commands, then seals and rechecks a result from
`flywheel gate`. Its source is [docs/explainer/index.html](docs/explainer/index.html).

## Checking an answer before it ships

```
flywheel check-output --contract task.contract.json --answer answer.json --allow-commands
```

Exit 0 confirmed, exit 1 disagrees, exit 3 unchecked. The report also says
whether the answer may ship: `RELEASE`, `RELEASE_WITH_CAVEAT`, or `HOLD` with
the fields that blocked it. Inside a lane, a held answer does not accept.

Finance, medicine and law each ship a pack of field templates for values that
go wrong the same way: a dose the formulary bands, a deadline counted in
calendar days where the rule counts court days, an amount carried to two
decimals in a currency that has none.

```
flywheel packs medicine
```

The answer can arrive as a document, and the report can go out as one:

```
flywheel check-output --contract c.json --answer memo.md --report review.pdf
```

Markdown, LaTeX and PDF all carry an answer. The report is written as `.txt`,
`.md`, `.tex`, `.pdf` or `.json`, whichever the suffix names. The PDF comes out
byte-identical across runs.

`--lean Answer.lean --verify-lean` emits the check as a Lean 4 file and runs
the kernel on it. What the kernel settles becomes a theorem, what an outside
authority decided becomes a named axiom, and `#print axioms` lists what the
result depends on. A kernel refusal shows up in the exit code.

See [docs/OUTPUT-VALIDATION.md](docs/OUTPUT-VALIDATION.md) for the contract
format, the checker protocol and the retry loop,
[docs/PROOF-AND-FORMATS.md](docs/PROOF-AND-FORMATS.md) for document formats and
the Lean output, and [docs/CRITICAL-DOMAINS.md](docs/CRITICAL-DOMAINS.md) for
the packs.

## Watch

[![Flywheel: check an answer against its source: a film rendered by the v1.5.1 release](https://harperz9.github.io/media/releases/flywheel/v1.5.1/first-check/poster.jpg)](https://harperz9.github.io/media/releases/flywheel/v1.5.1/first-check/index.html)

**[Flywheel: check an answer against its source](https://harperz9.github.io/media/releases/flywheel/v1.5.1/first-check/index.html)** (28 s).

[![Which source decides: a film rendered by the v1.5.1 release](https://harperz9.github.io/media/releases/flywheel/v1.5.1/which-source/poster.jpg)](https://harperz9.github.io/media/releases/flywheel/v1.5.1/which-source/index.html)

**[Which source decides](https://harperz9.github.io/media/releases/flywheel/v1.5.1/which-source/index.html)** (55 s).

Video files: [check an answer against its source](https://github.com/HarperZ9/flywheel/releases/latest/download/first-check-540p.mp4) and [which source decides](https://github.com/HarperZ9/flywheel/releases/latest/download/which-source-540p.mp4), captioned and attached to each release.

## Benchmarks

<!-- benchmarks:begin generated by scripts/build_benchmark_page.py, do not edit -->
7 suites run with no model endpoint and no network, so you get these numbers
back on your own machine:

```bash
python scripts/run_offline_benchmarks.py
```

| suite | what it answers | headline |
| :-- | :-- | :-- |
| accountability | does an unaccountable system score badly here | dimensions 8; harness_overall 1.0; separation 0.99; strawman_overall 0.01 |
| governed-agent | does a workflow refuse an action above its tier | failed 0; mean_quality_score 0.542; pass_rate 1.0; passed 6; scenarios 6 |
| agent-recovery | does an injected fault recover without failing quietly | receipt_completeness 1.0; recovery_success_rate 1.0; scenarios 6; silent_failure_rate 0.0 |
| stateful-provider-swap | does state survive a provider swap | checks 10; pass_rate 1.0; passed True |
| source-mined | do the mined checks still hold against their datasets | cases 26; failed 0; metrics_asserted 170; pass_rate 1.0; passed 26 |
| paired-replication | did continued pretraining change general code completion | delta_points -0.0305; gains 9; p_exact 0.4049; regressions 14; tasks 164 |
| receipting-cost | what does it cost to keep the receipt | durability_share 0.8742; log_bytes_per_action 788.3; ms_per_witnessed_action 8.79; recheck_us_per_record 21.5; verdict MATCH |

The strawman, a system with no receipts, scores 1% on the same axes. A
benchmark that everything passes measures nothing.

Against 5 named peers (codex, cursor, claude code, hermes, omp): 48
capabilities, 48 witnessed in this repository by a check that runs every time
the matrix is read, and 10 that every peer was read on and none declares. 9
rows carry at least one peer surface nobody here has read, and a star is
withheld from every one of them, so the starred count moves up as the reading
is done and not before. The peer columns are dated readings of public
documentation and public source, not measurements taken here.

Full results, the matrix, and the measurements that were not taken:
[docs/BENCHMARKS.md](docs/BENCHMARKS.md).

One of those suites recomputes the project's only capability comparison, and it
is negative: continued pretraining on the workspace corpus moved general code
completion -3.05 percentage points over 164 tasks, p = 0.40. It is here because
a negative result published is worth more than a positive one withheld.

The arms benchmark is a separate instrument, retired on 2026-07-26. The arms
were not independent: the treatment's first attempt is the same call as the
baseline's only attempt, so the treatment cannot score lower and the difference
is not a comparison. The quantity measured is verified pass@k. The retired
table read verified inference 9/10 against single-shot 8/10, difference +0.100
with 95% CI [-0.236, +0.420], an interval that includes zero, and no capability
uplift is claimed.
<!-- benchmarks:end -->

## What is in this repo

<p align="center"><img src="docs/schematics/architecture.svg" alt="The browser shell, the command line, curl and MCP clients all reach one gateway on localhost, which routes to a local model or an external check and writes a receipt either way, escalating only what does not pass." width="100%"></p>

- **`harness/`** is the Python engine. It runs tasks, checks tool requests,
  writes run records, discovers companion tools, and serves the localhost API.
  The installed runtime uses only the Python standard library.
- **`desktop/`** is the Flutter client. It talks to the gateway over localhost
  and can launch the bundled engine on Windows without a separate Python
  install. Its assistant, **Rowan**, runs on the model you choose.
- **`site/`** is the browser fallback used in development and CI.

Run the native client from a checkout:

```
cd desktop
flutter run -d windows --release
```

From a checkout, `python -m harness.cli_entry app --port 8799` also serves the
browser fallback at `/site/index.html`. The desktop app serves its UI on
localhost only and contacts no external address to show it.

## Included tools

Flywheel can connect to fourteen companion tools. Each has a public repository:

| Tool | Repository | What it does |
| :-- | :-- | :-- |
| gather | [gather](https://github.com/HarperZ9/gather) | Collect research and record its sources. |
| crucible | [crucible](https://github.com/HarperZ9/crucible) | Recheck a claim and report a match, change, or missing evidence. |
| index | [index](https://github.com/HarperZ9/index) | Map files and symbols in a workspace. |
| forum | [forum](https://github.com/HarperZ9/forum) | Route work among models and record decisions. |
| learn | [learn](https://github.com/HarperZ9/learn) | Turn your material and recorded attempts into a study plan. |
| telos | [telos](https://github.com/HarperZ9/telos) | Reconcile findings from several tools. |
| local-model | [archived predecessor](https://github.com/HarperZ9/local-model) | Historical engine repository. Its runtime is now part of Flywheel; the lane name remains for compatibility. |
| relay | [relay](https://github.com/HarperZ9/relay) | Run a coding agent with a local or hosted model. |
| plexus | [plexus](https://github.com/HarperZ9/plexus) | Find installed tools and connect them. |
| mneme | [mneme](https://github.com/HarperZ9/mneme) | Store and retrieve memories with source checks. |
| calibrate-pro | [calibrate-pro](https://github.com/HarperZ9/calibrate-pro) | Check display calibration targets and readiness. |
| accountable-surface | [accountable-surface](https://github.com/HarperZ9/accountable-surface) | Require approval before actions and keep a tamper-evident record. |
| canon | [canon](https://github.com/HarperZ9/canon) | Keep one set of instructions and memories across the assistants you use, and see what each one would be given. |
| bulletin | [bulletin](https://github.com/HarperZ9/bulletin) | Reach an open message board where agents post, search, and reply under a signed identity. [Watch it live](https://harperz9.github.io/bulletin.html). |

List their configured state, or probe their live MCP connections:

```
flywheel lanes
flywheel lanes --probe    # live MCP handshake per lane
```

The bulletin board is live at <https://harperz9.github.io/bulletin.html> and
needs no key or account to read. Flywheel contacts no board until you choose
one: set `FLYWHEEL_BULLETIN_URL` to the board's MCP URL to use the Bulletin
lane, and `FLYWHEEL_BULLETIN_BASE_URL` to its HTTPS origin to register an
identity. Anyone can post there, so treat what you read as untrusted input.

## Run records

<p align="center"><img src="docs/schematics/verified-loop.svg" alt="A local model proposes, an external check disposes, and the pair is written to a content-addressed receipt; a pass goes to the proof cache and anything that does not pass escalates." width="100%"></p>

Routed runs keep a ledger of tool names, arguments and outputs. With sealed
tool-call receipts turned on, each call also records its capability
(`builtin-read`, `builtin-write`, `builtin-exec`, `external-mcp` or `unknown`),
its outcome, hashes of its arguments and output, and the hash of the previous
receipt. If one receipt in the chain is invalid, the entries after it read as
unverifiable.

## Recent additions

- **Signed citations.** `harness/grounding_signatures.py` reads an Ed25519
  sidecar beside a receipt and reports it absent, invalid or valid. An unsigned
  store behaves as before.
- **Scheduled runs.** A schedule runs when something asks for a tick; there is
  no daemon. Each schedule names its catch-up policy: fire every missed
  occurrence, fire the most recent one, or drop them. A schedule stops itself
  after two failed fires in a row, including a hook that exits 0 while its
  output ends on a rate-limit or quota error, and stays stopped until you press
  Re-arm on its row (`docs/RUN-BUDGET.md`).
- **Code scan coverage.** Beside the finding count, a scan records how many
  files it read out of how many exist, whether the ruleset still fires, and how
  many findings were suppressed.
- **Every route in the app.** A coverage check fails when a route the gateway
  serves has no way in from the desktop client. It reads 152 of 152 today.
- **Lessons from failures.** A proposed lesson carries hashes of its evidence
  and stays a proposal until a person accepts it. See
  [docs/LESSON-LOOP.md](docs/LESSON-LOOP.md).

## Documentation

- [QUICKSTART.md](QUICKSTART.md): first ten minutes
- [GETTING-STARTED.md](GETTING-STARTED.md): install, sign in, first run
- [WALKTHROUGH.md](WALKTHROUGH.md): guided tour
- [docs/FLYWHEEL-1.0.0-OVERVIEW.md](docs/FLYWHEEL-1.0.0-OVERVIEW.md): the full feature set and an install-to-first-verdict walkthrough
- [docs/features/](docs/features/README.md): per-feature docs and how the lanes fit together
- [docs/CONFIGURATION.md](docs/CONFIGURATION.md): settings
- [docs/OUTPUT-VALIDATION.md](docs/OUTPUT-VALIDATION.md): check an answer against the source that decides it
- [docs/PROOF-AND-FORMATS.md](docs/PROOF-AND-FORMATS.md): Markdown, LaTeX and PDF in and out, and the Lean output
- [docs/CRITICAL-DOMAINS.md](docs/CRITICAL-DOMAINS.md): the finance, medicine and law packs
- [docs/TRACE-OWNERSHIP.md](docs/TRACE-OWNERSHIP.md): where agent traces live, and status, encryption, export, deletion, import and retention
- [docs/trace-redaction.md](docs/trace-redaction.md): what trace redaction catches and misses
- [docs/WRAPPER-HOOKS.md](docs/WRAPPER-HOOKS.md): capture hooks for Claude Code and Codex
- [docs/CONTEXT-MEMORY.md](docs/CONTEXT-MEMORY.md): context and memory binding per owner and project
- [docs/native-cli-session-contract.md](docs/native-cli-session-contract.md): native CLI session profiles
- [docs/native-continuation.md](docs/native-continuation.md): continuation preview
- [docs/writing-workspace.md](docs/writing-workspace.md): the Writing Workspace and its MCP launcher
- [docs/REMOTE-ACCESS.md](docs/REMOTE-ACCESS.md): drive the same loop from your phone
- [docs/LESSON-LOOP.md](docs/LESSON-LOOP.md) and [docs/GUIDE-LESSON-LOOP.md](docs/GUIDE-LESSON-LOOP.md): the lesson loop, architecture and full guide
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md): full benchmark results and the peer matrix
- [docs/INSPECT-EVIDENCE.md](docs/INSPECT-EVIDENCE.md): Inspect JSON import (development)
- [docs/INCIDENT-SIM-EVALUATION.md](docs/INCIDENT-SIM-EVALUATION.md): incident traces and process-audit packets (development)
- [docs/CLIENT-AND-TOOL-ARCHITECTURE.md](docs/CLIENT-AND-TOOL-ARCHITECTURE.md): the desktop client, the engine and the companion tools
- [desktop/README.md](desktop/README.md): desktop development and packaging

## Development

Zain Dana Harper maintains this repository. AI-assisted tools are used for
parts of development and documentation.

## License

Text: CC BY 4.0. Code: FSL-1.1-MIT.

The essays in [`docs/essays/`](docs/essays/) and the research notes in
`docs/research/`, `project-docs/research/` and `tasks/research/` are licensed
CC BY 4.0. Share and adapt them with credit to Zain Dana Harper; the terms are
in [`LICENSE-TEXT`](LICENSE-TEXT). The code and the software documentation are
FSL-1.1-MIT (Functional Source License). See [LICENSE](LICENSE).
