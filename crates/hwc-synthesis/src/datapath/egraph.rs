// crates/hwc-synthesis/src/datapath/egraph.rs

use crate::aig::arena::{Edge, PackedAigGraph};
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Word-level arithmetic and datapath expression DAG.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WordExpr {
    Signal(CompactString, u16), // Name, Bit-Width
    Constant(u128, u16),        // Value, Bit-Width
    Add(Box<WordExpr>, Box<WordExpr>),
    Sub(Box<WordExpr>, Box<WordExpr>),
    Mul(Box<WordExpr>, Box<WordExpr>),
    ShiftLeft(Box<WordExpr>, u16),
    ShiftRight(Box<WordExpr>, u16),
    BitwiseAnd(Box<WordExpr>, Box<WordExpr>),
    BitwiseOr(Box<WordExpr>, Box<WordExpr>),
    BitwiseXor(Box<WordExpr>, Box<WordExpr>),
    Concat(Box<WordExpr>, Box<WordExpr>),
    Extract(Box<WordExpr>, u16, u16), // High bit, Low bit (inclusive)
    Equal(Box<WordExpr>, Box<WordExpr>),
    NotEqual(Box<WordExpr>, Box<WordExpr>),
    LessThan(Box<WordExpr>, Box<WordExpr>),
    Mux(Box<WordExpr>, Box<WordExpr>, Box<WordExpr>), // Cond, Then, Else
}

impl WordExpr {
    /// Return the bit-width of this word expression.
    pub fn bit_width(&self) -> u16 {
        match self {
            WordExpr::Signal(_, width) | WordExpr::Constant(_, width) => *width,
            WordExpr::Add(a, _)
            | WordExpr::Sub(a, _)
            | WordExpr::Mul(a, _)
            | WordExpr::ShiftLeft(a, _)
            | WordExpr::ShiftRight(a, _)
            | WordExpr::BitwiseAnd(a, _)
            | WordExpr::BitwiseOr(a, _)
            | WordExpr::BitwiseXor(a, _) => a.bit_width(),
            WordExpr::Concat(a, b) => a.bit_width() + b.bit_width(),
            WordExpr::Extract(_, high, low) => high - low + 1,
            WordExpr::Equal(_, _) | WordExpr::NotEqual(_, _) | WordExpr::LessThan(_, _) => 1,
            WordExpr::Mux(_, then_e, _) => then_e.bit_width(),
        }
    }

    /// Check if this expression evaluates to a constant power of 2.
    pub fn is_constant_power_of_two(&self) -> Option<u16> {
        if let WordExpr::Constant(v, _) = self {
            if *v > 0 && (*v & (*v - 1)) == 0 {
                return Some(v.trailing_zeros() as u16);
            }
        }
        None
    }

    /// Applies algebraic term-rewriting rules to normalize datapath structures and eliminate phase-ordering traps.
    pub fn optimize_algebraic(&self) -> Self {
        match self {
            // Constant Shifting: x * 2^N -> x << N
            WordExpr::Mul(a, b) => {
                let opt_a = a.optimize_algebraic();
                let opt_b = b.optimize_algebraic();
                if let Some(shift) = opt_b.is_constant_power_of_two() {
                    WordExpr::ShiftLeft(Box::new(opt_a), shift)
                } else if let Some(shift) = opt_a.is_constant_power_of_two() {
                    WordExpr::ShiftLeft(Box::new(opt_b), shift)
                } else {
                    WordExpr::Mul(Box::new(opt_a), Box::new(opt_b))
                }
            }
            // Additive Inversion: (a + b) - b -> a, (a + b) - a -> b
            WordExpr::Sub(a, b) => {
                let opt_a = a.optimize_algebraic();
                let opt_b = b.optimize_algebraic();
                if let WordExpr::Add(x, y) = &opt_a {
                    if **y == opt_b {
                        return *x.clone();
                    }
                    if **x == opt_b {
                        return *y.clone();
                    }
                }
                // x - 0 -> x
                if let WordExpr::Constant(0, _) = &opt_b {
                    return opt_a;
                }
                WordExpr::Sub(Box::new(opt_a), Box::new(opt_b))
            }
            // x + 0 -> x, 0 + x -> x
            WordExpr::Add(a, b) => {
                let opt_a = a.optimize_algebraic();
                let opt_b = b.optimize_algebraic();
                if let WordExpr::Constant(0, _) = &opt_b {
                    return opt_a;
                }
                if let WordExpr::Constant(0, _) = &opt_a {
                    return opt_b;
                }
                WordExpr::Add(Box::new(opt_a), Box::new(opt_b))
            }
            // x ^ x -> 0, x ^ 0 -> x
            WordExpr::BitwiseXor(a, b) => {
                let opt_a = a.optimize_algebraic();
                let opt_b = b.optimize_algebraic();
                if opt_a == opt_b {
                    return WordExpr::Constant(0, opt_a.bit_width());
                }
                if let WordExpr::Constant(0, _) = &opt_b {
                    return opt_a;
                }
                if let WordExpr::Constant(0, _) = &opt_a {
                    return opt_b;
                }
                WordExpr::BitwiseXor(Box::new(opt_a), Box::new(opt_b))
            }
            // MUX(cond, x, x) -> x
            WordExpr::Mux(c, t, e) => {
                let opt_c = c.optimize_algebraic();
                let opt_t = t.optimize_algebraic();
                let opt_e = e.optimize_algebraic();
                if opt_t == opt_e {
                    return opt_t;
                }
                if let WordExpr::Constant(1, _) = &opt_c {
                    return opt_t;
                }
                if let WordExpr::Constant(0, _) = &opt_c {
                    return opt_e;
                }
                WordExpr::Mux(Box::new(opt_c), Box::new(opt_t), Box::new(opt_e))
            }
            _ => self.clone(),
        }
    }

