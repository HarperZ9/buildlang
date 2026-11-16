"""A loop left by a plain break finishes with no value. Run from the repo root."""
import io

p = "compiler/src/types/infer.rs"
s = io.open(p, newline="").read()
nl = "\r\n" if "\r\n" in s else "\n"


def rep(a, b, count=1):
    global s
    a = a.replace("\n", nl)
    b = b.replace("\n", nl)
    assert s.count(a) == count, (a[:70], s.count(a))
    s = s.replace(a, b)


rep(
    """    fn push_loop_label(&mut self, label: Option<&ast::Ident>) {
        self.loop_label_stack
            .push(label.map(|l| l.as_str().to_string()));
    }

    /// Leave a loop, dropping its label entry.
    fn pop_loop_label(&mut self) {
        self.loop_label_stack.pop();
    }
""",
    """    fn push_loop_label(&mut self, label: Option<&ast::Ident>) {
        self.loop_label_stack
            .push(label.map(|l| l.as_str().to_string()));
        self.loop_break_kinds.push((false, false));
    }

    /// Leave a loop, dropping its label entry. Returns whether a `break`
    /// without a value and a `break` with a value left this loop.
    fn pop_loop_label(&mut self) -> (bool, bool) {
        self.loop_label_stack.pop();
        self.loop_break_kinds.pop().unwrap_or((false, false))
    }
""",
)

# infer_loop: the function body from its signature to its end.
start = s.index("    fn infer_loop(&mut self, body: &ast::Block, label: Option<&ast::Ident>) -> Ty {")
end = s.index("    fn infer_while(", start)
body = s[start:end]
a = "        self.pop_loop_label();" + nl
assert body.count(a) == 1
body = body.replace(a, "        let (plain_break, value_break) = self.pop_loop_label();" + nl)
a = ("        // Loop returns never (unless break with value)" + nl + "        Ty::never()" + nl)
assert body.count(a) == 1
body = body.replace(
    a,
    (
        """        // A `loop` left by a plain `break` has finished with no value: code
        // after it runs, so a function with a value cannot end on it. With no
        // `break` it never finishes. A `break` with a value keeps the earlier
        // typing until the value's type is carried through.
        if plain_break && !value_break {
            Ty::unit()
        } else {
            Ty::never()
        }
"""
    ).replace("\n", nl),
)
s = s[:start] + body + s[end:]

rep(
    """        if let Some(expr) = value {
            let _ = self.infer_expr(expr);
        }

        if self.ctx.in_loop() {
            self.record_loop_break_source_snapshot();
        }
""",
    """        if let Some(expr) = value {
            let _ = self.infer_expr(expr);
        }
        // Note on the loop this `break` leaves which kind of `break` it was.
        let target = match label {
            Some(l) => self
                .loop_label_stack
                .iter()
                .rposition(|have| have.as_deref() == Some(l.as_str())),
            None => self.loop_break_kinds.len().checked_sub(1),
        };
        if let Some(kinds) = target.and_then(|i| self.loop_break_kinds.get_mut(i)) {
            if value.is_some() {
                kinds.1 = true;
            } else {
                kinds.0 = true;
            }
        }

        if self.ctx.in_loop() {
            self.record_loop_break_source_snapshot();
        }
""",
)
rep(
    "    loop_break_source_frames: Vec<BreakSourceFrame>,\n",
    "    loop_break_source_frames: Vec<BreakSourceFrame>,\n"
    "    /// One entry for each enclosing loop, beside `loop_label_stack`: whether a\n"
    "    /// `break` without a value, and a `break` with a value, leaves that loop.\n"
    "    loop_break_kinds: Vec<(bool, bool)>,\n",
)
rep(
    "            loop_break_source_frames: Vec::new(),\n",
    "            loop_break_source_frames: Vec::new(),\n            loop_break_kinds: Vec::new(),\n",
    2,
)
io.open(p, "w", newline="").write(s)
print("applied")
