# Model Capability Implementation Plan (slice 4 of the five-modes brief)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A model call enters the language as a typed foreign capability (`Model`), and the compiler enforces the propose/dispose split at the receipt boundary: a program that observes `Model` can never emit a scientific receipt, because models propose and oracles dispose.

**Architecture:** One builtin, `model_complete(prompt) -> str`, carrying a new `Model` capability effect. Transport is a deliberately dumb line protocol over the existing TCP runtime (endpoint from `BUILD_MODEL_ENDPOINT`, fail closed when unset), because the model adapter belongs on the harness side of the seam, not in the compiler. The receipt layer gains one admission rule with its own failure class (`CAPABILITY_INADMISSIBLE`): emit refuses a Model-observing program up front, and verify refuses any receipt whose RE-DERIVED capabilities include `Model`. The fail-closed witnessed-absence machinery already treats an unrecognized capability as a hazard for both dataset and determinism claims, which a unit test pins for `Model` explicitly.

**Honest scope, stated:** v0 ships the capability, the transport contract, and the type-level propose/dispose rule. It does NOT ship model receipts (model digest, prompt hash, parameters): those belong to the harness-side boundary receipt, and the flywheel demo (model proposes, a buildc-verified kernel disposes) is a separate follow-on gated on the operator's machine state.

## Global constraints

- ONE commit on branch `feat/model-capability` (already created and checked out). Do not push. Do not touch main.
- Every new gate mutation-tested: break it, watch the covering test fail, restore, watch green. Record red/green evidence in your report.
- No em-dash characters anywhere. No local drive paths in docs. ASCII only in .bld files.
- Full suite green with the exit code captured BEFORE any pipe. `cargo fmt --check` clean before commit.
- Never `git checkout` a working-tree file to undo a mutation; restore by inverse edit.
- The corpus and the verifier self-test are UNTOUCHED by this slice (Model programs cannot emit receipts, so there is no corpus member and no tamper case; say so in the docs).
- The two reference commits for the shipped slice pattern are in `git log`: "feat: Random capability with witnessed-seed scientific receipts" and "feat: Monte Carlo estimator receipts (five-modes slice 2)". Registration-point structure (capabilities.rs, infer.rs, lower/expr.rs, runtime.rs) mirrors the Random commit exactly.

### Task 1: the capability and the builtin registration

- [ ] compiler/src/types/capabilities.rs: `pub const MODEL: &str = "Model";`, added to `CAPABILITY_EFFECTS`; mapping arm before the vk block:

```rust
        // The model-call builtin. A foreign boundary crossing with its own
        // capability (not Foreign) because the receipt layer treats it
        // distinctly: a Model-observing program is INADMISSIBLE on the
        // receipt path outright. Models propose; oracles dispose.
        "model_complete" | "build_model_complete" => Some(MODEL),
```

  Tests: `capability_effect_for_call("model_complete")` and `("build_model_complete")` both `Some("Model")`; `capability_effect_names().contains(&"Model")`.
