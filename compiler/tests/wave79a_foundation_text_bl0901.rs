// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Regression tests for `BuildString::collect()` lowering when the expected
//! type is `BuildString` (e.g. `c.to_uppercase().collect::<String>()` in
//! `universe/foundation/text.bld`), while preserving the fail-closed `[BL0901]`
//! C-backend runtime container guard.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use buildlang::codegen::backend::c::CBackend;
use buildlang::codegen::backend::Backend;
use buildlang::codegen::ir::{
    BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirModule, MirRValue, MirStmt,
    MirTerminator, MirType, MirValue,
};

fn buildc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn stdlib_dir() -> PathBuf {
    repo_root().join("stdlib")
}

#[test]
fn char_case_collect_respects_string_and_vec_targets() {
    let dir =
        std::env::temp_dir().join(format!("buildlang_wave79a_collect_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let src = dir.join("char_case_collect.bld");
    std::fs::write(
        &src,
        r#"
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn typed_string_binding(c: char) -> String {
    let upper: String = c.to_uppercase().collect();
    upper
}

fn typed_vec_binding(c: char) -> Vec<char> {
    let chars: Vec<char> = c.to_uppercase().collect();
    chars
}

fn main() -> i32 ~ Console {
    let a = capitalize_first("helloWorld");
    let b = lower_first("Hello_world");
    let c = typed_string_binding('a');
    let v = typed_vec_binding('a');
    println!("{}|{}|{}|{}|{}", a, b, c, v.len(), v[0] as u32);
    0
}
"#,
    )
    .unwrap();

    for args in [
        vec!["run", src.to_str().unwrap()],
        vec!["run", "--release", src.to_str().unwrap()],
    ] {
        let out = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args(&args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "args {:?} failed: {}\nstdout: {}",
            args,
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        let txt = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            txt.trim(),
            "HelloWorld|hello_world|A|1|65",
            "unexpected output for {:?}: {txt}",
            args
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn universe_foundation_text_passes_buildc_check() {
    let text_bld = repo_root()
        .join("universe")
        .join("foundation")
        .join("text.bld");
    let out = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["check", text_bld.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "buildc check universe/foundation/text.bld failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bl0901_still_rejects_vec_char_into_build_string() {
    let mut mir = MirModule::new("bad_vec_char_to_string");
    let sig = MirFnSig::new(vec![MirType::Vec(Box::new(MirType::u32()))], MirType::Void);
    let mut func = MirFunction::new("bad_fn", sig);
    let mut p0 = MirLocal::named(LocalId(0), "v", MirType::Vec(Box::new(MirType::u32())));
    p0.is_param = true;
    func.add_local(p0);
    func.add_local(MirLocal::named(
        LocalId(1),
        "s",
        MirType::Struct(Arc::from("BuildString")),
    ));
    let mut bb = MirBlock::new(BlockId::ENTRY);
    bb.push_stmt(MirStmt::assign(
        LocalId(1),
        MirRValue::Use(MirValue::Local(LocalId(0))),
    ));
    bb.set_terminator(MirTerminator::Return(None));
    func.add_block(bb);
    mir.add_function(func);

    let mut c_backend = CBackend::new();
    let err = c_backend
        .generate(&mir)
        .expect_err("CBackend must still reject Vec<char> -> BuildString fail-closed");
    let msg = format!("{err}");
    assert!(
        msg.contains("incompatible runtime container"),
        "expected incompatible runtime container refusal, got: {msg}"
    );
}
