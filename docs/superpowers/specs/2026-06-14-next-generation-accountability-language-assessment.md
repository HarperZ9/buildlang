# Next-Generation Accountability Language Assessment

Date: 2026-06-14
Status: Assessment complete; implementation design requires approval

## Purpose

BuildLang needs a reason to exist that is stronger than "another systems
language with several backends." The current compiler has a working C execution
path, an effect system, capability gates, policy profiles, and source/input
receipts. That combination points to a sharper product identity:

BuildLang should become an accountability language for operational software.
It should make power visible, typed, reviewable, policy-checkable, and
receipt-backed before code reaches execution.

This assessment records the next strategic slice for that direction.

## Current Evidence

- Local repository state: `main` is aligned with `origin/main` after
  `fb83b5e` (`feat: add input graph digest to check receipts`). Confidence:
  high, verified with `git status -sb` and `git log`.
- GitHub mainline CI and Pages for `fb83b5e2abd0424168f1e9785fdc1afcf0625c36`
  completed successfully. Confidence: high, verified with `gh run list`.
- There are no open GitHub issues. Confidence: high, verified with
  `gh issue list`.
- One open Dependabot PR exists and its checks are green, but it is not the
  strategic compiler lane. Confidence: high, verified with `gh pr list`.
- `buildc check` currently supports typed capability effects, check receipts,
  policy profiles, per-input SHA-256 ledgers, and an `input_graph_digest`.
  Confidence: high, verified in `README.md`, `docs/EFFECTS_GUIDE.md`,
  `compiler/src/main.rs`, `compiler/src/types/capabilities.rs`, and
  `compiler/tests/cli.rs`.

## Strategic Reading

The repository has two strong but competing narratives:

1. Graphics language: shader targets, SPIR-V, HLSL, GLSL, Vulkan-facing
   ambition, color-space typing, and render pipeline roadmaps.
2. Accountability language: typed effects, capability gates, receipts, policy
   profiles, input graph digests, and CI-oriented evidence.

The graphics lane can produce impressive demonstrations, but it competes with
established shader tooling. The accountability lane is more differentiated:
few languages can prove, in a portable receipt, which operational capabilities a
program declared, which ambient surfaces it directly touched, which source graph
was checked, and which policy accepted or rejected it.

The next step should deepen that differentiator instead of adding another
backend-adjacent feature.

## Alternatives Considered

### Approach A: SPIR-V and Vulkan Proof

Implement more MIR-to-SPIR-V coverage and host runtime proof work.

Tradeoff: high demo value and aligned with older roadmaps, but it does not
strengthen the fresh capability/security posture. It also risks becoming a
backend project before the language has a crisp adoption promise.

### Approach B: Standard Library and Real App Ports

Port more practical programs and grow the standard library from use.

Tradeoff: important for eventual adoption, but too broad for the current slice.
It creates surface area before the language has a firm operational contract
model.

### Approach C: Effect Provenance Contracts

Extend the current capability receipt and policy system so it distinguishes
direct ambient access from propagated effect obligations through the call graph.

Tradeoff: less visually flashy, but it turns existing features into a stronger
language promise. It also creates a foundation for CI, release candidates,
package review, and regulated operational code.

Recommendation: Approach C.

## Recommended Slice: Effect Provenance Contracts

Today a receipt can say:

```json
{
  "declared_effects": {
    "main": ["FileSystem"]
  },
  "observed_capabilities": {
    "load_config": {
      "FileSystem": ["read_file"]
    }
  }
}
```

This is useful, but incomplete. A practical policy needs to know whether
`main` directly touched the filesystem or merely propagated `FileSystem` because
it called `load_config`.

The next slice should add a provenance layer:

```json
{
  "propagated_effects": {
    "main": {
      "FileSystem": ["load_config"]
    }
  }
}
```

This lets policies say:

- direct `FileSystem` access is allowed only in `load_config`;
- `main` may propagate `FileSystem` only through approved functions;
- `Network` and `Foreign` are denied everywhere;
- every accepted receipt must include `source_digest`, `input_digests`, and
  `input_graph_digest`.

## Design Shape

The implementation should stay close to the existing checker flow:

1. `TypeInfer` already records direct capability sources as
   `observed_capabilities`.
2. When `TypeInfer` sees a call to a known Build function whose signature has
   effects, it should record a propagated-effect source:
   effect name -> callee function name.
3. `FunctionEffectSummary` should carry both direct observed capabilities and
   propagated effect sources.
4. `CheckReceipt` should serialize `propagated_effects` beside
   `observed_capabilities`.
5. `CheckPolicyProfile` should gain optional rules for direct and propagated
   effect policy while preserving existing `allowed_effects` and
   `denied_effects`.

The first implementation should be conservative. It does not need whole-program
interprocedural graph solving. It only needs to distinguish:

- direct ambient source: `read_file`, `println!`, `tcp_connect`, `extern` call;
- local propagation source: calling a known function whose signature declares
  an effect.

## Policy Extension Sketch

The current schema can remain `buildlang-check-policy/v1` if fields are
additive and optional:

```json
{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console", "FileSystem"],
  "denied_effects": ["Network", "Foreign"],
  "direct_effect_allowlist": {
    "FileSystem": ["load_config"],
    "Console": ["main", "report"]
  },
  "propagated_effect_allowlist": {
    "FileSystem": ["main", "run"]
  },
  "require_input_graph_digest": true
}
```

Policy behavior:

- `denied_effects` still wins globally.
- `allowed_effects` still bounds all declared, observed, and propagated
  effects when non-empty.
- `direct_effect_allowlist` applies only to `observed_capabilities`.
- `propagated_effect_allowlist` applies only to `propagated_effects`.
- `require_input_graph_digest` fails if the receipt lacks a SHA-256
  `input_graph_digest`.

## Acceptance Criteria

The slice is acceptable when:

- a function that directly calls `read_file` records `FileSystem` under
  `observed_capabilities`;
- a caller of that function records `FileSystem` under `propagated_effects`;
- receipts remain deterministic and backward-compatible;
- direct-only policy can reject a direct filesystem call in `main`;
- propagation policy can allow `main` to call `load_config` without allowing
  direct filesystem calls in `main`;
- violations name the effect, function, surface, and direct call or callee that
  triggered the decision;
- existing capability, policy, receipt, full compiler, warning-clean compiler,
  docs, diff hygiene, and secret-scan gates pass.

## Non-Goals

- No OS sandboxing.
- No runtime mediation.
- No network policy fetching.
- No cryptographic signature chain.
- No global call graph fixed-point solver in the first slice.
- No change to code generation.
- No claim that experimental backends enforce the same runtime behavior until
  backend-specific evidence exists.

## Next Work

If this assessment is accepted, write the implementation design as:

`docs/superpowers/specs/2026-06-14-effect-provenance-contracts-design.md`

Then write a TDD implementation plan under:

`docs/superpowers/plans/2026-06-14-effect-provenance-contracts.md`
