//! Seeded generator, reference evaluator and shrinker for `i64` expressions.
//!
//! The reference is Rust checked arithmetic. An expression whose reference
//! value is `None` (overflow, division by zero) is a trap case.

/// SplitMix64: fixed, dependency-free, the same stream on every platform.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
}

const OPS: [(Op, &str); 8] = [
    (Op::Add, "+"),
    (Op::Sub, "-"),
    (Op::Mul, "*"),
    (Op::Div, "/"),
    (Op::Rem, "%"),
    (Op::And, "&"),
    (Op::Or, "|"),
    (Op::Xor, "^"),
];

const CMPS: [&str; 6] = ["<", "<=", ">", ">=", "==", "!="];

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Lit(i64),
    Var(usize),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    /// `true` shifts left. The amount is a literal in 0..8.
    Shift(bool, Box<Expr>, u32),
    /// `if l CMP r { then } else { otherwise }`; the index picks from `CMPS`.
    If(usize, Box<Expr>, Box<Expr>, Box<Expr>, Box<Expr>),
}

/// The reference semantics. `euclid_rem` is the planted defect: remainder
/// takes the sign of the divisor's absolute value instead of the dividend.
#[derive(Clone, Copy)]
pub struct Reference {
    pub euclid_rem: bool,
}

pub const VARS: [&str; 4] = ["a", "b", "c", "d"];

/// Argument values, weighted toward the edges of `i32` and `i64`.
pub const POOL: [i64; 12] = [
    0,
    1,
    -1,
    2,
    7,
    -13,
    100,
    65_535,
    2_147_483_647,
    -2_147_483_648,
    i64::MAX,
    i64::MIN + 1,
];

pub fn generate(rng: &mut Rng, depth: u32) -> Expr {
    if depth == 0 || rng.below(8) == 0 {
        return if rng.below(3) == 0 {
            Expr::Lit(rng.below(41) as i64 - 20)
        } else {
            Expr::Var(rng.below(4) as usize)
        };
    }
    let sub = |rng: &mut Rng| Box::new(generate(rng, depth - 1));
    match rng.below(12) {
        0 => Expr::Neg(sub(rng)),
        1 => Expr::Shift(rng.below(2) == 0, sub(rng), rng.below(8) as u32),
        2 | 3 => Expr::If(
            rng.below(6) as usize,
            sub(rng),
            sub(rng),
            sub(rng),
            sub(rng),
        ),
        _ => Expr::Bin(OPS[rng.below(8) as usize].0, sub(rng), sub(rng)),
    }
}

fn binary(reference: Reference, op: Op, x: i64, y: i64) -> Option<i64> {
    match op {
        Op::Add => x.checked_add(y),
        Op::Sub => x.checked_sub(y),
        Op::Mul => x.checked_mul(y),
        Op::Div => x.checked_div(y),
        Op::Rem if reference.euclid_rem => x.checked_rem_euclid(y),
        Op::Rem => x.checked_rem(y),
        Op::And => Some(x & y),
        Op::Or => Some(x | y),
        Op::Xor => Some(x ^ y),
    }
}

fn compare(index: usize, x: i64, y: i64) -> bool {
    match index {
        0 => x < y,
        1 => x <= y,
        2 => x > y,
        3 => x >= y,
        4 => x == y,
        _ => x != y,
    }
}

/// Shifts follow Rust: the amount is below 64, a left shift drops high bits
/// without a trap, and a right shift is arithmetic.
fn shift(left: bool, x: i64, amount: u32) -> Option<i64> {
    Some(if left { x << amount } else { x >> amount })
}

pub fn eval(reference: Reference, e: &Expr, vars: &[i64; 4]) -> Option<i64> {
    let go = |e: &Expr| eval(reference, e, vars);
    match e {
        Expr::Lit(n) => Some(*n),
        Expr::Var(i) => Some(vars[*i]),
        Expr::Neg(x) => go(x)?.checked_neg(),
        Expr::Bin(op, x, y) => binary(reference, *op, go(x)?, go(y)?),
        Expr::Shift(left, x, amount) => shift(*left, go(x)?, *amount),
        Expr::If(cmp, l, r, then, otherwise) => {
            if compare(*cmp, go(l)?, go(r)?) {
                go(then)
            } else {
                go(otherwise)
            }
        }
    }
}

/// A literal the parser reads without a unary minus.
pub fn literal(n: i64) -> String {
    if n < 0 {
        format!("(0 - {})", n.unsigned_abs())
    } else {
        n.to_string()
    }
}

pub fn render(e: &Expr) -> String {
    match e {
        Expr::Lit(n) => literal(*n),
        Expr::Var(i) => VARS[*i].to_string(),
        Expr::Neg(x) => format!("(-{})", render(x)),
        Expr::Bin(op, x, y) => {
            let symbol = OPS.iter().find(|(o, _)| o == op).map_or("?", |(_, s)| *s);
            format!("({} {} {})", render(x), symbol, render(y))
        }
        Expr::Shift(left, x, amount) => {
            let symbol = if *left { "<<" } else { ">>" };
            format!("({} {} {})", render(x), symbol, amount)
        }
        Expr::If(cmp, l, r, then, otherwise) => format!(
            "(if {} {} {} {{ {} }} else {{ {} }})",
            render(l),
            CMPS[*cmp],
            render(r),
            render(then),
            render(otherwise)
        ),
    }
}

pub fn nodes(e: &Expr) -> usize {
    1 + children(e).iter().map(|c| nodes(c)).sum::<usize>()
}

fn children(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Lit(_) | Expr::Var(_) => vec![],
        Expr::Neg(x) | Expr::Shift(_, x, _) => vec![x],
        Expr::Bin(_, x, y) => vec![x, y],
        Expr::If(_, l, r, then, otherwise) => vec![l, r, then, otherwise],
    }
}

fn with_child(e: &Expr, index: usize, new: Expr) -> Expr {
    let mut out = e.clone();
    let slot: &mut Box<Expr> = match (&mut out, index) {
        (Expr::Neg(x), _) | (Expr::Shift(_, x, _), _) | (Expr::Bin(_, x, _), 0) => x,
        (Expr::Bin(_, _, y), _) => y,
        (Expr::If(_, l, _, _, _), 0) => l,
        (Expr::If(_, _, r, _, _), 1) => r,
        (Expr::If(_, _, _, then, _), 2) => then,
        (Expr::If(_, _, _, _, otherwise), _) => otherwise,
        (Expr::Lit(_) | Expr::Var(_), _) => unreachable!("leaves have no children"),
    };
    **slot = new;
    out
}

/// Every expression one step smaller: a child in place of its parent, or the
/// same reduction applied inside one child.
fn reductions(e: &Expr) -> Vec<Expr> {
    let kids = children(e);
    let mut out: Vec<Expr> = kids.iter().map(|c| (*c).clone()).collect();
    for (index, kid) in kids.iter().enumerate() {
        for smaller in reductions(kid) {
            out.push(with_child(e, index, smaller));
        }
    }
    out
}

/// Greedy shrink: take the first smaller expression that still fails, repeat.
pub fn shrink<F: FnMut(&Expr) -> bool>(start: &Expr, mut fails: F) -> Expr {
    let mut current = start.clone();
    while let Some(next) = reductions(&current).into_iter().find(|c| fails(c)) {
        current = next;
    }
    current
}
