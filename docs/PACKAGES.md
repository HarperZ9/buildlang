# BuildLang packages

`buildc pkg` creates packages, publishes them into the registry in this
repository, resolves versions, writes `Build.lock` with a content hash per
package, and builds offline from that lock. Nothing in the flow opens a
network connection: the registry is the `registry/` folder.

## Commands

| Command | What it does |
|---|---|
| `buildc pkg init [DIR] [--lib]` | Writes `Build.toml` and `src/main.bld` (or `src/lib.bld`). |
| `buildc pkg add NAME [--version REQ]` | Adds a registry dependency under `[dependencies]`; the default requirement is `^` the newest published version. |
| `buildc pkg publish [DIR]` | Copies the package into `registry/packages/NAME/VERSION/` and records its hash in `registry/index.json`. |
| `buildc pkg resolve [DIR]` | Picks a version of every dependency and writes `Build.lock`. |
| `buildc pkg verify [DIR]` | Re-hashes every locked package and compares it with the lock. |
| `buildc pkg build [DIR] [--emit c\|exe] [--release]` | Builds offline from `Build.lock` into `target/pkg/`. |
| `buildc pkg check-registry` | Re-hashes every published version against the index. |
| `buildc pkg search QUERY` | Lists packages whose name or description matches. |

Every command takes `--registry DIR`. Without it, `buildc` uses
`BUILDLANG_REGISTRY`, then the nearest `registry/index.json` above the
package folder.

## A package

```text
my-tool/
  Build.toml      name, version, license, [dependencies]
  Build.lock      written by `pkg resolve`
  LICENSE         the licence text, when it is not the repository's
  src/lib.bld     library: every src/*.bld except main.bld, joined in name order
  src/main.bld    the program, when the package is a tool
  tests/          inputs, goldens and tests/cases.txt
```

```toml
[package]
name = "merkle"
version = "1.0.0"
description = "RFC 6962 Merkle roots and proofs"
author = "Zain Dana Harper"
license = "FSL-1.1-MIT"

[dependencies]
sha256 = "^1.0"
```

A name is lowercase letters, digits and `-`. Publishing needs `license`.
Requirements take `^1.2`, `~1.2.3`, `=1.2.3`, `>=1.0, <2.0`, `1.*` and `*`.
Only registry dependencies are supported; a `path` or `git` dependency is
refused with BL0705.

## Content hash

A package is the files at the top of its folder plus everything under `src/`
and `tests/`; hidden files and `target/` are left out. Its hash is SHA-256
over each file in byte order of its relative path: the line
`file <path> <length>`, the bytes, then a newline. It is written
`sha256:<hex>`. `publish` writes text files outside `tests/` with LF
endings (test inputs keep their bytes), and
`.gitattributes` keeps git from rewriting `registry/`, so a clone on any
platform hashes the same bytes.

A published version is immutable. Publishing it again with the same files
does nothing; with other files it fails with BL0707.

## Resolution

Each round collects the requirements of the root and of every package picked
so far, and picks the newest published version that meets all of a name's
requirements. Rounds repeat until the picks stop changing. There is one
version per name, and no backtracking: when two requirements on one name have
no common version, resolution fails with BL0701 and names both requirements
and every published version.

## Build.lock

```toml
lock-version = 1

[root]
name = "app"
version = "0.1.0"
requires = ["merkle ^1.0.0"]

[[package]]
name = "merkle"
version = "1.0.0"
checksum = "sha256:..."
dependencies = ["sha256 1.0.0"]
```

`requires` repeats the manifest's requirements. When the manifest changes
after the lock was written, `build` and `verify` fail with BL0703 until
`resolve` runs again. The same resolution always writes the same bytes.

## Offline build

`buildc pkg build` reads only `Build.toml`, `Build.lock` and the registry
folder. It checks that every locked version is in the registry (BL0704) and
that its files hash to the locked checksum (BL0702). It then joins the
library files of each package in dependency order, the project's own library
files and `src/main.bld` into one source. Lines that import a locked package
by name (`use merkle;`) are dropped, because the joined source already holds
it. The C backend compiles that source.

Outputs in `target/pkg/`: `NAME.c`, the executable, and `build-receipt.json`
with the lock's hash, each package's checksum, the joined source's hash and
the C artifact's hash. The C artifact is the reproducible output: two clean
copies of the registry and a project give the same hash
(`compiler/tests/pkg_flow.rs`). The executable's hash is recorded too, but the
C compiler may stamp it, so it is not compared.

