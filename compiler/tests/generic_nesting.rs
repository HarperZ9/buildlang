//! A generic type instantiated inside its own methods ends in a refusal.
//!
//! Issue 303: `buildc check` never returned on a trait default method that
//! returns a generic struct around `Self` when that struct implements the
//! trait. Each instance of `Chain` was compiled with its methods as soon as
//! it was named, `chain` named `Chain<Chain<..>, R>`, and memory grew by a
//! few hundred megabytes a second until the process was killed.
//!
//! Since the methods of a generic instance are compiled at their first
//! call, the reproduction builds. A method that calls itself on a larger
//! instance still has no end, and is refused.
//!
//! Every check here runs under a deadline, and on Unix under an address
//! space limit, so a compiler that hangs again fails the test in seconds and
//! cannot take the machine down.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(20);

const WRAPS_SELF: &str = r#"trait Read {
    fn read(self: &mut Self) -> i32;

    fn chain<R: Read>(self, next: R) -> Chain<Self, R> where Self: Sized {
        Chain { first: self, second: next, done_first: false }
    }
}

struct Chain<R1: Read, R2: Read> {
    first: R1,
    second: R2,
    done_first: bool,
}

struct Zero {
    n: i32,
}

impl Read for Zero {
    fn read(self: &mut Self) -> i32 {
        self.n
    }
}

impl<R1: Read, R2: Read> Read for Chain<R1, R2> {
    fn read(self: &mut Self) -> i32 {
        0
    }
}

fn main() ~ Console {
    let mut z = Zero { n: 3 };
    println!("{}", z.read());
}
"#;

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-nesting-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, text).expect("source file");
    main
}

/// `buildc <command> <file>` under the deadline. `None` when it had to be
/// killed.
fn buildc_within_deadline(command_name: &str, file: &Path) -> Option<(bool, String)> {
    let dir = file.parent().expect("program folder");
    let out_path = dir.join("out.txt");
    let err_path = dir.join("err.txt");
    let stdout = std::fs::File::create(&out_path).expect("stdout file");
    let stderr = std::fs::File::create(&err_path).expect("stderr file");
    let buildc = env!("CARGO_BIN_EXE_buildc");

    // Two gibibytes of address space on Unix: the hang reached that in
    // about six seconds, and a check of this program needs a few megabytes.
    #[cfg(unix)]
    let mut command = {
        let mut c = Command::new("sh");
        c.arg("-c")
            .arg("ulimit -v 2097152; exec \"$0\" \"$1\" \"$2\"")
            .arg(buildc)
            .arg(command_name)
            .arg(file);
        c
    };
    #[cfg(not(unix))]
    let mut command = {
        let mut c = Command::new(buildc);
        c.arg(command_name).arg(file);
        c
    };

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("buildc starts");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("buildc is waited for") {
            break status;
        }
        if started.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let text = format!(
        "{}{}",
        std::fs::read_to_string(&out_path).unwrap_or_default(),
        std::fs::read_to_string(&err_path).unwrap_or_default()
    );
    Some((status.success(), text))
}

fn check_within_deadline(file: &Path) -> Option<(bool, String)> {
    buildc_within_deadline("check", file)
}

/// The reproduction of issue 303 passes the check: the methods of `Chain`
/// are compiled when they are called, and nothing calls `chain`.
#[test]
fn a_default_method_that_wraps_self_passes_the_check() {
    let main = program("wraps", WRAPS_SELF);
    let Some((passed, text)) = check_within_deadline(&main) else {
        panic!("`buildc check` did not return within {DEADLINE:?} (issue 303)");
    };
    assert!(
        passed,
        "the check failed
{text}"
    );
}

/// A method of `W<T>` that calls itself on `W<W<T>>` needs an instance
/// inside an instance without end. It is refused within the deadline.
const CALLS_A_LARGER_SELF: &str = r#"struct W<T> {
    v: T,
}

impl<T> W<T> {
    fn deeper(self, n: i32) -> i32 {
        if n == 0 {
            return 0;
        }
        let w = W { v: self };
        w.deeper(n - 1) + 1
    }
}

fn main() ~ Console {
    let w = W { v: 1 };
    println!("{}", w.deeper(3));
}
"#;

#[test]
fn a_method_that_calls_itself_on_a_larger_instance_is_refused() {
    let main = program("larger", CALLS_A_LARGER_SELF);
    let Some((passed, text)) = check_within_deadline(&main) else {
        panic!("`buildc check` did not return within {DEADLINE:?}");
    };
    assert!(
        !passed,
        "the check passed
{text}"
    );
    for part in ["BL0901", "generic type `W`", "without end"] {
        assert!(
            text.contains(part),
            "no {part:?} in
{text}"
        );
    }
    assert!(
        !text.contains("memory allocation") && !text.contains("panicked"),
        "the compiler crashed
{text}"
    );
}

/// Control: the same program without `impl Read for Chain` has no chain of
/// instances and passes, as it did before.
#[test]
fn the_same_program_without_the_wrapping_impl_passes() {
    let start = WRAPS_SELF.find("impl<R1: Read, R2: Read>").expect("impl");
    let end = WRAPS_SELF.find("fn main()").expect("main");
    let text = format!("{}{}", &WRAPS_SELF[..start], &WRAPS_SELF[end..]);
    let main = program("control", &text);
    let Some((passed, log)) = check_within_deadline(&main) else {
        panic!("the control did not return within {DEADLINE:?}");
    };
    assert!(passed, "the control failed\n{log}");
}
