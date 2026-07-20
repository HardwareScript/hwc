//! Morton ordering (Z-order curve) for cache-friendly spatial sorting.

use crate::geometry_router::spatial_index::IndexedSegment;

/// Compute a 2D Morton code (Z-order curve) for cache-friendly spatial sorting.
///
/// Interleaves the bits of x and y coordinates to produce a single u64 value.
/// Positions close in 2D space produce similar Morton codes, yielding
/// excellent L1/L2 cache hit rates during the sweep.
#[inline]
pub fn compute_morton_code(x: i64, y: i64) -> u64 {
    let xu = (x as u64) & 0xFFFFFFFF;
    let yu = (y as u64) & 0xFFFFFFFF;
    spread_bits_2d(xu) | (spread_bits_2d(yu) << 1)
}

/// Spread bits of a 32-bit value so each bit is separated by one zero.
/// Core primitive for 2D Morton encoding.
#[inline(always)]
fn spread_bits_2d(mut v: u64) -> u64 {
    v &= 0xFFFFFFFF;
    v = (v | (v << 16)) & 0x0000FFFF0000FFFF;
    v = (v | (v << 8)) & 0x00FF00FF00FF00FF;
    v = (v | (v << 4)) & 0x0F0F0F0F0F0F0F0F;
    v = (v | (v << 2)) & 0x3333333333333333;
    v = (v | (v << 1)) & 0x5555555555555555;
    v
}

/// Sort segments by Morton code for cache-friendly access patterns.
///
/// Uses each segment's center point to compute the Morton code, ensuring
/// spatially proximate segments are adjacent in the sorted array.
#[inline]
pub fn sort_segments_by_morton(segments: &mut [IndexedSegment]) {
    segments.sort_by_key(|s| {
        let center = s.center();
        compute_morton_code(center.x, center.y)
    });
}