Generated programs use LF on stdout by default (`--stdio-mode portable-lf`).

Package sources share one namespace. Library functions carry a package
prefix (`merkle_`, `cr_`, `color_`) so two packages do not collide.

## Tool packages and their tests

A tool package carries `tests/cases.txt`. Each line is
`MODE GOLDEN ARGS...`: run the program with ARGS from the package folder and
compare stdout with GOLDEN. `exact` wants the same bytes. `float:ABS:REL`
compares whitespace-separated tokens, numbers within ABS or within REL of the
golden value. `tests/reference.py` in each package runs the original tool on
the same inputs and wrote the goldens. `compiler/tests/registry_packages.rs`
builds every such package from its own lock and runs every case.

## Kernel packages and the improvement loop

A kernel package is a tool package with a written numeric contract. `sum-det`
is the first: it adds binary64 values in a fixed order and returns the same
bits on every machine. Beside the usual files it carries `SPEC.md` and
`KERNEL.toml` (the contract in words and in machine-readable form),
`tests/controls.txt` (planted-wrong versions every check must refuse),
`tests/props.txt`, `ref/` (the reference that wrote the goldens), `bench/`
(the benchmark and its decision rule) and `RECEIPT.json`.

One source file of the package is a slot: a pure function with a slower
reference beside it. A faster version replaces the slot only through the loop
under `improve/`:

```
python improve/spawn.py --buildc buildc PROPOSAL...
```

The loop sends the planted controls and then each proposal through admission,
build, lint, `buildc tv`, the cases, the properties, a seeded fuzz run against
the reference, a paired benchmark of 20 rounds against a threshold fixed
beforehand, and a hold-out set. It appends every decision to
`improve/LEDGER.jsonl`, refusals included, and each line carries the digest of
the line before it. A run in which any control passes promotes nothing.

`improve/FROZEN.sha256` lists the digest of every file the loop may not change:
cases, goldens, controls, hold-out, the benchmark rule, the reference and the
loop itself. The loop refuses to start when a digest differs or a required
file is missing from the list. `compiler/tests/kernel_improve.rs` runs both
checks in CI, and plants a changed threshold, a dropped list entry, a removed
ledger line and an edited ledger line to confirm each is refused.

The loop's judge is written in BuildLang (`improve/kernel/`). BuildLang has no
builtin that starts a process, so a small Python script runs the commands the
judge prints. `SPEC.md` in the package lists what the loop does not cover.

## Codes

| Code | When |
|---|---|
| BL0701 | no published version meets every requirement |
| BL0702 | a locked package's files or index entry do not match the lock |
| BL0703 | `Build.lock` is missing, malformed or older than `Build.toml` |
| BL0704 | a locked version is not in the registry |
| BL0705 | the manifest is invalid, or a published package lacks `license` or sources |
| BL0706 | no registry was found, or its index is invalid |
| BL0707 | that version is already published with other files |
| BL0708 | a package file could not be read or written |

---

# Design draft (2026-03-21)

The rest of this file is the original design draft. Workspaces, features,
git and path dependencies, a remote registry and namespaced imports are not
built. The sections above describe what is.

Version: 0.1 (Draft)
Date: 2026-03-21

---

## 1. Package Manifest (Build.toml)

Every BuildLang project has a `Build.toml` at its root. This file declares
the package identity, dependencies, build targets, and features.

### 1.1 Full Manifest Format

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2026"
authors = ["Zain Harper <zain@example.com>"]
description = "A short description of the package"
license = "MIT"
repository = "https://github.com/user/my-project"

[dependencies]
json = "1.0"
http = { version = "0.2", features = ["tls"] }
local-lib = { path = "../local-lib" }
git-lib = { git = "https://github.com/user/lib.git", branch = "main" }

[dev-dependencies]
test-utils = "0.3"

[build-dependencies]
codegen-helper = "0.1"

[features]
default = ["std"]
std = []
serde = ["dep:serde"]
tls = ["http/tls"]

[[bin]]
name = "my-cli"
path = "src/main.bld"

