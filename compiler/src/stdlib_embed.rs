// ===============================================================================
// BUILDLANG COMPILER - EMBEDDED STANDARD LIBRARY
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! The standard library modules, compiled into `buildc`.
//!
//! `cargo install buildlang` installs only the binary, so before this a
//! program that used `mod math;` from the standard library needed
//! `BUILDLANG_STDLIB` pointed at a clone. When no stdlib directory is found on
//! disk, `buildc` now writes these files to a versioned directory under the
//! system temp directory and uses that. `compiler/stdlib/` mirrors the
//! repository's `stdlib/`; a test fails if they differ.

use std::path::PathBuf;

/// Every standard library module, by file name.
pub const FILES: &[(&str, &str)] = &[
    ("algorithms.bld", include_str!("../stdlib/algorithms.bld")),
    ("bitwise.bld", include_str!("../stdlib/bitwise.bld")),
    ("core.bld", include_str!("../stdlib/core.bld")),
    ("effects.bld", include_str!("../stdlib/effects.bld")),
    ("graphics.bld", include_str!("../stdlib/graphics.bld")),
    ("io.bld", include_str!("../stdlib/io.bld")),
    ("iter.bld", include_str!("../stdlib/iter.bld")),
    ("linalg.bld", include_str!("../stdlib/linalg.bld")),
    ("math.bld", include_str!("../stdlib/math.bld")),
    ("option.bld", include_str!("../stdlib/option.bld")),
    ("result.bld", include_str!("../stdlib/result.bld")),
    ("sorting.bld", include_str!("../stdlib/sorting.bld")),
    (
        "string_utils.bld",
        include_str!("../stdlib/string_utils.bld"),
    ),
    ("strings.bld", include_str!("../stdlib/strings.bld")),
];

/// Write the embedded modules to `dir` (created if missing). A file that
/// already holds the right bytes is left alone.
pub fn materialize_into(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, text) in FILES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            std::fs::write(&path, text)?;
        }
    }
    Ok(())
}

/// The directory the embedded stdlib is written to for this compiler version.
pub fn default_dir() -> PathBuf {
    std::env::temp_dir().join(format!("buildlang-stdlib-{}", buildlang::VERSION))
}

/// Materialize into the default directory; `None` if it cannot be written.
pub fn materialize() -> Option<PathBuf> {
    let dir = default_dir();
    match materialize_into(&dir) {
        Ok(()) => Some(dir),
        Err(err) => {
            eprintln!(
                "warning: cannot write the embedded stdlib to '{}': {}",
                dir.display(),
                err
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_stdlib() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stdlib")
    }

    #[test]
    fn embedded_copy_matches_the_repository_stdlib() {
        let Ok(entries) = std::fs::read_dir(repo_stdlib()) else {
            // Packaged crate: the repository stdlib is not present.
            return;
        };
        let mut on_disk: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".bld") && n != "131_stdlib_integration.bld")
            .collect();
        on_disk.sort();
        let embedded: Vec<String> = FILES.iter().map(|(n, _)| n.to_string()).collect();
        assert_eq!(
            embedded, on_disk,
            "compiler/stdlib and stdlib list different modules"
        );
        for (name, text) in FILES {
            let disk = std::fs::read_to_string(repo_stdlib().join(name)).unwrap();
            assert_eq!(
                disk.replace("\r\n", "\n"),
                text.replace("\r\n", "\n"),
                "compiler/stdlib/{name} differs from stdlib/{name}; copy it over"
            );
        }
    }

    #[test]
    fn materialize_writes_every_module() {
        let dir =
            std::env::temp_dir().join(format!("buildlang_stdlib_embed_{}", std::process::id()));
        materialize_into(&dir).unwrap();
        for (name, text) in FILES {
            assert_eq!(&std::fs::read_to_string(dir.join(name)).unwrap(), text);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
