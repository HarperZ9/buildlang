// Unsafe and macro_rules must be refused
macro_rules! gen { () => { 1 }; }
fn main() {
    let x: i64 = 1;
    unsafe { let y: i64 = x; }
}