[lib]
name = "my_lib"
path = "src/lib.bld"
```

### 1.2 What Already Exists in the Compiler

The `compiler/src/pkg/manifest.rs` module already implements:

- **`Manifest` struct** with fields for: `package` (name, version, authors,
  edition, description, license, repository), `dependencies`, `dev_dependencies`,
  `build_dependencies`, `features`, `default_features`, `workspace`, and target
  arrays (`bin`, `lib`, `example`, `test`, `bench`).
- **`Manifest::new()`** constructor with defaults.
- **`Manifest::from_str()`** and **`Manifest::from_file()`** for parsing TOML.
- **`Manifest::to_toml()`** for serialization back to TOML text.
- **`add_dependency()`** and **`add_dev_dependency()`** methods.
- **`Dependency` type** supporting version string, path, git, and feature lists.

This is a solid foundation. The manifest parser can already read and write the
full format shown above.

---

## 2. Module System

### 2.1 Import Syntax

```build
// Import from standard library
use std::collections::HashMap;
use std::io::Read;

// Import from a dependency declared in Build.toml
use json::Value;
use http::Client;

// Import from local file (sibling module)
mod my_module;
use my_module::MyType;

// Import from nested module directory
mod utils;
use utils::helpers::format_output;

// Glob import (all public items)
use std::collections::*;

// Aliased import
use json::Value as JsonValue;

// Multiple imports from same path
use std::collections::{HashMap, HashSet, BTreeMap};
```

### 2.2 Module Resolution Rules

When the compiler encounters `use foo::bar::Baz;`, it resolves the path as follows:

1. **`std::`** -- Standard library. Resolved from the compiler's built-in std
   implementation. Not a filesystem lookup.

2. **`<dep_name>::`** where `dep_name` is a key in `[dependencies]` -- External
   dependency. Resolved from the package cache (see Section 3).

3. **`<local_name>::`** where `local_name` matches a `mod <name>;` declaration --
   Local module. Resolved by filesystem convention:
   - `mod foo;` looks for `./foo.bld` or `./foo/mod.bld` relative to the
     current file.
   - `mod foo::bar;` looks for `./foo/bar.bld` or `./foo/bar/mod.bld`.

4. **`self::`** -- Current module. Relative import within the same file.

5. **`super::`** -- Parent module. One level up in the module hierarchy.

6. **`crate::`** -- Crate root. Absolute path from the package root (`src/lib.bld`
   or `src/main.bld`).

### 2.3 File Layout Convention

```
my-project/
  Build.toml
  Build.lock
  src/
    main.bld          # Binary entry point
    lib.bld           # Library root (if [lib] is defined)
    utils.bld         # Module: `mod utils;` or `use crate::utils`
    utils/
      mod.bld         # Alternative to utils.bld
      helpers.bld     # Sub-module: `use crate::utils::helpers`
  tests/
    integration.bld   # Integration tests
  examples/
    demo.bld          # Example programs
```

### 2.4 Visibility

```build
// Public (accessible from other modules)
pub fn public_function() { }
pub struct PublicStruct { }

// Private (default, accessible only within this module)
fn private_function() { }
struct PrivateStruct { }

// Public within the crate only (not exported to dependents)
pub(crate) fn crate_internal() { }

// Public within the parent module only
pub(super) fn parent_only() { }
```

---

## 3. Package Registry

### 3.1 Registry Design

- **URL:** `https://registry.buildlang.org` (future, already configured as
  default in `compiler/src/pkg/registry.rs`)
- **API:** RESTful JSON API for package metadata, tarball downloads for source
- **Index:** Git-based index (similar to crates.io) for fast metadata lookups
- **Authentication:** Token-based via `buildc login`

### 3.2 CLI Commands

```
buildc pkg init                    # Create Build.toml in current directory
buildc pkg add json@1.0            # Add dependency to Build.toml
buildc pkg add http --features tls # Add with features
buildc pkg remove json             # Remove dependency
buildc pkg update                  # Update all dependencies to latest compatible
buildc pkg update json             # Update specific dependency
buildc pkg publish                 # Publish package to registry
buildc pkg search "json parser"    # Search registry
buildc pkg info json               # Show package details
buildc pkg login                   # Authenticate with registry
buildc pkg yank 0.1.0              # Yank a published version
```

### 3.3 What Already Exists in the Compiler

The `compiler/src/pkg/registry.rs` module implements:

- **`RegistryConfig`** with url, auth token, cache directory, timeout, max
  concurrent downloads. Default URL is `https://registry.buildlang.org`.
- **`RegistryError`** enum covering: Network, NotFound, VersionNotFound,
  AuthRequired, AuthFailed, RateLimited, InvalidResponse, CacheError, Io,
  ChecksumMismatch, Yanked.
