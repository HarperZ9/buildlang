// ===============================================================================
// `buildc pkg`: CREATE, PUBLISH, RESOLVE, VERIFY AND BUILD PACKAGES
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! The command line over `buildlang::pkg::local`. Every failure prints
//! `error[BL07xx]: ...` and exits 1.

use buildlang::codegen::StdioMode;
use buildlang::pkg::local::content::{package_checksum, sha256_hex};
use buildlang::pkg::local::index::{find_registry, Registry};
use buildlang::pkg::local::ops::{lock_project, publish};
use buildlang::pkg::local::source::{build_input, read_lock, verify_locked};
use buildlang::pkg::local::{project::read_project, PkgError, PkgResult};
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum PkgCommands {
    /// Create a package: Build.toml and src/main.bld (or src/lib.bld with --lib)
    Init {
        /// Package directory
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Create a library (src/lib.bld) instead of a program
        #[arg(long)]
        lib: bool,
    },
    /// Add a registry dependency to Build.toml
    Add {
        /// Package name
        name: String,
        /// Version requirement (e.g. "^1.0"); default: ^ the newest published version
        #[arg(long)]
        version: Option<String>,
        /// Package directory
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Registry folder (default: BUILDLANG_REGISTRY, then the nearest registry/)
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
    },
    /// Publish the package into the registry and record its content hash
    Publish {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
    },
    /// Resolve dependency versions and write Build.lock
    Resolve {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
    },
    /// Check every locked package against its content hash
    Verify {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
    },
    /// Build offline from Build.lock; writes target/pkg/ and a build receipt
    Build {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
        /// 'c' for the C source only, 'exe' for an executable too
        #[arg(long, default_value = "exe")]
        emit: String,
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Generated-program stdio behavior: native, portable-lf
        #[arg(long, default_value_t = StdioMode::PortableLf)]
        stdio_mode: StdioMode,
    },
    /// Check that every version in the registry hashes to its recorded checksum
    CheckRegistry {
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
        /// Record the checksum of versions whose checksum is empty
        #[arg(long)]
        fill_missing: bool,
    },
    /// Search the registry by name or description
    Search {
        query: String,
        #[arg(long, value_name = "DIR")]
        registry: Option<PathBuf>,
    },
}

pub fn cmd_pkg(cmd: PkgCommands) -> Result<(), i32> {
    let result = match cmd {
        PkgCommands::Init { path, lib } => init(&path, lib),
        PkgCommands::Add {
            name,
            version,
            path,
            registry,
        } => add(&path, &name, version, registry.as_deref()),
        PkgCommands::Publish { path, registry } => do_publish(&path, registry.as_deref()),
        PkgCommands::Resolve { path, registry } => resolve(&path, registry.as_deref()),
        PkgCommands::Verify { path, registry } => verify(&path, registry.as_deref()),
        PkgCommands::Build {
            path,
            registry,
            emit,
            release,
            stdio_mode,
        } => return build(&path, registry.as_deref(), &emit, release, stdio_mode),
        PkgCommands::CheckRegistry {
            registry,
            fill_missing,
        } => check_registry(registry.as_deref(), fill_missing),
        PkgCommands::Search { query, registry } => search(&query, registry.as_deref()),
    };
    result.map_err(|e| {
        eprintln!("{}", e);
        1
    })
}

fn open_registry(path: &Path, explicit: Option<&Path>) -> PkgResult<Registry> {
    Registry::load(&find_registry(path, explicit)?)
}

fn init(path: &Path, lib: bool) -> PkgResult<()> {
    let manifest = path.join("Build.toml");
    if manifest.exists() {
        return Err(PkgError::new(
            buildlang::diagnostics::codes::PKG_MANIFEST,
            format!("{} already exists", manifest.display()),
        ));
    }
    std::fs::create_dir_all(path.join("src")).map_err(|e| PkgError::io(path, e))?;
    let name = path
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "my-package".to_string())
        .replace('_', "-");
    let text = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\ndescription = \"\"\nlicense = \"\"\n\n[dependencies]\n",
        name
    );
    write(&manifest, &text)?;
    let (file, body) = if lib {
        (
            "lib.bld",
            "/// A function other packages can call.\nfn answer() -> i64 {\n    42\n}\n",
        )
    } else {
        (
            "main.bld",
            "fn main() ~ Console {\n    println!(\"hello\");\n}\n",
        )
    };
    let src = path.join("src").join(file);
    if !src.exists() {
        write(&src, body)?;
    }
    println!("Created {} and {}", manifest.display(), src.display());
    Ok(())
}

