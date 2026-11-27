# Cross-Backend MIR Opcode, Cast, and Intrinsic Parity Matrix

This specification records the audited lowering of MIR binary operators, unary operators, casts, and math/bitcast intrinsics across BuildLang's six code generation targets: CPU `C` and the five GPU backends (`HLSL`, `GLSL`, `WGSL`, `MSL`, `SPIR-V`).

## 1. Fail-Closed Guarantee

Every backend lowers MIR operators via exhaustive matches with zero `"??"` placeholder strings, zero `_ => OpIAdd` catch-all opcodes, and zero silent `"0"` fallbacks on unsupported rvalues. Any MIR construct without a semantically sound lowering on a target fails closed at compile time with `CodegenError::Unsupported`.

## 2. Binary Operators (`BinOp`)

| MIR `BinOp` | `C` | `HLSL` | `GLSL` | `WGSL` | `MSL` | `SPIR-V` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `Add` / `Sub` / `Mul` (int) | Checked trap helpers (`exit 101`) | `+`, `-`, `*` | `+`, `-`, `*` | `+`, `-`, `*` | `+`, `-`, `*` | `OpIAdd`, `OpISub`, `OpIMul` |
| `Add` / `Sub` / `Mul` (float) | `+`, `-`, `*` | `+`, `-`, `*` | `+`, `-`, `*` | `+`, `-`, `*` | `+`, `-`, `*` | `OpFAdd`, `OpFSub`, `OpFMul` |
| `AddWrapping` / `SubWrapping` / `MulWrapping` | `bl_{i,u}{add,sub,mul}_wrapping_*` | `uint`: native; `int`: `asint((asuint(l) op asuint(r)))` | Native GLSL 450 wrap | Native WGSL wrap | `uint`: native; `int`: `static_cast<int>((static_cast<uint>(l) op static_cast<uint>(r)))` | `OpIAdd`, `OpISub`, `OpIMul` |
| `AddChecked` / `SubChecked` / `MulChecked` | `bl_{i,u}{add,sub,mul}_checked_*` | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) |
| `AddSaturating` / `SubSaturating` / `MulSaturating` | `bl_{i,u}{add,sub,mul}_saturating_*` | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) |
| `Div` (int / float) | `bl_{i,u}div_*` / `/` | `/` | `/` | `/` | `/` | `OpSDiv` / `OpUDiv` / `OpFDiv` |
| `Rem` (int) | `bl_{i,u}rem_*` | `%` | `%` | `%` | `%` | `OpSRem` / `OpUMod` |
| `Rem` (float, truncated) | `fmodf` / `fmod` | `fmod(l, r)` | `(l) - (r) * trunc((l) / (r))` | `%` (truncated per WGSL spec) | `fmod(l, r)` | `OpFRem` |
| `Pow` (float) | `build_det_pow(l, r)` | `pow(l, r)` | `pow(l, r)` | `pow(l, r)` | `pow(l, r)` | `GLSL.std.450 Pow (26)` |
| `Pow` (int) | `build_det_pow(l, r)` | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) | Fail-closed (`Unsupported`) |
| `BitAnd` / `BitOr` / `BitXor` | `&`, `|`, `^` | `&`, `|`, `^` | `&`, `|`, `^` | `&`, `|`, `^` | `&`, `|`, `^` | `OpBitwiseAnd`, `OpBitwiseOr`, `OpBitwiseXor` |
| `Shl` (masked `bits - 1`) | `l << ((r) & mask)` | `l << ((uint)(r) & mask)` | `l << (uint(r) & mask)` | `l << (u32(r) & mask)` | `l << (static_cast<uint>(r) & mask)` | `OpBitwiseAnd(r, mask)` + `OpShiftLeftLogical` |
| `Shr` (signed arith / unsigned logic, masked) | `l >> ((r) & mask)` | `l >> ((uint)(r) & mask)` | `l >> (uint(r) & mask)` | `l >> (u32(r) & mask)` | `l >> (static_cast<uint>(r) & mask)` | `OpBitwiseAnd(r, mask)` + `OpShiftRightArithmetic` / `OpShiftRightLogical` |
| `Eq` / `Ne` (`bool`) | `==`, `!=` | `==`, `!=` | `==`, `!=` | `==`, `!=` | `==`, `!=` | `OpLogicalEqual`, `OpLogicalNotEqual` |
| `Eq` / `Ne` (float, IEEE-754 `NaN != NaN`) | `==`, `!=` | `==`, `!=` | `==`, `!=` | `==`, `!=` | `==`, `!=` | `OpFOrdEqual`, `OpFUnordNotEqual` |
| `Lt` / `Le` / `Gt` / `Ge` | `<`, `<=`, `>`, `>=` | `<`, `<=`, `>`, `>=` | `<`, `<=`, `>`, `>=` | `<`, `<=`, `>`, `>=` | `<`, `<=`, `>`, `>=` | `Op{S,U,FOrd}{Less,Greater}Than{,Equal}` |

## 3. Unary Operators (`UnaryOp`) and Casts (`CastKind`)

| Construct | `C` | `HLSL` | `GLSL` | `WGSL` | `MSL` | `SPIR-V` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `UnaryOp::Neg` | `bl_ineg_*` / `-v` | `-v` | `-v` | `-v` | `-v` | `OpFNegate` / `OpSNegate` |
| `UnaryOp::Not` (`bool`) | `!v` | `!v` | `!v` | `!v` | `!v` | `OpLogicalNot` (`168`) |
| `UnaryOp::BitNot` (int) | `~v` | `~v` | `~v` | `~v` | `~v` | `OpNot` (`200`) |
| Same-width `i32 <-> u32` cast | `(uint32_t)(v)` | `(uint)(v)` | `uint(v)` | `u32(v)` | `static_cast<uint>(v)` | `OpBitcast` (never invalid same-width `OpSConvert`) |
| `bool -> int / float` cast | `(int32_t)(v)` | `(int)(v)` | `int(v)` | `i32(v)` | `static_cast<int>(v)` | `OpSelect` (`1` / `0`) |
| `int -> bool` cast | `(bool)(v)` | `(bool)(v)` | `bool(v)` | `bool(v)` | `static_cast<bool>(v)` | `OpINotEqual` (`v != 0`) |
| `Transmute` / `f32::to_bits` / `f32::from_bits` | C99 union bitcast / `build_f32_*_bits` | `asuint` / `asint` / `asfloat` | `floatBitsToUint` / `uintBitsToFloat` | `bitcast<u32>` / `bitcast<f32>` | `as_type<uint>` / `as_type<float>` | `OpBitcast` |

## 4. Math Intrinsics

All five GPU backends map `fabs`/`fabsf`, `sqrt`/`sqrtf`, `sin`/`sinf`, `cos`/`cosf`, `tan`/`tanf`, `exp`/`expf`, `exp2`/`exp2f`, `log`/`logf`, `log2`/`log2f`, `pow`/`powf`, `floor`/`floorf`, `ceil`/`ceilf`, `round`/`roundf`, `trunc`/`truncf`, `fma`/`fmaf`, `min`, `max`, and `clamp` consistently. In SPIR-V, `abs`, `min`, `max`, and `clamp` dispatch on scalar/element type so integer operands emit `SAbs` (`5`), `SMin` (`39`), `UMin` (`38`), `SMax` (`42`), `UMax` (`41`), `SClamp` (`45`), and `UClamp` (`44`) rather than float-only instructions.
