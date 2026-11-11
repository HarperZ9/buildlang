//! G29: two constants or statics of one name in one module. The check passed
//! and the C compiler stopped with "redefinition of 'LIMIT'". `buildc check`
//! now reports the second definition and names the line of the first.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

/// `buildc check` on `src`: (success, stdout and stderr together).
fn check(tag: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("bl-dupconst-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// The reproduction: the diagnostic is on line 3 and names line 2.
#[test]
fn a_duplicate_const_is_reported_with_both_lines() {
    let src = "// two limits\n\
               const LIMIT: u64 = 1_u64;\n\
               const LIMIT: u64 = 2_u64;\n\
               fn main() ~ Console { println!(\"{}\", LIMIT); }\n";
    let (ok, text) = check("const", src);
    assert!(!ok, "a duplicate const must fail the check: {text}");
    assert!(
        text.contains("3:7: [BL0380] duplicate definition: `LIMIT` is already defined on line 2"),
        "{text}"
    );
}

/// Statics share the namespace of constants, and an inline module is checked
/// on its own.
#[test]
fn statics_and_inline_modules_are_covered() {
    let src = "static mut G: u64 = 1_u64;\n\
               static mut G: u64 = 2_u64;\n\
               const A: u64 = 1_u64;\n\
               static A: u64 = 2_u64;\n\
               mod inner {\n\
               const B: u8 = 1_u8;\n\
               const B: u8 = 2_u8;\n\
               }\n\
               fn main() ~ Console { println!(\"{}\", A); }\n";
    let (ok, text) = check("static", src);
    assert!(!ok, "{text}");
    for wanted in [
        "duplicate definition: `G` is already defined on line 1",
        "duplicate definition: `A` is already defined on line 3",
        "duplicate definition: `B` is already defined on line 6",
    ] {
        assert!(text.contains(wanted), "missing {wanted:?} in {text}");
    }
}

/// One name in two modules is two constants, and a third definition is
/// reported against the first. A check that flagged every constant would
/// fail the first half.
#[test]
fn the_same_name_in_another_module_is_not_a_duplicate() {
    let ok_src = "const N: u64 = 1_u64;\n\
                  mod a { const N: u64 = 2_u64; }\n\
                  mod b { const N: u64 = 3_u64; }\n\
                  const M: u64 = 4_u64;\n\
                  fn main() ~ Console { println!(\"{} {}\", N, M); }\n";
    let (ok, text) = check("modules", ok_src);
    assert!(ok, "{text}");
    assert!(!text.contains("BL0380"), "{text}");

    let three = "const N: u8 = 1_u8;\nconst N: u8 = 2_u8;\nconst N: u8 = 3_u8;\n\
                 fn main() ~ Console { println!(\"{}\", N); }\n";
    let (ok, text) = check("three", three);
    assert!(!ok, "{text}");
    let reports = text.matches("[BL0380]").count();
    assert_eq!(reports, 2, "one report per extra definition: {text}");
    assert!(!text.contains("defined on line 2"), "{text}");
}

/// Two `struct`, or a `struct` and an `enum`, of one name failed in the C
/// compiler after the check passed. Types are their own namespace: a type and
/// a constant may share a name. Functions are left alone, because two
/// functions of one name with different parameter types are overloads.
#[test]
fn duplicate_types_are_reported_and_overloads_are_not() {
    let src = "struct S { a: u8 }
               struct S { b: u8 }
               enum S { X }
               fn main() ~ Console { println!(\"{}\", 1_u8); }
";
    let (ok, text) = check("types", src);
    assert!(!ok, "{text}");
    assert!(
        text.contains("duplicate definition: `S` is already defined on line 1"),
        "{text}"
    );
    assert_eq!(text.matches("[BL0380]").count(), 2, "{text}");

    let apart = "const K: u64 = 1_u64;
                 fn g(x: u64) -> u64 { x }
                 fn g(x: u8) -> u64 { 2_u64 }
                 struct T { a: u8 }
                 impl T { fn g(&self) -> u64 { 3_u64 } }
                 fn main() ~ Console { let t = T { a: 1_u8 }; println!(\"{} {} {} {}\", K, g(5_u64), g(5_u8), t.g()); }
";
    let (ok, text) = check("apart", apart);
    assert!(ok, "{text}");
    assert!(!text.contains("BL0380"), "{text}");
}
