//! `std::memory`: CPU textures and a typed buffer, as plain data.
//!
//! 30 files of `universe/` import `core::memory` or `std::memory`, a module
//! the standard library did not have. `stdlib/memory.bld` declares
//! `Texture2D`, `Texture3D` and `Buffer<T>`. Device resources (`GpuBuffer`,
//! `GpuTexture`) are not here: `universe/photon/gpu.bld` declares those.
//!
//! Values written before the first run. A 4 by 2 texture holds 32 numbers.
//! Texel (3, 1) set to (1, 2, 3, 4) reads back; a texel outside reads 0.
//! `sample((0.99, 0.99))` is texel (3, 1); `sample((-1, 5))` clamps to
//! texel (0, 1), which is zero. After `clear((0.5, 0.5, 0.5, 1))`, red of
//! one texel plus alpha of another is 1.5. A 2x2x2 texture from the numbers
//! 0..8 has 1 at (1,0,0), 6 at (0,1,1) and alpha 1; the point (1.7, 1.2, 9)
//! clamps to texel (1,1,1) = 7; the trilinear sample at the centre is the
//! mean of 0..8, 3.5, and at (0.25, 0.25, 0.25) the centre of texel
//! (0,0,0), 0; the 2D sample at (0.9, 0.9) is texel (1,1,0) = 3. Eight
//! numbers as RGBA give texel 1 = (4, 5, 6, 7).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-memory-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, text).expect("source file");
    main
}

fn buildc(command: &str, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(command)
        .arg(file)
        .output()
        .expect("buildc runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Check, build and run the program; its stdout must be `wanted`.
fn checks_and_prints(main: &Path, wanted: &str) {
    let check = buildc("check", main);
    assert!(check.status.success(), "the check failed\n{}", text(&check));

    let run = buildc("run", main);
    let log = text(&run);
    // A machine with no C compiler cannot answer. CI has one, and there a
    // missing compiler is a failure, not a pass.
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler\n{log}"
        );
        return;
    }
    assert_eq!(run.status.code(), Some(0), "{log}");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), wanted, "{log}");
}

const TEXTURES: &str = r#"fn main() ~ Console {
    let mut t = Texture2D::new(4, 2);
    println!("{} {} {}", t.width(), t.height(), t.data.len());
    t.set_pixel(3, 1, Vec4::new(1.0, 2.0, 3.0, 4.0));
    let p = t.get_pixel(3, 1);
    println!("{} {} {} {}", p.x, p.y, p.z, p.w);
    let q = t.get_pixel(4, 0);
    println!("{}", q.x);
    let s = t.sample(Vec2::new(0.99, 0.99));
    println!("{}", s.w);
    let s0 = t.sample(Vec2::new(-1.0, 5.0));
    println!("{}", s0.w);
    t.clear(Vec4::new(0.5, 0.5, 0.5, 1.0));
    println!("{}", t.get_pixel(0, 0).x + t.get_pixel(3, 1).w);

    let mut v: Vec<f32> = Vec::new();
    let mut i: i32 = 0;
    while i < 8 {
        v.push(i as f32);
        i += 1;
    }
    let g = Texture3D::from_data(&v, 2, 2, 2);
    println!("{} {} {}", g.get_pixel(1, 0, 0).x, g.get_pixel(0, 1, 1).z, g.get_pixel(1, 1, 1).w);
    println!("{}", g.sample_point(Vec3::new(1.7, 1.2, 9.0)).x);
    println!("{}", g.sample_trilinear(Vec3::new(0.5, 0.5, 0.5)).x);
    println!("{}", g.sample_trilinear(Vec3::new(0.25, 0.25, 0.25)).x);
    println!("{}", g.sample_2d(Vec2::new(0.9, 0.9)).x);
    let r = Texture3D::from_rgba(&v, 2, 1, 1);
    let a = r.get_pixel(1, 0, 0);
    println!("{} {} {} {}", a.x, a.y, a.z, a.w);
    let mut e = Texture3D::new(1, 1, 2);
    e.set_pixel(0, 0, 1, Vec4::new(9.0, 8.0, 7.0, 6.0));
    println!("{} {}", e.get_pixel(0, 0, 1).y, e.get_pixel(0, 0, 0).y);
}
"#;

