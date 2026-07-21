//! Integer Geometry Math Utilities
//!
//! Deterministic, non-overflowing integer math for geometry operations.
//! All functions use only integer arithmetic for bit-identical builds
//! across platforms.

/// Deterministic Newton-Heron integer square root.
///
/// Computes floor(sqrt(n)) using only integer arithmetic.
/// Guaranteed to converge for all u128 values without overflow.
///
/// Uses bit-length based initial estimate for fast convergence.
#[inline(always)]
pub fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    // Initial estimate based on bit position: sqrt(2^b) ~ 2^(b/2)
    let bits = 128 - n.leading_zeros();
    let mut x = 1u128 << ((bits + 1) / 2);
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Integer square root of a u64 value, returning u64.
#[inline(always)]
pub fn integer_sqrt_u64(n: u64) -> u64 {
    integer_sqrt(n as u128) as u64
}

/// Euclidean distance between two points using integer sqrt.
///
/// Returns the integer approximation of the Euclidean distance.
#[inline]
pub fn integer_distance(x1: i64, y1: i64, x2: i64, y2: i64) -> i64 {
    let dx = (x1 - x2) as i128;
    let dy = (y1 - y2) as i128;
    let d2 = (dx * dx + dy * dy) as u128;
    integer_sqrt(d2) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_sqrt_zero() {
        assert_eq!(integer_sqrt(0), 0);
    }

    #[test]
    fn test_integer_sqrt_one() {
        assert_eq!(integer_sqrt(1), 1);
    }

    #[test]
    fn test_integer_sqrt_perfect_squares() {
        assert_eq!(integer_sqrt(4), 2);
        assert_eq!(integer_sqrt(9), 3);
        assert_eq!(integer_sqrt(16), 4);
        assert_eq!(integer_sqrt(25), 5);
        assert_eq!(integer_sqrt(100), 10);
        assert_eq!(integer_sqrt(10000), 100);
    }

    #[test]
    fn test_integer_sqrt_non_perfect() {
        assert_eq!(integer_sqrt(2), 1);
        assert_eq!(integer_sqrt(3), 1);
        assert_eq!(integer_sqrt(5), 2);
        assert_eq!(integer_sqrt(8), 2);
        assert_eq!(integer_sqrt(10), 3);
    }

    #[test]
    fn test_integer_sqrt_large_values() {
        assert_eq!(integer_sqrt(u128::MAX), 18446744073709551615);
    }

    #[test]
    fn test_integer_distance_same_point() {
        assert_eq!(integer_distance(0, 0, 0, 0), 0);
    }

    #[test]
    fn test_integer_distance_horizontal() {
        assert_eq!(integer_distance(0, 0, 3, 0), 3);
    }

    #[test]
    fn test_integer_distance_3_4_5() {
        assert_eq!(integer_distance(0, 0, 3, 4), 5);
    }

    #[test]
    fn test_integer_distance_symmetric() {
        let d1 = integer_distance(1, 2, 4, 6);
        let d2 = integer_distance(4, 6, 1, 2);
        assert_eq!(d1, d2);
    }
}