- [ ] compiler/src/types/infer.rs: builtin signature arm `"model_complete" => (vec![str_ty.clone()], str_ty.clone()),` beside the `random_f64` arm; and `"model_complete" |` added to the bare-identifier is_builtin list beside `"random_f64" |`.
- [ ] compiler/src/codegen/lower/expr.rs: `"model_complete" => return MirType::Struct(Arc::from("BuildString")),` beside the `random_f64` arm.
- [ ] compiler/src/codegen/runtime.rs: `"model_complete"` appended to `MATH_BUILTINS` (update the pinned length test 97 -> 98 and add a `contains` assert); `"model_complete" => Some("build_model_complete"),` in `math_builtin_to_c` (add the lookup assert beside random_f64's); the C implementation after `build_random_f64`:

```c
// --- Model-call builtin (Model capability) ---
//
// A deliberately DUMB line protocol over TCP: connect to the host:port in
// BUILD_MODEL_ENDPOINT, send the prompt bytes plus a single '\n', read one
// '\n'-terminated line back (or to connection close), return it without the
// terminator. The model adapter (HTTP, tokenization, parameters) lives on
// the harness side of this seam, never in the compiler. FAIL CLOSED: no
// endpoint, a malformed endpoint, an embedded newline in the prompt, or a
// connection failure aborts rather than fabricating a completion.
static BuildString build_model_complete(const char* prompt) {
    const char* endpoint = getenv("BUILD_MODEL_ENDPOINT");
    char host[256];
    const char* colon;
    int64_t port;
    int64_t sock;
    size_t i;
    if (endpoint == NULL || endpoint[0] == '\0') {
        fprintf(stderr, "model_complete: no endpoint provided; set BUILD_MODEL_ENDPOINT to host:port (the harness-side shim speaks one prompt line out, one completion line back)\n");
        exit(102);
    }
    colon = strrchr(endpoint, ':');
    if (colon == NULL || colon == endpoint || colon[1] == '\0' || (size_t)(colon - endpoint) >= sizeof(host)) {
        fprintf(stderr, "model_complete: BUILD_MODEL_ENDPOINT `%s` is not host:port\n", endpoint);
        exit(102);
    }
    memcpy(host, endpoint, (size_t)(colon - endpoint));
    host[colon - endpoint] = '\0';
    port = 0;
    for (i = 0; colon[1 + i] != '\0'; i++) {
        if (colon[1 + i] < '0' || colon[1 + i] > '9' || port > 65535) {
            fprintf(stderr, "model_complete: BUILD_MODEL_ENDPOINT port `%s` is not a valid port\n", colon + 1);
            exit(102);
        }
        port = port * 10 + (colon[1 + i] - '0');
    }
    if (port == 0 || port > 65535) {
        fprintf(stderr, "model_complete: BUILD_MODEL_ENDPOINT port out of range\n");
        exit(102);
    }
    for (i = 0; prompt[i] != '\0'; i++) {
        if (prompt[i] == '\n') {
            fprintf(stderr, "model_complete: the prompt may not contain a newline (the line protocol uses it as the terminator)\n");
            exit(102);
        }
    }
    sock = build_tcp_connect(host, port);
    if (sock < 0) {
        fprintf(stderr, "model_complete: could not connect to %s\n", endpoint);
        exit(102);
    }
    {
        BuildString line = build_string_from_cstr(prompt);
        BuildString newline = build_string_from_cstr("\n");
        BuildString reply;
        build_tcp_send(sock, line.data);
        build_tcp_send(sock, newline.data);
        reply = build_tcp_recv(sock);
        build_tcp_close(sock);
        /* Trim one trailing newline (and a carriage return before it). */
        if (reply.len > 0 && reply.data[reply.len - 1] == '\n') {
            reply.data[reply.len - 1] = '\0';
            reply.len -= 1;
        }
        if (reply.len > 0 && reply.data[reply.len - 1] == '\r') {
            reply.data[reply.len - 1] = '\0';
            reply.len -= 1;
        }
        return reply;
    }
}
```

  IMPORTANT: before writing this, read the ACTUAL signatures of `build_tcp_connect`, `build_tcp_send`, `build_tcp_recv`, `build_string_from_cstr`, and the `BuildString` struct fields in runtime.rs, and adapt the snippet to them exactly (argument types, whether send takes a char pointer or a BuildString, the real field names for data/len, and whether this block must appear AFTER the TCP section so the functions are already defined). The snippet is the design; the existing runtime is the authority on calling conventions. Also confirm `string.h` functions used (strrchr, memcpy) are available in the emitted C (the runtime already uses string functions; if not, use manual loops).
- [ ] Runtime tests: `test_runtime_header_contains_model` asserting the header contains `build_model_complete`, `BUILD_MODEL_ENDPOINT`, and `no endpoint provided`.

### Task 2: the receipt-boundary admission rule

- [ ] compiler/src/main.rs, `cmd_run` receipt path: directly after `derive_effect_policy` and BEFORE the seed gates:

```rust
    let uses_model = effect_policy
        .observed_capabilities
        .iter()
        .any(|cap| cap == "Model");
    if uses_model {
        eprintln!(
            "Error: this program observes the Model capability and cannot emit a scientific receipt: models propose, oracles dispose. Run the model as a proposer and verify its output with a model-free oracled kernel."
        );
        return Err(1);
    }
```

- [ ] compiler/src/scientific_runtime.rs, `evaluate_scientific_runtime_receipt`: directly after the EFFECT_POLICY_DRIFT re-derivation comparison (so it operates on RE-DERIVED capabilities, trusting nothing sealed), before the seed pairing gate:

```rust
    // The propose/dispose admission rule, enforced on the RE-DERIVED
    // capability union: a Model-observing program is inadmissible on the
    // accept path outright. Emit refuses to produce such a receipt; one
    // presented anyway (hand-built, or emitted by a tampered toolchain) is
    // refused with its own class rather than folded into a field contract.
    if rederived
        .effect_policy
        .observed_capabilities
        .iter()
        .any(|cap| cap == "Model")
    {
        eprintln!(
            "Error: the program observes the Model capability; a scientific receipt cannot witness a model-mediated run (models propose, oracles dispose)"
        );
        return Err(verify_failure_class(json, "CAPABILITY_INADMISSIBLE", 1));
    }
```

  NOTE: `rederived_uses_random` is computed nearby; place this block so `rederived` is in scope and the ordering comment stays true.
- [ ] Add `CAPABILITY_INADMISSIBLE` to the failure-class vocabulary doc comment on `verify_failure_class` (one line: a capability the receipt layer refuses outright; today only `Model`).
- [ ] Unit tests in scientific_runtime.rs tests module:
  - `witnessed_fields_treat_model_as_a_hazard_for_both_claims`: `witnessed_fields_from_capabilities(&["Console".to_string(), "Model".to_string()], false, None)` yields dataset `POSSIBLE_UNWITNESSED` with grounds containing `Model`, and non-deterministic with grounds containing `Model` (this pins that the fail-closed default arm covers `Model`; if someone later adds an explicit `Model` arm that weakens either claim, this test goes red).
  - `verify_refuses_a_model_observing_receipt`: build a receipt via `base_inputs` with `effect_policy` whose `observed_capabilities` is `["Console", "Model"]` (facts digest `hex_digest('9')`), rederive closure returns the same policy, rerun closure panics ("a model-observing receipt must be refused before the re-run"); result `Err(1)`. Model this on `verify_enforces_the_seed_capability_pairing`'s structure.

### Task 3: the example and end-to-end tests

- [ ] `examples/model_propose.bld`:

```
// The Model capability: a model call is a typed foreign boundary crossing.
//
// model_complete() sends one prompt line to the shim at
// BUILD_MODEL_ENDPOINT (host:port) and returns one completion line. FAIL
// CLOSED: with no endpoint the first call aborts. And the propose/dispose
// rule is enforced by the compiler at the receipt boundary: this program
// can run, but `--emit-receipt` refuses it outright, because models
// propose and oracles dispose. A model's output becomes evidence only by
// passing through a model-free oracled kernel.

fn main() ~ Console + Model {
    let reply: str = model_complete("propose: 2 + 2 =");
    println!("{}", reply);
}
```

  Adjust the declared type of `reply` to whatever the language's string type is spelled as in existing examples (check an example using read_line or getenv; if `let reply = ...` without annotation is the house style, use that).
- [ ] compiler/tests/cli.rs `model_capability_is_fail_closed_and_inadmissible_on_the_receipt_path`:
  - Run `model_propose.bld` with NO `BUILD_MODEL_ENDPOINT` in the child env (explicitly remove it): assert non-zero exit and stderr containing `no endpoint provided`.
  - Spawn a `std::net::TcpListener` on 127.0.0.1 port 0 in a thread that accepts one connection, reads until it sees `\n`, asserts the received line equals the kernel's prompt, writes `four\n`, and closes. Run the kernel with `BUILD_MODEL_ENDPOINT=127.0.0.1:<port>`: assert exit 0 and stdout containing `four`.
  - `--emit-receipt` on the same kernel (any invariant): assert refusal with stderr containing `models propose, oracles dispose`.
  - Note: `buildc()` helper builds a Command; use `.env_remove("BUILD_MODEL_ENDPOINT")` / `.env("BUILD_MODEL_ENDPOINT", ...)` on it. The compiled program inherits the run child's env through `cmd_run`'s plain-run path untouched (only BUILD_RANDOM_SEED is scrubbed), so setting the var on the buildc process reaches the program; verify this by reading the plain-run env handling in cmd_run before relying on it, and if the run path scrubs more than BUILD_RANDOM_SEED, adapt.
- [ ] Effect-gate coverage: a second .bld fixture is NOT needed on disk: assert the type checker's gate by running `buildc check` on a temp file (written by the test into its temp dir) whose main lacks `~ Model` but calls `model_complete("x")`, asserting non-zero exit and stderr naming the Model effect (mirror however an existing test asserts an undeclared-capability failure; search cli.rs for an undeclared-effect test to imitate; if none exists, assert on the checker's standard undeclared-effect wording after observing it once manually).

