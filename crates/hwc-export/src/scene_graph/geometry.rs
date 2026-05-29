//! Geometric algorithms for path simplification and calculations

/// Douglas-Peucker algorithm for line simplification
/// Collapses collinear points into straight segments
pub fn douglas_peucker(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    // Find the point with maximum distance from the line
    let mut max_dist = 0.0;
    let mut max_index = 0;

    let start = points[0];
    let end = points[points.len() - 1];

    for (i, &point) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let dist = perpendicular_distance(point, start, end);
        if dist > max_dist {
            max_dist = dist;
            max_index = i;
        }
    }

    // If max distance is greater than tolerance, recursively simplify
    if max_dist > tolerance {
        let mut left = douglas_peucker(&points[0..=max_index], tolerance);
        let right = douglas_peucker(&points[max_index..], tolerance);

        left.pop(); // Remove duplicate point
        left.extend(right);
        left
    } else {
        // All points are within tolerance - return just start and end
        vec![start, end]
    }
}

/// Calculate perpendicular distance from point to line
pub fn perpendicular_distance(
    point: (f64, f64),
    line_start: (f64, f64),
    line_end: (f64, f64),
) -> f64 {
    let dx = line_end.0 - line_start.0;
    let dy = line_end.1 - line_start.1;

    if dx == 0.0 && dy == 0.0 {
        // Line is a point
        let px = point.0 - line_start.0;
        let py = point.1 - line_start.1;
        return (px * px + py * py).sqrt();
    }

    let numerator =
        (dy * point.0 - dx * point.1 + line_end.0 * line_start.1 - line_end.1 * line_start.0).abs();
    let denominator = (dx * dx + dy * dy).sqrt();

    numerator / denominator
}

/// Calculate perpendicular vector at a point on the path (for miter joins)
pub fn calculate_perpendicular(path: &[(f64, f64)], index: usize) -> (f64, f64) {
    let n = path.len();

    if n == 1 {
        return (0.0, 1.0); // Default perpendicular
    }

    let (dx, dy) = if index == 0 {
        // First point - use direction to next point
        let next = path[1];
        let curr = path[0];
        (next.0 - curr.0, next.1 - curr.1)
    } else if index == n - 1 {
        // Last point - use direction from previous point
        let curr = path[n - 1];
        let prev = path[n - 2];
        (curr.0 - prev.0, curr.1 - prev.1)
    } else {
        // Middle point - use average of incoming and outgoing directions (miter)
        let prev = path[index - 1];
        let curr = path[index];
        let next = path[index + 1];

        let dx1 = curr.0 - prev.0;
        let dy1 = curr.1 - prev.1;
        let dx2 = next.0 - curr.0;
        let dy2 = next.1 - curr.1;

        // Average direction
        ((dx1 + dx2) / 2.0, (dy1 + dy2) / 2.0)
    };

    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        return (0.0, 1.0);
    }

    // Perpendicular is 90° rotation: (dx, dy) -> (-dy, dx)
    let px = -dy / mag;
    let py = dx / mag;

    (px, py)
}
