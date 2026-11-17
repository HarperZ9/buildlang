// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Unit tests of `compute_attr`: attribute text in, size or refusal out.

use super::*;
use crate::codegen::ir::ShaderStage;

#[test]
fn bare_compute_has_no_size() {
    assert_eq!(parse_compute_attr("#[compute]"), Ok(None));
    assert_eq!(parse_compute_attr("#[compute()]"), Ok(None));
}

#[test]
fn size_is_read_in_order() {
    assert_eq!(
        parse_compute_attr("#[compute(workgroup_size = [8, 4, 2])]"),
        Ok(Some([8, 4, 2]))
    );
    assert_eq!(
        parse_compute_attr("#[ compute( workgroup_size=[64u32,1,1] ) ]"),
        Ok(Some([64, 1, 1]))
    );
}

#[test]
fn zero_dimension_is_refused() {
    let e = parse_compute_attr("#[compute(workgroup_size = [8, 0, 1])]").unwrap_err();
    assert!(e.contains("dimension y is 0"), "{e}");
}

#[test]
fn oversized_dimensions_are_refused() {
    let e = parse_compute_attr("#[compute(workgroup_size = [2048, 1, 1])]").unwrap_err();
    assert!(e.contains("dimension x is 2048"), "{e}");
    let e = parse_compute_attr("#[compute(workgroup_size = [1, 1, 65])]").unwrap_err();
    assert!(e.contains("dimension z is 65"), "{e}");
    let e = parse_compute_attr("#[compute(workgroup_size = [64, 32, 1])]").unwrap_err();
    assert!(e.contains("2048 invocations"), "{e}");
    // The limits themselves are accepted.
    assert_eq!(
        parse_compute_attr("#[compute(workgroup_size = [1024, 1, 1])]"),
        Ok(Some([1024, 1, 1]))
    );
    assert_eq!(
        parse_compute_attr("#[compute(workgroup_size = [16, 1, 64])]"),
        Ok(Some([16, 1, 64]))
    );
}

#[test]
fn malformed_arguments_are_refused() {
    for (text, needle) in [
        ("#[compute(workgroup_size = [8, 8])]", "three dimensions"),
        (
            "#[compute(workgroup_size = [8, 8, 1, 1])]",
            "three dimensions",
        ),
        ("#[compute(workgroup_size = 64)]", "expected `[x, y, z]`"),
        ("#[compute(workgroup_size = [8, -1, 1])]", "not an unsigned"),
        ("#[compute(threads = [8, 8, 1])]", "no argument `threads`"),
        (
            "#[compute(workgroup_size = [8, 8, 1], workgroup_size = [4, 4, 1])]",
            "given twice",
        ),
    ] {
        let e = parse_compute_attr(text).unwrap_err();
        assert!(e.contains(needle), "{text}: {e}");
    }
}

#[test]
fn select_entry_keeps_one_and_names_the_rest_on_a_miss() {
    use crate::codegen::ir::{MirFnSig, MirFunction, MirType};
    let mut mir = MirModule::new("m");
    for name in ["cs_pack", "cs_main"] {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::Void));
        f.blocks = Some(vec![]);
        f.shader_stage = Some(ShaderStage::Compute);
        f.workgroup_size = Some([8, 8, 1]);
        mir.functions.push(f);
    }
    let e = select_entry(&mut mir.clone(), "nope").unwrap_err();
    assert!(e.contains("no shader entry point named `nope`"), "{e}");
    assert!(e.contains("cs_pack, cs_main"), "{e}");

    select_entry(&mut mir, "cs_main").unwrap();
    let kept: Vec<&str> = mir
        .functions
        .iter()
        .filter(|f| f.shader_stage.is_some())
        .map(|f| f.name.as_ref())
        .collect();
    assert_eq!(kept, ["cs_main"]);
    assert_eq!(mir.functions[0].workgroup_size, None);
}