### Task 4: docs + changelog, same commit

- [ ] docs/EFFECTS_GUIDE.md: capability table row: `| `Model` | `model_complete` (line-protocol shim at `BUILD_MODEL_ENDPOINT`; a Model-observing program cannot emit a scientific receipt: models propose, oracles dispose) |`.
- [ ] docs/SCIENTIFIC-RECEIPT.md: a short admission paragraph in the honest-scope section (section "Honest scope" or directly after the capability-derived fields bullet): the Model capability exists in the language, and the receipt layer refuses it outright (`CAPABILITY_INADMISSIBLE`); the failure-classes table gains the row; state explicitly that there is no corpus member and no self-test case because the refusal happens before a receipt can exist.
- [ ] CHANGELOG.md: new bullet at the top of `## Unreleased`: the capability, the line-protocol seam (adapter lives harness-side), the fail-closed endpoint rule, and the propose/dispose enforcement at both emit and verify with the new failure class.

### Task 5: verification gate (record in your report)

- [ ] Full suite exit 0 (captured before any pipe); corpus stays 26/26 (run it: your change must not disturb it); self-test stays 8/8 on a budgeted receipt.
- [ ] Mutation checks: (a) break the emit refusal (make `uses_model` always false), watch the cli receipt-refusal assert fail, restore, green; (b) break the verify admission rule (condition always false), watch `verify_refuses_a_model_observing_receipt` fail, restore, green.
- [ ] Manual smoke: the TCP shim test IS the live smoke; confirm its listener assert on the received prompt line passed (this proves the wire format, not just the exit code).
- [ ] `cargo fmt` + `cargo fmt --check` clean; ONE commit on `feat/model-capability`, subject `feat: Model capability with the propose/dispose receipt boundary (five-modes slice 4)`, body in the shipped slice style, ending with: Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>. Do NOT push.
