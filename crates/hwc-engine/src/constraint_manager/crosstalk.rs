//! EMI and crosstalk constraint generation.
//!
//! This module implements Phase 1.4 of the constraint generation pipeline,
//! calculating parallel length between traces and applying crosstalk penalties.

use crate::geometry::Point3D;

// ============================================================================
// Phase 1.4: EMI and Crosstalk Constraint Generation
// ============================================================================

/// Calculate parallel length between two routes.
///
/// Finds segments that run parallel within threshold distance and counts
/// the total length where both traces are parallel.
///
/// **Algorithm**:
/// 1. For each segment in route A
/// 2. For each segment in route B
/// 3. Check if segments are parallel (same direction)
/// 4. Check if segments are within threshold distance
/// 5. Count overlapping parallel length
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 300-400, Translation 3)
///
/// # Arguments
/// * `route_a` - First route's path points
/// * `route_b` - Second route's path points
/// * `threshold_distance_nm` - Maximum distance to consider "parallel"
///
/// # Returns
/// Total parallel length in nanometers
///
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::calculate_parallel_length;
/// use hwc_engine::Point3D;
///
/// // Two parallel traces running north-south, 1mm apart
/// let route_a = vec![
///     Point3D::new(0, 0, 0),
///     Point3D::new(0, 10_000_000, 0),  // 10mm north
/// ];
/// let route_b = vec![
///     Point3D::new(1_000_000, 0, 0),   // 1mm east of route_a
///     Point3D::new(1_000_000, 10_000_000, 0),  // 10mm north, parallel to route_a
/// ];
///
/// let parallel = calculate_parallel_length(&route_a, &route_b, 2_000_000);
/// assert!(parallel > 0);  // Routes are parallel and within threshold
/// ```
pub fn calculate_parallel_length(
    route_a: &[Point3D],
    route_b: &[Point3D],
    threshold_distance_nm: i64,
) -> i64 {
    if route_a.len() < 2 || route_b.len() < 2 {
        return 0; // Need at least 2 points to form a segment
    }

    let mut total_parallel_length = 0i64;

    // For each segment in route A
    for i in 0..route_a.len() - 1 {
        let a_start = route_a[i];
        let a_end = route_a[i + 1];

        // Determine direction of segment A
        let a_direction = get_segment_direction(a_start, a_end);

        // For each segment in route B
        for j in 0..route_b.len() - 1 {
            let b_start = route_b[j];
            let b_end = route_b[j + 1];

            // Determine direction of segment B
            let b_direction = get_segment_direction(b_start, b_end);

            // Check if segments are parallel (same direction)
            if a_direction != b_direction {
                continue; // Not parallel
            }

            // Check if segments are within threshold distance
            let min_distance = calculate_segment_distance(a_start, a_end, b_start, b_end);
            if min_distance > threshold_distance_nm {
                continue; // Too far apart
            }

            // Calculate overlapping parallel length
            let overlap = calculate_segment_overlap(a_start, a_end, b_start, b_end, a_direction);
            total_parallel_length += overlap;
        }
    }

    total_parallel_length
}

/// Get the primary direction of a segment.
#[inline]
fn get_segment_direction(start: Point3D, end: Point3D) -> Option<crate::geometry::Direction> {
    use crate::geometry::Direction;

    let dz = end.z - start.z;
    let dx = end.x - start.x;
    let dy = end.y - start.y;

    // Determine primary direction (Manhattan routing)
    if dz.abs() > dx.abs() && dz.abs() > dy.abs() {
        Some(if dz > 0 {
            Direction::Up
        } else {
            Direction::Down
        })
    } else if dx.abs() > dy.abs() {
        Some(if dx > 0 {
            Direction::East
        } else {
            Direction::West
        })
    } else if dy.abs() > 0 {
        Some(if dy > 0 {
            Direction::North
        } else {
            Direction::South
        })
    } else {
        None // Zero-length segment
    }
}