- **`PackageMetadata`** struct for registry responses.
- **Cross-platform cache directory** resolution (LOCALAPPDATA on Windows,
  XDG_CACHE_HOME on Linux).

The `compiler/src/pkg/resolver.rs` module implements:

- **`Resolver`** with PubGrub-style dependency resolution.
- **`ResolveError`** covering: NoMatchingVersion (with available versions),
  Conflict (with source requirements), Cycle (with package chain),
  FeatureNotFound, MaxIterations.
- **`ResolvedPackage`** with name, version, features, dependencies.
- **`Resolution`** with root package, all resolved packages, dependency graph.

The `compiler/src/pkg/lockfile.rs` module implements:

- **Lockfile format** (version 1) using `Build.lock` filename.
- **`LockfileError`** for IO, parse, version mismatch, and integrity errors.
- **BTreeMap-based** package storage for deterministic output.

The `compiler/src/pkg/version.rs` module implements:

- **`Version`** struct with major, minor, patch, pre-release, build metadata.
- **SemVer 2.0.0** compliant parsing, comparison, and ordering.
- **`VersionReq`** for version requirement matching (caret, tilde, exact, etc.).

### 3.4 Lockfile Format

```toml
# Build.lock - auto-generated, do not edit manually
version = 1

[[package]]
name = "json"
version = "1.2.3"
source = "registry+https://registry.buildlang.org"
checksum = "sha256:abc123..."

[[package]]
name = "http"
version = "0.2.1"
source = "registry+https://registry.buildlang.org"
checksum = "sha256:def456..."
dependencies = ["json 1.2.3"]
features = ["tls"]

[[package]]
name = "local-lib"
version = "0.1.0"
source = "path+../local-lib"
```

---

## 4. What Needs to Be Built

The compiler currently compiles single `.bld` files to C via the C backend.
There is no multi-file compilation, no module resolution, and no dependency
fetching at compile time. The pkg infrastructure exists as data structures
and algorithms but is not connected to the compilation pipeline.

### 4.1 Module Resolution (finding .bld files from `use` statements)

**Current state:** The parser recognizes `use` statements syntactically but
the compiler does not resolve them to files or load additional source.

**What needs to happen:**
1. After parsing a file, collect all `mod` declarations and `use` paths.
2. Resolve each to a filesystem path using the rules in Section 2.2.
3. Parse each resolved file recursively (with cycle detection).
4. Build a module tree that maps qualified names to their definitions.
5. Wire the module tree into name resolution so that `use json::Value` finds
   the `Value` type from the `json` package's module tree.

**Dependencies:** None. This can be built now with single-crate, multi-file
compilation as the scope.

### 4.2 Multi-File Compilation (compiling multiple .bld files into one output)

**Current state:** The compiler processes one file -> one AST -> one HIR -> one
MIR -> one C output. There is no mechanism to merge multiple MIR modules.

**What needs to happen:**
1. Compile each source file to its own MIR module independently.
2. Merge MIR modules: combine function lists, type definitions, string tables,
   global variables. Handle name collisions with module-qualified names
   (e.g., `mod_name__func_name` in the C output).
3. Resolve cross-module function calls: when file A calls a function defined
   in file B, the MIR for file A contains an external function declaration
   that matches file B's function definition.
4. Generate a single C output (or per-file C outputs linked together).

**Dependencies:** Module resolution (4.1) must be done first.

### 4.3 Dependency Fetching (downloading packages from registry)

**Current state:** `RegistryConfig`, `PackageMetadata`, `Resolver`, and
`Lockfile` types exist but have no network implementation. No HTTP client
is embedded in the compiler.

**What needs to happen:**
1. Implement `Registry::fetch()` -- HTTP GET to registry API for package metadata.
2. Implement `Registry::download()` -- download and extract package tarball to
   cache directory.
3. Integrate resolver: read `Build.toml`, resolve all dependencies, write
   `Build.lock`, download missing packages.
4. Add cache management: check if package version already exists in cache
   before downloading.
5. Wire into CLI: `buildc pkg add`, `buildc pkg update`, etc.

**Dependencies:** Module resolution (4.1) must be done first so that downloaded
packages can be compiled. An HTTP client library is needed (either vendored
or via system libcurl).

### 4.4 Build Caching (not recompiling unchanged files)

**Current state:** Every `buildc build` recompiles from scratch.

**What needs to happen:**
1. Hash each source file (content hash, not timestamp).
2. Store compiled artifacts (MIR or C output) alongside hashes in a build
   directory (`target/` or `.bld-cache/`).
