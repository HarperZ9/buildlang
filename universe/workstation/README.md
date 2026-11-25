# BuildLang Workstation Dogfood Modules

Two compiled BuildLang programs verifying Layer 1 deterministic parallel kernels and Layer 3 native workstation MCP governance directly on `buildc 1.15.0`:

- `kcore_fast.bld`: Superframe/RAW FNV-1a 64-bit hash (`1469598103934665603`), `COL-001` sRGB transfer + inverse, `OPT-002` dielectric Fresnel with TIR, planted-failure controls, and a `1,048,576`-point `par_for` bit-exact benchmark timed via `clock_ns()`.
- `workstation_mcp.bld`: Pure-BuildLang MCP JSON-RPC 2.0 stdio server (`~ Console + FileSystem + Clock + Environment`, zero Network/Process/Foreign authority) implementing `initialize`, `ping`, `tools/list`, `workstation_health`, and `check_em_dash_and_secret`, with `--self-test` known-answer and planted-failure controls.

## Run

```bash
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/kcore_fast.bld
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/workstation_mcp.bld -- --self-test
```
