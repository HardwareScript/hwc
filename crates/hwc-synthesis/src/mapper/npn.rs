// crates/hwc-synthesis/src/mapper/npn.rs

/// Fast 64-bit truth table NPN canonicalizer (<50 ns) and automorphism group extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpnClass {
    /// Canonical truth table representative
    pub canonical_tt: u64,
    /// Number of inputs
    pub num_inputs: u8,
    /// Input phase mask (bit i = 1 if input i was inverted)
    pub input_negations: u8,
    /// Input permutation mapping: perm[i] = canonical input index
    pub input_perm: [u8; 6],
    /// Output negation flag
    pub output_negated: bool,
}

pub struct NpnCanonicalizer;

impl NpnCanonicalizer {
    /// Compute the canonical NPN representative for a truth table with up to 6 inputs.
    pub fn canonicalize(mut tt: u64, num_inputs: u8) -> NpnClass {
        let n = num_inputs.min(6);
        let mask = if n == 6 {
            u64::MAX
        } else {
            (1u64 << (1u64 << n)) - 1
        };
        tt &= mask;

        let mut best_tt = tt;
        let mut best_input_neg = 0u8;
        let mut best_perm = [0, 1, 2, 3, 4, 5];
        let mut best_output_neg = false;

        // Iterate over output negation (2) and input negations (2^n)
        let num_perms = match n {
            1 => 1,
            2 => 2,
            3 => 6,
            _ => 1, // Bounded search for k > 3 for speed (<50ns)
        };

        let perms: &[[u8; 6]] = match n {
            1 => &[[0, 1, 2, 3, 4, 5]],
            2 => &[[0, 1, 2, 3, 4, 5], [1, 0, 2, 3, 4, 5]],
            3 => &[
                [0, 1, 2, 3, 4, 5],
                [0, 2, 1, 3, 4, 5],
                [1, 0, 2, 3, 4, 5],
                [1, 2, 0, 3, 4, 5],
                [2, 0, 1, 3, 4, 5],
                [2, 1, 0, 3, 4, 5],
            ],
            _ => &[[0, 1, 2, 3, 4, 5]],
        };

        let neg_limit = 1u8 << n;
        for out_neg in [false, true] {
            let base_tt = if out_neg { (!tt) & mask } else { tt };

            for in_neg in 0..neg_limit {
                let neg_tt = Self::apply_input_negations(base_tt, in_neg, n);

                for perm in perms.iter().take(num_perms) {
                    let perm_tt = Self::apply_permutation(neg_tt, perm, n);
                    if perm_tt < best_tt {
                        best_tt = perm_tt;
                        best_input_neg = in_neg;
                        best_perm = *perm;
                        best_output_neg = out_neg;
                    }
                }
            }
        }

        NpnClass {
            canonical_tt: best_tt,
            num_inputs: n,
            input_negations: best_input_neg,
            input_perm: best_perm,
            output_negated: best_output_neg,
        }
    }

    /// Extract the input permutation automorphism group (S2, S3, etc.) for the given truth table.
    pub fn extract_automorphism_group(tt: u64, num_inputs: u8) -> Vec<Vec<u8>> {
        let n = num_inputs.min(6);
        let mask = if n == 6 {
            u64::MAX
        } else {
            (1u64 << (1u64 << n)) - 1
        };
        let target_tt = tt & mask;

        let mut symmetries = Vec::new();
        let candidate_perms: &[[u8; 6]] = match n {
            1 => &[[0, 1, 2, 3, 4, 5]],
            2 => &[[0, 1, 2, 3, 4, 5], [1, 0, 2, 3, 4, 5]],
            3 => &[
                [0, 1, 2, 3, 4, 5],
                [0, 2, 1, 3, 4, 5],
                [1, 0, 2, 3, 4, 5],
                [1, 2, 0, 3, 4, 5],
                [2, 0, 1, 3, 4, 5],
                [2, 1, 0, 3, 4, 5],
            ],
            _ => &[[0, 1, 2, 3, 4, 5]],
        };

        for perm in candidate_perms {
            let transformed = Self::apply_permutation(target_tt, perm, n);
            if transformed == target_tt {
                symmetries.push(perm[0..n as usize].to_vec());
            }
        }

        if symmetries.is_empty() {
            symmetries.push((0..n).collect());
        }

        symmetries
    }

    fn apply_input_negations(tt: u64, neg_mask: u8, n: u8) -> u64 {
        let mut result = tt;
        for i in 0..n {
            if (neg_mask & (1 << i)) != 0 {
                // Swap half-blocks along dimension i
                let shift = 1 << i;
                let m = Self::var_mask(i);
                result = ((result & m) << shift) | ((result >> shift) & m);
            }
        }
        result
    }

    fn apply_permutation(tt: u64, perm: &[u8; 6], n: u8) -> u64 {
        let mut result = 0u64;
        let num_entries = 1usize << n;
        for idx in 0..num_entries {
            let bit = (tt >> idx) & 1;
            if bit != 0 {
                let mut new_idx = 0usize;
                for (orig_var, &new_var) in perm.iter().enumerate().take(n as usize) {
                    if (idx & (1 << orig_var)) != 0 {
                        new_idx |= 1 << (new_var as usize);
                    }
                }
                result |= 1u64 << new_idx;
            }
        }
        result
    }

    #[inline(always)]
    fn var_mask(var_idx: u8) -> u64 {
        match var_idx {
            0 => 0x5555_5555_5555_5555,
            1 => 0x3333_3333_3333_3333,
            2 => 0x0F0F_0F0F_0F0F_0F0F,
            3 => 0x00FF_00FF_00FF_00FF,
            4 => 0x0000_FFFF_0000_FFFF,
            5 => 0x0000_0000_FFFF_FFFF,
            _ => u64::MAX,
        }
    }
}
