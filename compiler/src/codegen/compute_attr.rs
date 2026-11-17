// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Compute entry points: the `#[compute(workgroup_size = [x, y, z])]` attribute,
//! the check that shader entry names are unique, and `--entry` selection.
//!
//! The attribute text is read from the source through the attribute's span,
//! because attribute token trees carry spans and kinds but no text. Parsing is
//! a pure function of that text so it is tested without a compiler run.
//!
//! Limits on the group size follow D3D12's compute limits: every dimension at
//! least 1, x and y at most 1,024, z at most 64, and at most 1,024 invocations
//! in one group. Vulkan only requires a device to allow 128 x 128 x 64 and 128
//! invocations, so a size inside these limits can still exceed what a given
//! Vulkan device reports; pipeline creation on that device refuses it.

use crate::ast;

use super::ir::MirModule;

/// Largest x or y dimension of a compute group.
pub const MAX_GROUP_XY: u32 = 1024;
/// Largest z dimension of a compute group.
pub const MAX_GROUP_Z: u32 = 64;
/// Largest number of invocations in one compute group (x * y * z).
pub const MAX_GROUP_INVOCATIONS: u32 = 1024;

/// Names of the attributes that mark a shader entry point.
const STAGE_ATTRS: [&str; 3] = ["vertex", "fragment", "compute"];

/// The first path segment of an attribute, as text.
fn attr_name(attr: &ast::Attribute) -> Option<&str> {
    attr.path.segments.first().map(|s| s.ident.name.as_ref())
}

/// True iff `attrs` carries a shader-stage attribute.
pub fn has_stage_attr(attrs: &[ast::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| attr_name(a).is_some_and(|n| STAGE_ATTRS.contains(&n)))
}

/// The source text of `attr` (from `#` to the closing `]`), if the span is
/// inside `source`.
pub fn attr_text<'a>(attr: &ast::Attribute, source: &'a str) -> Option<&'a str> {
    let start = attr.span.start.to_usize();
    let end = attr.span.end.to_usize();
    source.get(start..end)
}

/// The group size a function's `#[compute(..)]` attribute declares, `None` for
/// a bare `#[compute]` or no compute attribute, or an error naming what is
/// wrong. Also refuses two shader-stage attributes on one function.
pub fn workgroup_size_of(
    attrs: &[ast::Attribute],
    source: Option<&str>,
) -> Result<Option<[u32; 3]>, String> {
    let stages: Vec<&str> = attrs
        .iter()
        .filter_map(attr_name)
        .filter(|n| STAGE_ATTRS.contains(n))
        .collect();
    if stages.len() > 1 {
        return Err(format!(
            "a function carries one shader-stage attribute; this one has {}: {}",
            stages.len(),
            stages
                .iter()
                .map(|s| format!("#[{s}]"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let Some(attr) = attrs.iter().find(|a| attr_name(a) == Some("compute")) else {
        return Ok(None);
    };
    match &attr.args {
        ast::AttrArgs::Empty => Ok(None),
        ast::AttrArgs::Eq(_) => Err("`#[compute = ..]` is not a form of the attribute; write \
             `#[compute(workgroup_size = [x, y, z])]`"
            .to_string()),
        ast::AttrArgs::Delimited(_) => {
            let text = source.and_then(|s| attr_text(attr, s)).ok_or_else(|| {
                "the compiler has no source text for this `#[compute(..)]` attribute".to_string()
            })?;
            parse_compute_attr(text)
        }
    }
}

/// Parse the text of one compute attribute, `#[compute]` or
/// `#[compute(workgroup_size = [x, y, z])]`, and check the size.
pub fn parse_compute_attr(text: &str) -> Result<Option<[u32; 3]>, String> {
    let t = text.trim();
    let inner = t
        .strip_prefix("#[")
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| format!("`{t}` is not an attribute"))?
        .trim();
    let rest = inner
        .strip_prefix("compute")
        .ok_or_else(|| format!("`{t}` is not a `#[compute]` attribute"))?
        .trim();
    if rest.is_empty() {
        return Ok(None);
    }
    let args = rest
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| format!("`{t}`: expected `(workgroup_size = [x, y, z])`"))?
        .trim();
    if args.is_empty() {
        return Ok(None);
    }

    let mut size: Option<[u32; 3]> = None;
    for (key, value) in split_args(args)? {
        match key {
            "workgroup_size" => {
                if size.is_some() {
                    return Err("`workgroup_size` is given twice in one `#[compute]`".to_string());
                }
                size = Some(parse_size(value)?);
            }
            other => {
                return Err(format!(
                    "`#[compute]` has no argument `{other}`; the one argument is \
                     `workgroup_size = [x, y, z]`"
                ))
            }
        }
    }
    let size = size.expect("split_args returns at least one argument");
    check_size(size)?;
    Ok(Some(size))
}

/// Split `key = value, key = value` at top-level commas (commas inside `[..]`
/// belong to the value).
fn split_args(args: &str) -> Result<Vec<(&str, &str)>, String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in args.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&args[start..]);
    parts
        .into_iter()
        .map(|p| {
            let (k, v) = p
                .split_once('=')
                .ok_or_else(|| format!("`{}`: expected `key = value`", p.trim()))?;
            Ok((k.trim(), v.trim()))
        })
        .collect()
}

/// Parse `[x, y, z]` into three unsigned integers.
fn parse_size(value: &str) -> Result<[u32; 3], String> {
    let body = value
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| format!("`workgroup_size = {value}`: expected `[x, y, z]`"))?;
    let dims: Vec<&str> = body.split(',').map(str::trim).collect();
    if dims.len() != 3 {
        return Err(format!(
            "`workgroup_size` takes three dimensions `[x, y, z]`; `{value}` has {}",
            dims.len()
        ));
    }
    let mut out = [0u32; 3];
    for (slot, d) in out.iter_mut().zip(&dims) {
        let digits = d.strip_suffix("u32").unwrap_or(d).replace('_', "");
        *slot = digits.parse::<u32>().map_err(|_| {
            format!("`workgroup_size` dimension `{d}` is not an unsigned integer literal")
        })?;
    }
    Ok(out)
}

