//! One spirv-val lookup for every build path.
//!
//! `find_spirv_val` in `src/main.rs` tries `spirv-val` on PATH first and only
//! then the fixed Vulkan SDK path. The watch loop once kept its own list with
//! the SDK path first, so a machine with an older SDK installed validated
//! with that SDK instead of the tool on PATH (the one CI installs and pins).
//! These checks read the source: the fixed path appears once, inside
//! `find_spirv_val`, after the PATH probe.

fn main_rs() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs");
    std::fs::read_to_string(path).expect("read src/main.rs")
}

/// The SDK directory name, split so this file does not count as a use of it.
fn sdk_marker() -> String {
    ["Vulkan", "SDK\\"].concat()
}

fn find_spirv_val_body(src: &str) -> &str {
    let start = src
        .find("fn find_spirv_val() -> Option<String> {")
        .expect("find_spirv_val is defined in src/main.rs");
    let rest = &src[start..];
    let end = rest.find("\n}\n").or_else(|| rest.find("\r\n}\r\n"));
    &rest[..end.expect("find_spirv_val has a closing brace")]
}

#[test]
fn the_fixed_sdk_path_appears_once_inside_find_spirv_val() {
    let src = main_rs();
    let marker = sdk_marker();
    let uses = src.matches(marker.as_str()).count();
    assert_eq!(
        uses, 1,
        "src/main.rs names the Vulkan SDK path {uses} times; every caller must use find_spirv_val"
    );
    assert!(
        find_spirv_val_body(&src).contains(marker.as_str()),
        "the one Vulkan SDK path must be inside find_spirv_val"
    );
}

#[test]
fn find_spirv_val_tries_path_before_the_sdk() {
    let src = main_rs();
    let body = find_spirv_val_body(&src);
    let path_probe = body
        .find("Command::new(tool)")
        .expect("find_spirv_val probes the PATH candidates");
    let sdk = body
        .find(sdk_marker().as_str())
        .expect("find_spirv_val keeps the SDK fallback");
    assert!(
        path_probe < sdk,
        "find_spirv_val must try spirv-val on PATH before the fixed SDK path"
    );
}
