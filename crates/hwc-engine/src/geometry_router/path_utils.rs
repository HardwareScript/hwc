//! Path utility functions for routing.

use crate::geometry::Point3D;

/// Calculate the electrical length of a routed path in nanometers.
///
/// Sums the Euclidean distance between consecutive points in the path.
/// For high-speed signals, this must match within tight tolerances (typically 0.1mm).
///
/// # Arguments
/// * `path` - The routed path as a sequence of 3D points
///
/// # Returns
/// Total electrical length in nanometers
pub fn calculate_path_length(path: &[Point3D]) -> i64 {
    if path.len() < 2 {
        return 0;
    }

    let mut total_length = 0i64;

    for i in 0..path.len() - 1 {
        let from = path[i];
        let to = path[i + 1];

        // Calculate Euclidean distance
        let dx = (to.x - from.x) as f64;
        let dy = (to.y - from.y) as f64;
        let dz = (to.z - from.z) as f64;

        let segment_length = (dx * dx + dy * dy + dz * dz).sqrt();
        total_length += segment_length as i64;
    }

    total_length
}