fn add(path: &Path, name: &str, version: Option<String>, reg: Option<&Path>) -> PkgResult<()> {
    let manifest = path.join("Build.toml");
    let text = std::fs::read_to_string(&manifest).map_err(|e| PkgError::io(&manifest, e))?;
    let req = match version {
        Some(v) => v,
        None => {
            let registry = open_registry(path, reg)?;
            match registry.versions(name).last() {
                Some(v) => format!("^{}", v),
                None => {
                    return Err(PkgError::new(
                        buildlang::diagnostics::codes::PKG_UNRESOLVABLE,
                        format!("no package named {} in {}", name, registry.root.display()),
                    ))
                }
            }
        }
    };
    let line = format!("{} = \"{}\"", name, req);
    let mut out = String::new();
    let mut placed = false;
    for l in text.lines() {
        out.push_str(l);
        out.push('\n');
        if l.trim() == "[dependencies]" && !placed {
            out.push_str(&line);
            out.push('\n');
            placed = true;
        }
    }
    if !placed {
        out.push_str(&format!("\n[dependencies]\n{}\n", line));
    }
    write(&manifest, &out)?;
    println!("Added {}", line);
    Ok(())
}

fn do_publish(path: &Path, reg: Option<&Path>) -> PkgResult<()> {
    let mut registry = open_registry(path, reg)?;
    let p = publish(path, &mut registry)?;
    let verb = if p.added {
        "Published"
    } else {
        "Already published (unchanged)"
    };
    println!("{} {} {} at {}", verb, p.name, p.version, p.path);
    println!("  {} files, {}", p.files.len(), p.checksum);
    Ok(())
}

fn resolve(path: &Path, reg: Option<&Path>) -> PkgResult<()> {
    let registry = open_registry(path, reg)?;
    let (lock, changed) = lock_project(path, &registry)?;
    for (name, p) in &lock.packages {
        println!("  {} {} {}", name, p.version, p.checksum);
    }
    let state = if changed { "written" } else { "unchanged" };
    println!("Build.lock {} ({} packages)", state, lock.packages.len());
    Ok(())
}

fn verify(path: &Path, reg: Option<&Path>) -> PkgResult<()> {
    let registry = open_registry(path, reg)?;
    let project = read_project(path)?;
    let (lock, _) = read_lock(path, &project)?;
    verify_locked(&registry, &lock)?;
    println!(
        "{} locked packages match their checksums",
        lock.packages.len()
    );
    Ok(())
}

fn check_registry(reg: Option<&Path>, fill: bool) -> PkgResult<()> {
    let mut registry = open_registry(Path::new("."), reg)?;
    let mut filled = 0;
    let mut checked = 0;
    let names: Vec<String> = registry.index.packages.keys().cloned().collect();
    for name in names {
        let versions: Vec<String> = registry.index.packages[&name]
            .versions
            .keys()
            .cloned()
            .collect();
        for v in versions {
            let entry = registry.index.packages[&name].versions[&v].clone();
            let actual = package_checksum(&registry.version_dir(&entry))?;
            if entry.checksum.is_empty() && fill {
                let e = registry.index.packages.get_mut(&name).expect("listed");
                e.versions.get_mut(&v).expect("listed").checksum = actual.clone();
                filled += 1;
            } else if entry.checksum != actual {
                return Err(PkgError::new(
                    buildlang::diagnostics::codes::PKG_CHECKSUM,
                    format!(
                        "{} {} at {}: index {}, files {}",
                        name, v, entry.path, entry.checksum, actual
                    ),
                ));
            }
            println!("  {} {} {}", name, v, actual);
            checked += 1;
        }
    }
    if filled > 0 {
        registry.save()?;
    }
    println!(
        "{} versions checked, {} checksums recorded",
        checked, filled
    );
    Ok(())
}

