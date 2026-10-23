# BuildLang diagnostic codes

Every error `buildc` reports carries a stable code. A code never changes meaning, and a retired code is never reused. `buildc explain CODE` prints a code's title; `buildc explain` prints this table.

`buildc check FILE --error-format=json` prints one JSON report on stdout. Its shape is `buildlang.diagnostics/v1`, described by [schemas/buildlang.diagnostics.v1.schema.json](schemas/buildlang.diagnostics.v1.schema.json). Each diagnostic has a code, a message, a span with 1-based lines and byte columns (or `null` when the error has no location), help lines, notes and suggestions.

Each suggestion carries an applicability level with the same four values as rustc: `MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders` and `Unspecified`. A suggestion that would give the code more authority, such as adding a capability to an effect row or opening an `unsafe` block, has `widens_authority: true` and is never `MachineApplicable`. An agent applying fixes on its own must not apply those without review.

A test in `compiler/src/diagnostics/tests.rs` fails if this table and the compiler disagree.

## Input, driver and policy

| Code | Meaning |
|---|---|
| BL0001 | the input file could not be read |
| BL0002 | an import, include or module file could not be loaded |
| BL0090 | the check policy profile rejected the program |

## Lexer

| Code | Meaning |
|---|---|
| BL0101 | unexpected character |
| BL0102 | unexpected end of file while lexing |
| BL0103 | unterminated string literal |
| BL0104 | unterminated character literal |
| BL0105 | unterminated byte string literal |
| BL0106 | unterminated raw string literal |
| BL0107 | empty character literal |
| BL0108 | character literal holds more than one codepoint |
| BL0109 | unknown escape sequence |
| BL0110 | invalid Unicode escape |
| BL0111 | Unicode escape missing its opening brace |
| BL0112 | Unicode escape missing its closing brace |
| BL0113 | Unicode escape has more than six digits |
| BL0114 | invalid Unicode codepoint |
| BL0115 | invalid hex escape |
| BL0116 | hex escape out of range |
| BL0117 | escape character at end of file |
| BL0118 | invalid numeric literal |
| BL0119 | integer literal too large |
| BL0120 | float literal out of range |
| BL0121 | exponent has no digits |
| BL0122 | radix prefix has no digits |
| BL0123 | digit not valid for the base |
| BL0124 | float literal with a non-decimal base |
| BL0125 | invalid numeric suffix |
| BL0126 | integer suffix on a float literal |
| BL0127 | float suffix on a non-decimal integer |
| BL0128 | numeric literal starts with an underscore |
| BL0129 | numeric literal ends with an underscore |
| BL0130 | consecutive underscores in a numeric literal |
| BL0131 | unterminated block comment |
| BL0132 | invalid documentation comment |
| BL0133 | invalid character in an identifier |
| BL0134 | expected an identifier after `r#` |
| BL0135 | name cannot be a raw identifier |
| BL0136 | expected a lifetime name |
| BL0137 | invalid lifetime name |
| BL0138 | mismatched DSL block delimiters |
| BL0139 | unterminated DSL block |
| BL0140 | unterminated format string |
| BL0141 | unclosed interpolation in a format string |
| BL0142 | empty interpolation in a format string |
| BL0143 | interpolation nested too deep |
| BL0144 | invalid format specifier |
| BL0145 | shebang not at the start of the file |
| BL0146 | invalid UTF-8 in source |
| BL0147 | NUL character in source |
| BL0148 | non-ASCII character in a byte literal |
| BL0199 | internal lexer error |

## Parser

