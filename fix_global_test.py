import io

p = "compiler/tests/global_stores.rs"
s = io.open(p, newline="").read()
nl = "\r\n" if "\r\n" in s else "\n"

old = (
    'fn an_initializer_that_is_not_a_constant_is_refused() {' + nl
    + '    let src = "static mut T: [u32; 2] = [1_u32 + 1_u32, 2_u32];\\n\\' + nl
)
new = (
    'fn an_initializer_that_is_not_a_constant_is_refused() {' + nl
    + '    let src = "fn two() -> u32 { 2_u32 }\\n\\' + nl
    + '               static mut T: [u32; 2] = [two(), 2_u32];\\n\\' + nl
)
assert s.count(old) == 1, s.count(old)
s = s.replace(old, new)

old = (
    "/// An aggregate initializer that is not a constant is refused by name; the" + nl
    + "/// global used to be zero-filled." + nl
    + "#[test]" + nl
)
new = (
    "/// Arithmetic in an element is evaluated at compile time." + nl
    + "#[test]" + nl
    + "fn an_element_written_as_arithmetic_is_evaluated() {" + nl
    + "    if !has_cc() {" + nl
    + "        return;" + nl
    + "    }" + nl
    + '    let src = "static mut T: [u32; 2] = [1_u32 + 1_u32, 2_u32 * 5_u32];\\n\\' + nl
    + '               fn main() ~ Console { unsafe { println!(\\"{} {}\\", T[0], T[1]); } }\\n";' + nl
    + '    assert_prints("arith", src, "2 10\\n");' + nl
    + "}" + nl
    + nl
    + old
)
assert s.count(old) == 1
s = s.replace(old, new)
io.open(p, "w", newline="").write(s)
print("applied")