fn search(query: &str, reg: Option<&Path>) -> PkgResult<()> {
    let registry = open_registry(Path::new("."), reg)?;
    let q = query.to_lowercase();
    let mut found = 0;
    for (name, e) in &registry.index.packages {
        if name.contains(&q) || e.description.to_lowercase().contains(&q) {
            let versions: Vec<&str> = e.versions.keys().map(|s| s.as_str()).collect();
            println!("  {} [{}] - {}", name, versions.join(", "), e.description);
            found += 1;
        }
    }
    println!("{} package(s) found.", found);
    Ok(())
}

fn write(path: &Path, text: &str) -> PkgResult<()> {
    std::fs::write(path, text).map_err(|e| PkgError::io(path, e))
}

/// Offline build: verify the lock, join the sources, compile with the C
/// backend, and write `target/pkg/<name>.c`, the executable and
/// `target/pkg/build-receipt.json`.
fn build(
    path: &Path,
    reg: Option<&Path>,
    emit: &str,
    release: bool,
    stdio: StdioMode,
) -> Result<(), i32> {
    let fail = |e: PkgError| {
        eprintln!("{}", e);
        1
    };
    if emit != "c" && emit != "exe" {
        eprintln!("error: --emit must be 'c' or 'exe'");
        return Err(1);
    }
    let registry = open_registry(path, reg).map_err(fail)?;
    let input = build_input(path, &registry).map_err(fail)?;
    let out_dir = path.join("target").join("pkg");
    let work = out_dir.join("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| fail(PkgError::io(&work, e)))?;
    write(&work.join("main.bld"), &input.source).map_err(fail)?;

    crate::cmd_build(&work, release, emit, true, "c", stdio)?;

    let mode = if release { "release" } else { "debug" };
    let built = work.join("target").join(mode);
    let name = &input.project.name;
    let c_src = built.join("main.c");
    let c_bytes = std::fs::read(&c_src).map_err(|e| fail(PkgError::io(&c_src, e)))?;
    let c_out = out_dir.join(format!("{}.c", name));
    std::fs::write(&c_out, &c_bytes).map_err(|e| fail(PkgError::io(&c_out, e)))?;
    let mut exe_sha = String::new();
    if emit == "exe" {
        let exe_name = if cfg!(windows) { "main.exe" } else { "main" };
        let exe = built.join(exe_name);
        let bytes = std::fs::read(&exe).map_err(|e| fail(PkgError::io(&exe, e)))?;
        exe_sha = sha256_hex(&bytes);
        let target = out_dir.join(exe_name.replace("main", name));
        std::fs::write(&target, &bytes).map_err(|e| fail(PkgError::io(&target, e)))?;
        println!("Executable: {}", target.display());
    }
    let receipt = serde_json::json!({
        "schema": "buildlang.pkg-build/v1",
        "package": name,
        "version": input.project.version,
        "lock_sha256": sha256_hex(input.lock_text.replace("\r\n", "\n").as_bytes()),
        "packages": input.order.iter().map(|n| {
            let p = &input.lock.packages[n];
            serde_json::json!({"name": n, "version": p.version.to_string(), "checksum": p.checksum})
        }).collect::<Vec<_>>(),
        "source_sha256": sha256_hex(input.source.as_bytes()),
        "artifact": {"kind": "c", "file": format!("{}.c", name), "sha256": sha256_hex(&c_bytes)},
        "executable_sha256": exe_sha,
        "stdio_mode": stdio.as_str(),
        "mode": mode,
    });
    let receipt_path = out_dir.join("build-receipt.json");
    let mut text = serde_json::to_string_pretty(&receipt).unwrap_or_default();
    text.push('\n');
    write(&receipt_path, &text).map_err(fail)?;
    println!(
        "Artifact: {} sha256:{}",
        c_out.display(),
        sha256_hex(&c_bytes)
    );
    println!("Receipt: {}", receipt_path.display());
    Ok(())
}