3. On rebuild, compare current hashes to stored hashes. Only recompile files
   whose hash changed or whose dependencies changed.
4. For the C backend: compile each module to a separate `.c` file, compile
   each to `.o` with the system C compiler, then link. Only recompile changed
   `.c` files.

**Dependencies:** Multi-file compilation (4.2) must be done first. Build
caching only makes sense when there are multiple compilation units.

---

## 5. Standard Library Plan

The standard library (`std`) would be a built-in package that ships with the
compiler. It does not go through the registry.

### 5.1 Planned Modules

```
std::
  collections::    HashMap, HashSet, BTreeMap, BTreeSet, Vec, VecDeque
  io::             Read, Write, stdin, stdout, stderr, File
  fs::             read, write, create_dir, remove_file, metadata
  fmt::            Display, Debug, format!, println!
  string::         String, str methods
  math::           abs, sqrt, sin, cos, pow, min, max, PI, E
  net::            TcpStream, TcpListener, UdpSocket
  env::            args, vars, current_dir
  process::        Command, exit, abort
  sync::           Mutex, RwLock, Arc, atomic
  thread::         spawn, sleep, current
  time::           Instant, Duration, SystemTime
  error::          Error trait, Result extensions
```

### 5.2 Implementation Strategy

Since the compiler currently outputs C, the standard library would be
implemented as:
1. BuildLang source files that define the public API (types, function signatures).
2. C runtime functions (already partially in `compiler/src/codegen/runtime.rs`)
   that implement the actual behavior.
3. The compiler maps `std::io::println` to a C `printf` call in the generated
   output.

This is the same strategy used by early Rust (pre-LLVM maturity) and by Zig
(which compiles to C as a fallback).

---

## 6. Realistic Roadmap

### Phase 0: Current State (now)
- Single-file compilation via C backend
- `use` statements parsed but not resolved
- Package infrastructure exists as data structures only
- No multi-file, no dependencies, no caching

### Phase 1: Multi-File Compilation (estimated: 2-3 weeks of focused work)
- Implement module resolution (find .bld files from `mod` and `use`)
- Implement MIR module merging
- Support compiling a project directory (not just a single file)
- **Prerequisite for everything else**
- **Can be built now** -- no external dependencies needed

### Phase 2: Local Dependencies (estimated: 1-2 weeks after Phase 1)
- Support `path` dependencies in Build.toml: `my-lib = { path = "../my-lib" }`
- Compile dependency packages and link their MIR/C output
- This gives multi-package development without a registry
- **Can be built immediately after Phase 1**

### Phase 3: Build Caching (estimated: 1 week after Phase 2)
- Content-hash source files
- Cache compiled artifacts in `target/` directory
- Incremental recompilation
- **Can be built after Phase 2**

### Phase 4: Package Registry (estimated: 4-6 weeks after Phase 2)
- Build the registry server (separate project)
- Add HTTP client to compiler for fetching packages
- Implement `buildc pkg publish`, `buildc pkg add`
- Write Build.lock on resolve
- **Requires a hosted server and an HTTP client in the compiler**
- **This is the most infrastructure-heavy phase**

### Phase 5: Standard Library (ongoing, parallel to Phases 1-4)
- Start with `std::io::println` (already partially implemented via runtime)
- Add `std::string`, `std::math`, `std::collections` incrementally
- Each module is a .bld file + C runtime backing
- **Can start in parallel with Phase 1**

### What Can Be Built Right Now

The following can be implemented immediately with zero external dependencies:

1. **Module resolution** -- pure compiler work, no network, no new dependencies.
   Parse `mod` and `use` statements, resolve to files, build module tree.

2. **Multi-file C output** -- extend the C backend to produce one .c file
   from multiple .bld sources, or produce multiple .c files and a Makefile.

3. **`buildc pkg init`** -- generate a Build.toml template. The `Manifest`
   type already has `to_toml()`.

4. **Local path dependencies** -- once multi-file works, supporting
   `path = "../other-pkg"` is straightforward: read the other package's
   Build.toml, find its src/ files, compile and merge.

### What Requires Other Features First

| Feature | Requires |
|---------|----------|
| Registry fetch | HTTP client + hosted server |
| `buildc pkg publish` | Registry server + auth system |
| Git dependencies | Git CLI or libgit2 |
| Build caching | Multi-file compilation |
| Cross-package type checking | Module resolution + name resolution |
| Workspaces | Local path dependencies |
