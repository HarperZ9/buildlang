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
    assert!(
        check.status.success(),
        "the check failed
{}",
        text(&check)
    );

    let run = buildc("run", main);
    let log = text(&run);
    // A machine with no C compiler cannot answer. CI has one, and there a
    // missing compiler is a failure, not a pass.
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler
{log}"
        );
        return;
    }
    assert_eq!(run.status.code(), Some(0), "{log}");
    let stdout = String::from_utf8_lossy(&run.stdout).replace(
        "
", "
",
    );
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

const TEXTURES_PRINT: &str = "4 2 32
1 2 3 4
0
4
0
1.5
1 6 1
7
3.5
0
3
4 5 6 7
8 0";

#[test]
fn textures_from_std_memory_build_and_run() {
    let main = program(
        "std",
        &format!(
            "use std::memory::{{Texture2D, Texture3D}};
use std::math::{{Vec2, Vec3, Vec4}};

{TEXTURES}"
        ),
    );
    checks_and_prints(&main, TEXTURES_PRINT);
}

#[test]
fn textures_from_core_memory_build_and_run() {
    let main = program(
        "core",
        &format!(
            "use core::memory::{{Texture2D, Texture3D}};
use core::math::{{Vec2, Vec3, Vec4}};

{TEXTURES}"
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
    assert!(
        !check.status.success(),
        "the check passed
{log}"
    );
    assert!(
        log.contains("Texture2D"),
        "no `Texture2D` in
{log}"
    );
}

#[test]
fn a_name_the_memory_module_lacks_is_still_refused() {
    let main = program(
        "arena",
        "use std::memory::{Texture2D, Arena};

fn main() {}
",
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(
        !check.status.success(),
        "the check passed
{log}"
    );
    assert!(
        log.contains("[BL0611]"),
        "no BL0611 in
{log}"
    );
    assert!(
        log.contains("`std::memory::Arena`"),
        "no path in
{log}"
    );
    assert!(
        !log.contains("`std::memory::Texture2D`"),
        "Texture2D resolves and must not be reported
{log}"
    );
}

const BUFFER: &str = "use std::memory::Buffer;

fn main() ~ Console {
    let mut b: Buffer<i32> = Buffer::new(4);
    b.push(3);
    b.push(4);
    b.set(0, 7);
    println!(\"{} {} {}\", b.len(), b.get(0), b.get(1));
}
";

/// `Buffer<T>` is declared and imports resolve, and a program that calls
/// its methods is refused by the C backend: the methods of a generic type
/// whose field is `Vec<T>` are not compiled yet (issue 305). The test is
/// strict in both directions: the import must resolve, and the day the
/// program builds this test fails and becomes a run test that expects
/// `2 7 4`.
#[test]
fn a_buffer_resolves_and_its_methods_are_refused_by_the_c_backend() {
    let main = program("buffer", BUFFER);
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(
        !log.contains("[BL0611]"),
        "the import did not resolve
{log}"
    );
    assert!(
        !check.status.success(),
        "the check passed: turn this into a run test (issue 305)
{log}"
    );
    assert!(
        log.contains("[BL0901]"),
        "no C backend refusal in
{log}"
    );
}