/// Calculate minimum distance between two segments.
#[inline]
fn calculate_segment_distance(
    a_start: Point3D,
    a_end: Point3D,
    b_start: Point3D,
    b_end: Point3D,
) -> i64 {
    // For parallel segments, calculate perpendicular distance
    // Simplified: use minimum distance between any two points
    let d1 = a_start.manhattan_distance(&b_start);
    let d2 = a_start.manhattan_distance(&b_end);
    let d3 = a_end.manhattan_distance(&b_start);
    let d4 = a_end.manhattan_distance(&b_end);

    d1.min(d2).min(d3).min(d4)
}

/// Calculate overlapping length of two parallel segments.
#[inline]
fn calculate_segment_overlap(
    a_start: Point3D,
    a_end: Point3D,
    b_start: Point3D,
    b_end: Point3D,
    direction: Option<crate::geometry::Direction>,
) -> i64 {
    use crate::geometry::Direction;

    let dir = match direction {
        Some(d) => d,
        None => return 0,
    };

    // Get the coordinate along the direction axis
    let (a_min, a_max) = match dir {
        Direction::North | Direction::South => (a_start.y.min(a_end.y), a_start.y.max(a_end.y)),
        Direction::East | Direction::West => (a_start.x.min(a_end.x), a_start.x.max(a_end.x)),
        Direction::Up | Direction::Down => (a_start.z.min(a_end.z), a_start.z.max(a_end.z)),
    };

    let (b_min, b_max) = match dir {
        Direction::North | Direction::South => (b_start.y.min(b_end.y), b_start.y.max(b_end.y)),
        Direction::East | Direction::West => (b_start.x.min(b_end.x), b_start.x.max(b_end.x)),
        Direction::Up | Direction::Down => (b_start.z.min(b_end.z), b_start.z.max(b_end.z)),
    };

    // Calculate overlap
    let overlap_start = a_min.max(b_min);
    let overlap_end = a_max.min(b_max);

    if overlap_end > overlap_start {
        overlap_end - overlap_start
    } else {
        0
    }
}

/// Calculate crosstalk penalty for pathfinding cost.
///
/// Applies exponential penalty when parallel length exceeds maximum allowed.
/// This discourages the router from creating long parallel runs that cause
/// electromagnetic interference.
///
/// **Algorithm**:
/// 1. Calculate ratio of actual to maximum parallel length
/// 2. If under limit, no penalty
/// 3. If over limit, apply exponential penalty
/// 4. Formula: `penalty = 1000 + ratio + (ratio * ratio) / 2000`
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 300-400, Translation 3)
///
/// # Arguments
/// * `parallel_length_nm` - Actual parallel length in nanometers
/// * `max_parallel_nm` - Maximum allowed parallel length in nanometers
///
/// # Returns
/// Cost penalty for pathfinding (0 if under limit, exponential if over)
///
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::calculate_crosstalk_penalty;
///
/// // Under limit → no penalty
/// let penalty = calculate_crosstalk_penalty(5_000_000, 10_000_000);
/// assert_eq!(penalty, 0);
///
/// // Over limit → exponential penalty
/// let penalty = calculate_crosstalk_penalty(15_000_000, 10_000_000);
/// assert!(penalty > 1000);
/// ```
pub fn calculate_crosstalk_penalty(parallel_length_nm: i64, max_parallel_nm: i64) -> i64 {
    if parallel_length_nm <= max_parallel_nm {
        return 0; // Under limit, no penalty
    }

    // Calculate how much over the limit we are (as integer ratio × 1000)
    let ratio = ((parallel_length_nm - max_parallel_nm) * 1000) / max_parallel_nm;

    // Exponential penalty: 1000 + ratio + (ratio^2 / 2000)
    // This creates a strong disincentive for long parallel runs
    1000 + ratio + (ratio * ratio) / 2000
}