| Code | Meaning |
|---|---|
| BL0201 | unexpected token |
| BL0202 | expected a different token |
| BL0203 | unexpected end of file while parsing |
| BL0204 | invalid expression |
| BL0205 | integer literal too large for the parser |
| BL0206 | invalid left-hand side of an assignment |
| BL0207 | missing operand |
| BL0208 | unclosed delimiter |
| BL0209 | mismatched delimiter |
| BL0210 | invalid statement |
| BL0211 | missing semicolon |
| BL0212 | invalid item declaration |
| BL0213 | duplicate modifier |
| BL0214 | conflicting modifiers |
| BL0215 | invalid type |
| BL0216 | expected a type |
| BL0217 | invalid unit annotation |
| BL0218 | invalid pattern |
| BL0219 | expected a pattern |
| BL0220 | rest pattern not last |
| BL0221 | invalid generic parameter |
| BL0222 | invalid where clause |
| BL0223 | invalid attribute |
| BL0224 | inner attribute not allowed here |
| BL0225 | invalid macro invocation |
| BL0226 | lexer error reported by the parser |
| BL0299 | internal parser error |

## Types, lookup, expressions, patterns, control flow, traits

| Code | Meaning |
|---|---|
| BL0301 | type mismatch |
| BL0302 | infinite type |
| BL0303 | mutability mismatch |
| BL0304 | array length mismatch |
| BL0305 | wrong number of arguments |
| BL0306 | unsafety mismatch |
| BL0307 | lifetime mismatch |
| BL0308 | ABI mismatch |
| BL0310 | undefined variable |
| BL0311 | undefined type |
| BL0312 | undefined function |
| BL0313 | undefined field |
| BL0314 | undefined method |
| BL0315 | undefined enum variant |
| BL0320 | value is not callable |
| BL0321 | no overload matches the argument types |
| BL0322 | ambiguous overloaded call |
| BL0323 | overloaded function used as a value |
| BL0324 | value cannot be indexed |
| BL0325 | value cannot be dereferenced |
| BL0326 | value cannot be used with `?` |
| BL0327 | value cannot be awaited |
| BL0328 | invalid binary operator for these types |
| BL0329 | invalid unary operator for this type |
| BL0330 | integer literal out of range for its type |
| BL0331 | integer overflow in a constant expression |
| BL0332 | invalid assignment target |
| BL0333 | assignment to an immutable variable |
| BL0340 | pattern type mismatch |
| BL0341 | refutable pattern in an irrefutable position |
| BL0342 | non-exhaustive patterns |
| BL0343 | non-exhaustive match: missing enum variants |
| BL0350 | `break` outside a loop |
| BL0351 | `continue` outside a loop |
| BL0352 | undefined loop label |
| BL0353 | `return` outside a function |
| BL0354 | return type mismatch |
| BL0360 | trait not implemented |
| BL0361 | ambiguous trait implementation |
| BL0362 | associated type not found in the trait |
| BL0363 | associated type not defined in the impl |
| BL0370 | wrong number of type arguments |
| BL0371 | trait bound not satisfied |
| BL0380 | duplicate definition |
| BL0381 | type annotation needed |
| BL0382 | unsafe operation outside an `unsafe` block |
| BL0383 | construct not supported yet |
| BL0384 | recursive type has infinite size |
| BL0398 | internal checker error |
| BL0399 | internal type error |

## Effects and capabilities

| Code | Meaning |
|---|---|
| BL0401 | unknown effect |
| BL0402 | effect performed but not declared |
| BL0403 | undeclared effect in the function body |
| BL0404 | unknown effect operation |
| BL0405 | handler missing an operation |

## Ownership, borrows and linear values

| Code | Meaning |
|---|---|
| BL0501 | mutable borrow while already borrowed |
| BL0502 | two mutable borrows at once |
| BL0503 | use of a moved value |
| BL0504 | reference to a local escapes its scope |
| BL0510 | linear value used after it was consumed |
| BL0511 | linear value borrowed after it was consumed |
| BL0512 | linear value moved out of a shared borrow |
| BL0513 | linear field in a non-linear type |
| BL0514 | linear value in an untracked position |

## Units, modules and macros

| Code | Meaning |
|---|---|
| BL0601 | unit mismatch |
| BL0602 | unit mismatch in an operation |
| BL0610 | module import cycle |
| BL0620 | formatting macro needs a string literal first |

## Backend

| Code | Meaning |
|---|---|
| BL0901 | the C backend cannot compile the program |