/// Check a group size against the portable limits.
pub fn check_size(size: [u32; 3]) -> Result<(), String> {
    let axes = ["x", "y", "z"];
    for (axis, &d) in axes.iter().zip(size.iter()) {
        if d == 0 {
            return Err(format!(
                "`workgroup_size` dimension {axis} is 0; every dimension is at least 1"
            ));
        }
    }
    for (axis, &d) in axes.iter().zip(size.iter()).take(2) {
        if d > MAX_GROUP_XY {
            return Err(format!(
                "`workgroup_size` dimension {axis} is {d}; the limit is {MAX_GROUP_XY}"
            ));
        }
    }
    if size[2] > MAX_GROUP_Z {
        return Err(format!(
            "`workgroup_size` dimension z is {}; the limit is {MAX_GROUP_Z}",
            size[2]
        ));
    }
    let total = size.iter().map(|&d| u64::from(d)).product::<u64>();
    if total > u64::from(MAX_GROUP_INVOCATIONS) {
        return Err(format!(
            "`workgroup_size` [{}, {}, {}] is {total} invocations; the limit is {MAX_GROUP_INVOCATIONS}",
            size[0], size[1], size[2]
        ));
    }
    Ok(())
}

/// Refuse two shader entry points of one name among a module's top-level
/// functions. Function names may repeat as overloads, but an entry point is
/// found by name, so two entries of one name cannot both be selected.
/// Returns the span of the second definition with the message.
pub fn duplicate_entry(module: &ast::Module) -> Option<(String, crate::lexer::Span)> {
    let mut seen: Vec<&str> = Vec::new();
    for item in &module.items {
        let ast::ItemKind::Function(f) = &item.kind else {
            continue;
        };
        if !has_stage_attr(&item.attrs) {
            continue;
        }
        let name = f.name.name.as_ref();
        if seen.contains(&name) {
            return Some((
                format!(
                    "duplicate shader entry point `{name}`: two entry points cannot share a name"
                ),
                f.name.span,
            ));
        }
        seen.push(name);
    }
    None
}

/// Keep only the shader entry `entry`: every other shader-stage function stops
/// being an entry point (and is dropped by the shader backends unless called).
/// An unknown name is an error listing the entries the module has.
pub fn select_entry(mir: &mut MirModule, entry: &str) -> Result<(), String> {
    let entries: Vec<String> = mir
        .functions
        .iter()
        .filter(|f| f.shader_stage.is_some() && !f.is_declaration())
        .map(|f| f.name.to_string())
        .collect();
    if !entries.iter().any(|e| e == entry) {
        let have = if entries.is_empty() {
            "this file has none".to_string()
        } else {
            format!("this file has: {}", entries.join(", "))
        };
        return Err(format!(
            "--entry: no shader entry point named `{entry}`; {have}"
        ));
    }
    for f in &mut mir.functions {
        if f.shader_stage.is_some() && f.name.as_ref() != entry {
            f.shader_stage = None;
            f.workgroup_size = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
