# BuildLang Self-Host Compiler (Slice 3)

Standalone pure BuildLang compiler (`buildc_selfhost_s3.bld`) implementing a lexer, recursive-descent parser, static type checker with struct fields and enum match dispatch, and C99 emitter. The compiler compiles its own source code across Stage 0, Stage 1, Stage 2, and Stage 3 to an exact SHA-256 fixed point where the C output of Stage 2 matches the C output of Stage 3 byte for byte.

## Usage

Compile `buildc_selfhost_s3.bld` with `buildc build` in release mode, then invoke the produced executable with `<input.bld> <output.c>` to lower a BuildLang program to standalone C99.

## Verification

Run `buildc check` and `buildc lint` on `buildc_selfhost_s3.bld` to verify zero type errors and zero lint warnings. To verify the Stage 3 fixed point, compile `buildc_selfhost_s3.bld` with Stage 0 to produce `stage1.c`, compile `stage1.c` with a C99 compiler to produce Stage 1, repeat for Stage 2 and Stage 3, and confirm identical SHA-256 digests for `stage2.c` and `stage3.c`.

## License

Licensed under `AGPL-3.0-only`, matching the root repository `LICENSE`.