const TEXTURES_PRINT: &str = "4 2 32\n1 2 3 4\n0\n4\n0\n1.5\n1 6 1\n7\n3.5\n0\n3\n4 5 6 7\n8 0";

#[test]
fn textures_from_std_memory_build_and_run() {
    let main = program(
        "std",
        &format!(
            "use std::memory::{{Texture2D, Texture3D}};\nuse std::math::{{Vec2, Vec3, Vec4}};\n\n{TEXTURES}"
        ),
    );
    checks_and_prints(&main, TEXTURES_PRINT);
}

#[test]
fn textures_from_core_memory_build_and_run() {
    let main = program(
        "core",
        &format!(
            "use core::memory::{{Texture2D, Texture3D}};\nuse core::math::{{Vec2, Vec3, Vec4}};\n\n{TEXTURES}"
        ),
    );
    checks_and_prints(&main, TEXTURES_PRINT);
}

/// Control for the two tests above: without the import the names are unknown.
#[test]
fn without_the_import_a_texture_is_an_unknown_name() {
    let main = program("noimport", TEXTURES);
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(log.contains("Texture2D"), "no `Texture2D` in\n{log}");
}

#[test]
fn a_name_the_memory_module_lacks_is_still_refused() {
    let main = program(
        "arena",
        "use std::memory::{Texture2D, Arena};\n\nfn main() {}\n",
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(log.contains("[BL0611]"), "no BL0611 in\n{log}");
    assert!(log.contains("`std::memory::Arena`"), "no path in\n{log}");
    assert!(
        !log.contains("`std::memory::Texture2D`"),
        "Texture2D resolves and must not be reported\n{log}"
    );
}

const BUFFER: &str = "use std::memory::Buffer;\n\nfn main() ~ Console {\n    let mut b: Buffer<i32> = Buffer::new(4);\n    b.push(3);\n    b.push(4);\n    b.set(0, 7);\n    println!(\"{} {} {}\", b.len(), b.get(0), b.get(1));\n}\n";

const BUFFER_F64: &str = "use std::memory::Buffer;\n\nfn make() -> Buffer<f64> {\n    let mut b: Buffer<f64> = Buffer::new(0);\n    b.push(1.0);\n    b.push(2.5);\n    b.push(0.25);\n    b\n}\n\nfn main() ~ Console {\n    let b = make();\n    println!(\"{} {} {}\", b.len(), b.get(1), b.get(2));\n}\n";

const BUFFER_UNTYPED: &str = "use std::memory::Buffer;\n\nfn main() ~ Console {\n    let b = Buffer::new(4);\n    println!(\"{}\", b.len());\n}\n";

/// `Buffer<T>` from `std::memory` builds and runs: `Buffer::new` takes its
/// instance from the annotation, and the methods of `Buffer<i32>` are
/// compiled when they are called, with the field `Vec<T>` as a `Vec<i32>`
/// (issue 305; before, the C backend refused them).
#[test]
fn a_buffer_from_std_memory_builds_and_runs() {
    let main = program("buffer", BUFFER);
    checks_and_prints(&main, "2 7 4");
}

/// The same with a `Buffer<f64>` returned from a function: the instance
/// comes from the return type, and the elements keep their own type.
#[test]
fn a_buffer_of_f64_from_a_return_type_builds_and_runs() {
    let main = program("buffer_f64", BUFFER_F64);
    checks_and_prints(&main, "3 2.5 0.25");
}

/// Fail closed: with no annotation the element type cannot be told, and
/// the call stays refused.
#[test]
fn a_buffer_new_with_no_element_type_is_refused() {
    let main = program("buffer_untyped", BUFFER_UNTYPED);
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(log.contains("[BL0"), "no diagnostic in\n{log}");
}