    /// Lower / Bit-blast the word-level expression into an array of bit-level AIG edges.
    pub fn bit_blast(
        &self,
        aig: &mut PackedAigGraph,
        signals: &FxHashMap<CompactString, Vec<Edge>>,
    ) -> Vec<Edge> {
        match self {
            WordExpr::Signal(name, width) => {
                if let Some(bits) = signals.get(name) {
                    bits.clone()
                } else {
                    // Fallback: create fresh input edges
                    let mut bits = Vec::with_capacity(*width as usize);
                    for i in 0..*width {
                        let bit_name = format!("{}_{}", name, i);
                        bits.push(aig.add_input(&bit_name));
                    }
                    bits
                }
            }
            WordExpr::Constant(val, width) => {
                let mut bits = Vec::with_capacity(*width as usize);
                for i in 0..*width {
                    let bit = (*val >> i) & 1;
                    bits.push(if bit == 1 { Edge::ONE } else { Edge::ZERO });
                }
                bits
            }
            WordExpr::BitwiseAnd(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let len = bits_a.len().min(bits_b.len());
                (0..len).map(|i| aig.add_and(bits_a[i], bits_b[i])).collect()
            }
            WordExpr::BitwiseOr(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let len = bits_a.len().min(bits_b.len());
                (0..len).map(|i| aig.add_or(bits_a[i], bits_b[i])).collect()
            }
            WordExpr::BitwiseXor(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let len = bits_a.len().min(bits_b.len());
                (0..len).map(|i| aig.add_xor(bits_a[i], bits_b[i])).collect()
            }
            WordExpr::ShiftLeft(a, shift) => {
                let bits_a = a.bit_blast(aig, signals);
                let width = bits_a.len();
                let s = *shift as usize;
                let mut res = vec![Edge::ZERO; width];
                for i in 0..width {
                    if i >= s {
                        res[i] = bits_a[i - s];
                    }
                }
                res
            }
            WordExpr::ShiftRight(a, shift) => {
                let bits_a = a.bit_blast(aig, signals);
                let width = bits_a.len();
                let s = *shift as usize;
                let mut res = vec![Edge::ZERO; width];
                for i in 0..width {
                    if i + s < width {
                        res[i] = bits_a[i + s];
                    }
                }
                res
            }
            WordExpr::Add(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let width = bits_a.len().max(bits_b.len());
                let mut sum_bits = Vec::with_capacity(width);
                let mut carry = Edge::ZERO;

                for i in 0..width {
                    let a_bit = bits_a.get(i).copied().unwrap_or(Edge::ZERO);
                    let b_bit = bits_b.get(i).copied().unwrap_or(Edge::ZERO);

                    // Full Adder: Sum = a ^ b ^ carry
                    let ab_xor = aig.add_xor(a_bit, b_bit);
                    let sum = aig.add_xor(ab_xor, carry);
                    sum_bits.push(sum);

                    // Carry_out = (a & b) | (carry & ab_xor)
                    let ab_and = aig.add_and(a_bit, b_bit);
                    let c_and = aig.add_and(carry, ab_xor);
                    carry = aig.add_or(ab_and, c_and);
                }
                sum_bits
            }
            WordExpr::Sub(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let width = bits_a.len().max(bits_b.len());
                let mut diff_bits = Vec::with_capacity(width);
                let mut borrow = Edge::ZERO;

                for i in 0..width {
                    let a_bit = bits_a.get(i).copied().unwrap_or(Edge::ZERO);
                    let b_bit = bits_b.get(i).copied().unwrap_or(Edge::ZERO);

                    // Full Subtractor: Diff = a ^ b ^ borrow
                    let ab_xor = aig.add_xor(a_bit, b_bit);
                    let diff = aig.add_xor(ab_xor, borrow);
                    diff_bits.push(diff);

                    // Borrow_out = (!a & b) | (!ab_xor & borrow)
                    let not_a_and_b = aig.add_and(a_bit.not(), b_bit);
                    let not_ab_and_borrow = aig.add_and(ab_xor.not(), borrow);
                    borrow = aig.add_or(not_a_and_b, not_ab_and_borrow);
                }
                diff_bits
            }
            WordExpr::Mul(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let width = bits_a.len().max(bits_b.len());
                let mut acc = vec![Edge::ZERO; width];

                // Shift-and-add multiplier
                for j in 0..bits_b.len() {
                    let b_j = bits_b[j];
                    let mut partial = vec![Edge::ZERO; width];
                    for i in 0..bits_a.len() {
                        if i + j < width {
                            partial[i + j] = aig.add_and(bits_a[i], b_j);
                        }
                    }

                    // Add partial product to acc
                    let mut carry = Edge::ZERO;
                    for k in 0..width {
                        let partial_k = partial[k];
                        let acc_k = acc[k];
                        let sum_ab = aig.add_xor(acc_k, partial_k);
                        let sum = aig.add_xor(sum_ab, carry);
                        let c1 = aig.add_and(acc_k, partial_k);
                        let c2 = aig.add_and(carry, sum_ab);
                        carry = aig.add_or(c1, c2);
                        acc[k] = sum;
                    }
                }
                acc
            }
            WordExpr::Concat(a, b) => {
                let mut bits_b = b.bit_blast(aig, signals);
                let bits_a = a.bit_blast(aig, signals);
                // Concatenation: low bits first (b), then high bits (a)
                bits_b.extend(bits_a);
                bits_b
            }
            WordExpr::Extract(a, high, low) => {
                let bits_a = a.bit_blast(aig, signals);
                let h = *high as usize;
                let l = *low as usize;
                if l <= h && l < bits_a.len() {
                    let end = (h + 1).min(bits_a.len());
                    bits_a[l..end].to_vec()
                } else {
                    vec![Edge::ZERO]
                }
            }
            WordExpr::Equal(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let len = bits_a.len().max(bits_b.len());
                let mut eq_bit = Edge::ONE;

                for i in 0..len {
                    let a_bit = bits_a.get(i).copied().unwrap_or(Edge::ZERO);
                    let b_bit = bits_b.get(i).copied().unwrap_or(Edge::ZERO);
                    // Bit match: NOT(a ^ b)
                    let match_i = aig.add_xor(a_bit, b_bit).not();
                    eq_bit = aig.add_and(eq_bit, match_i);
                }
                vec![eq_bit]
            }
            WordExpr::NotEqual(a, b) => {
                let eq_bits = WordExpr::Equal(a.clone(), b.clone()).bit_blast(aig, signals);
                vec![eq_bits[0].not()]
            }
            WordExpr::LessThan(a, b) => {
                let bits_a = a.bit_blast(aig, signals);
                let bits_b = b.bit_blast(aig, signals);
                let len = bits_a.len().max(bits_b.len());
                let mut less = Edge::ZERO;

                for i in 0..len {
                    let a_bit = bits_a.get(i).copied().unwrap_or(Edge::ZERO);
                    let b_bit = bits_b.get(i).copied().unwrap_or(Edge::ZERO);
                    // less_new = (!a & b) | (!(a ^ b) & less_old)
                    let a_lt_b = aig.add_and(a_bit.not(), b_bit);
                    let a_eq_b = aig.add_xor(a_bit, b_bit).not();
                    let eq_and_prev = aig.add_and(a_eq_b, less);
                    less = aig.add_or(a_lt_b, eq_and_prev);
                }
                vec![less]
            }
            WordExpr::Mux(cond, then_e, else_e) => {
                let cond_bits = cond.bit_blast(aig, signals);
                let then_bits = then_e.bit_blast(aig, signals);
                let else_bits = else_e.bit_blast(aig, signals);
                let cond_edge = cond_bits.first().copied().unwrap_or(Edge::ZERO);
                let width = then_bits.len().max(else_bits.len());

                let mut out_bits = Vec::with_capacity(width);
                for i in 0..width {
                    let t = then_bits.get(i).copied().unwrap_or(Edge::ZERO);
                    let e = else_bits.get(i).copied().unwrap_or(Edge::ZERO);
                    out_bits.push(aig.add_mux(cond_edge, t, e));
                }
                out_bits
            }
        }
    }
}
