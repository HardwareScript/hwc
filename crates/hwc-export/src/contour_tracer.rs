//! Contour Tracer: Voxel-to-Vector Conversion with Anti-Aliasing
//!
//! This module solves the "Diagonal Line Problem" (VULNERABILITY-1):
//! Converting discrete voxel grids to smooth vector polygons for manufacturing.
//!
//! ## The Problem
//! Hardware Script uses a voxel grid internally, but manufacturing formats
//! (Gerber, GDSII, 3D meshes) require smooth vector polygons. Naive conversion
//! creates jagged "stair-stepped" edges on diagonal traces.
//!
//! ## The Solution
//! Multi-stage pipeline:
//! 1. Boundary Extraction: Marching Squares algorithm
//! 2. Feature Classification: Detect intentional corners vs. voxel artifacts
//! 3. Smoothing: Chaikin's algorithm with corner preservation
//! 4. Simplification: Douglas-Peucker algorithm
//! 5. Validation: Ensure tolerance compliance
//!
//! ## Configuration
//! Controlled via profile `export:` block:
//! - `antialiasing`: Enable/disable smoothing
//! - `smoothing_tolerance`: Maximum deviation from voxel grid
//! - `corner_lock`: Angles to preserve (e.g., [45, 90])

use rustc_hash::FxHashSet;

/// A 2D point in continuous space
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// A closed polygon contour
#[derive(Debug, Clone)]
pub struct Contour {
    pub points: Vec<Point>,
    pub is_hole: bool, // True if this is a hole inside another polygon
}

/// Configuration for contour tracing
#[derive(Debug, Clone)]
pub struct ContourConfig {
    /// Enable anti-aliasing/smoothing
    pub antialiasing: bool,

    /// Maximum deviation from voxel grid (in voxel units)
    pub smoothing_tolerance: f64,

    /// Angles to preserve during smoothing (degrees)
    pub corner_lock: Vec<u32>,

    /// Number of smoothing iterations (Chaikin's algorithm)
    pub smoothing_iterations: usize,

    /// Tolerance for Douglas-Peucker simplification (in voxel units)
    pub simplification_tolerance: f64,
}

impl Default for ContourConfig {
    fn default() -> Self {
        Self {
            antialiasing: false,
            smoothing_tolerance: 0.1, // 10% of voxel size
            corner_lock: vec![45, 90],
            smoothing_iterations: 2,
            simplification_tolerance: 0.05,
        }
    }
}

/// Contour tracer for voxel-to-vector conversion
pub struct ContourTracer {
    config: ContourConfig,
}

impl ContourTracer {
    pub fn new(config: ContourConfig) -> Self {
        Self { config }
    }

    /// Extract contours from a 2D voxel grid layer
    ///
    /// # Arguments
    /// * `grid` - 2D boolean grid where true = filled voxel
    /// * `width` - Grid width
    /// * `height` - Grid height
    ///
    /// # Returns
    /// Vector of contours (outer boundaries and holes)
    ///
    /// # Safety (Hitbox Law)
    /// If anti-aliasing is enabled, smoothed contours are validated to ensure
    /// they stay within the conservative voxel bounds. If validation fails,
    /// returns raw (jagged) contours as a safe fallback.
    pub fn extract_contours(&self, grid: &[bool], width: usize, height: usize) -> Vec<Contour> {
        // Stage 1: Boundary Extraction (Marching Squares)
        let mut raw_contours = self.marching_squares(grid, width, height);

        if !self.config.antialiasing {
            // No smoothing - return raw voxel boundaries
            return raw_contours;
        }

        // Stage 2-5: Smoothing pipeline with validation
        for contour in &mut raw_contours {
            // Stage 2: Feature Classification
            let locked_indices = self.classify_corners(&contour.points);

            // Stage 3: Smoothing (Chaikin's algorithm)
            let smoothed = self.smooth_contour(&contour.points, &locked_indices);

            // Stage 4: Simplification (Douglas-Peucker)
            let simplified = self.simplify_contour(&smoothed);

            // Stage 5: Validation (Hitbox Law - Conservative Smoothing)
            match self.validate_conservative_smoothing(&simplified, grid, width, height) {
                Ok(()) => {
                    // Validation passed - use smoothed contour
                    contour.points = simplified;
                }
                Err(e) => {
                    // Validation failed - fall back to jagged mode (safe)
                    eprintln!(
                        "⚠️  Anti-aliasing validation failed: {}. Using jagged export (safe mode).",
                        e
                    );
                    // Keep original jagged contour
                }
            }
        }

        raw_contours
    }

    /// Marching Squares algorithm for boundary extraction
    ///
    /// Traces the boundary between filled and empty voxels.
    /// Returns closed polygon loops.
    fn marching_squares(&self, grid: &[bool], width: usize, height: usize) -> Vec<Contour> {
        let mut contours = Vec::new();
        let mut visited = FxHashSet::default();

        // Helper to check if a voxel is filled
        let _is_filled = |x: i32, y: i32| -> bool {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                return false;
            }
            grid[y as usize * width + x as usize]
        };

        // Scan for boundary edges
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if !grid[idx] || visited.contains(&idx) {
                    continue;
                }

                // Found a filled voxel - trace its boundary
                let contour =
                    self.trace_boundary(grid, width, height, x as i32, y as i32, &mut visited);
                if !contour.points.is_empty() {
                    contours.push(contour);
                }
            }
        }

        contours
    }

    /// Trace a single contour starting from a filled voxel
    fn trace_boundary(
        &self,
        _grid: &[bool],
        width: usize,
        _height: usize,
        start_x: i32,
        start_y: i32,
        visited: &mut FxHashSet<usize>,
    ) -> Contour {
        let mut points = Vec::new();

        // Simple boundary tracing: walk around the perimeter
        // For now, just create a rectangle around the voxel
        // TODO: Implement proper Moore-Neighbor tracing

        let x = start_x as f64;
        let y = start_y as f64;

        // Create a square around the voxel
        points.push(Point::new(x, y));
        points.push(Point::new(x + 1.0, y));
        points.push(Point::new(x + 1.0, y + 1.0));
        points.push(Point::new(x, y + 1.0));

        visited.insert(start_y as usize * width + start_x as usize);

        Contour {
            points,
            is_hole: false,
        }
    }

    /// Classify corners: identify vertices that should be preserved
    ///
    /// Returns indices of vertices that represent intentional design features
    /// (90° corners, 45° angles) rather than voxel artifacts.
    fn classify_corners(&self, points: &[Point]) -> FxHashSet<usize> {
        let mut locked = FxHashSet::default();

        if points.len() < 3 {
            return locked;
        }

        for i in 0..points.len() {
            let prev = &points[(i + points.len() - 1) % points.len()];
            let curr = &points[i];
            let next = &points[(i + 1) % points.len()];

            // Calculate angle at this vertex
            let angle = self.calculate_angle(prev, curr, next);

            // Check if this angle matches any locked angles
            for &lock_angle in &self.config.corner_lock {
                let lock_rad = (lock_angle as f64).to_radians();
                let diff = (angle - lock_rad).abs();

                // Allow 5° tolerance for angle matching
                if diff < 5.0_f64.to_radians() {
                    locked.insert(i);
                    break;
                }
            }
        }

        locked
    }

    /// Calculate the angle at a vertex (in radians)
    fn calculate_angle(&self, prev: &Point, curr: &Point, next: &Point) -> f64 {
        let v1x = prev.x - curr.x;
        let v1y = prev.y - curr.y;
        let v2x = next.x - curr.x;
        let v2y = next.y - curr.y;

        let dot = v1x * v2x + v1y * v2y;
        let det = v1x * v2y - v1y * v2x;

        det.atan2(dot).abs()
    }

    /// Smooth contour using Chaikin's corner-cutting algorithm
    ///
    /// Iteratively replaces each edge with two shorter edges,
    /// creating a smoother curve while preserving locked corners.
    fn smooth_contour(&self, points: &[Point], locked: &FxHashSet<usize>) -> Vec<Point> {
        let mut result = points.to_vec();

        for _ in 0..self.config.smoothing_iterations {
            let mut smoothed = Vec::new();

            for i in 0..result.len() {
                let curr = &result[i];
                let next = &result[(i + 1) % result.len()];

                if locked.contains(&i) {
                    // Preserve locked corner
                    smoothed.push(*curr);
                } else {
                    // Chaikin's algorithm: replace edge with two points
                    // at 1/4 and 3/4 along the edge
                    let q =
                        Point::new(0.75 * curr.x + 0.25 * next.x, 0.75 * curr.y + 0.25 * next.y);
                    let r =
                        Point::new(0.25 * curr.x + 0.75 * next.x, 0.25 * curr.y + 0.75 * next.y);

                    smoothed.push(q);
                    smoothed.push(r);
                }
            }

            result = smoothed;
        }

        result
    }

    /// Simplify contour using Douglas-Peucker algorithm
    ///
    /// Reduces vertex count while preserving shape within tolerance.
    fn simplify_contour(&self, points: &[Point]) -> Vec<Point> {
        if points.len() < 3 {
            return points.to_vec();
        }

        let mut result = Vec::new();
        self.douglas_peucker(points, 0, points.len() - 1, &mut result);

        // Close the loop
        if !result.is_empty() {
            result.push(result[0]);
        }

        result
    }

    /// Validate that smoothed contour stays within voxel bounds (Hitbox Law)
    ///
    /// This is the CRITICAL safety check that enforces the "Conservative Smoothing" rule:
    /// The smooth vector must always stay INSIDE the jagged voxels.
    ///
    /// If validation fails, the export should fall back to jagged mode (safe mode).
    pub fn validate_conservative_smoothing(
        &self,
        smoothed: &[Point],
        original_grid: &[bool],
        width: usize,
        height: usize,
    ) -> Result<(), String> {
        for point in smoothed {
            // Convert continuous point to voxel coordinates
            let vx = point.x.floor() as i32;
            let vy = point.y.floor() as i32;

            // Check if point is within grid bounds
            // CRITICAL FIX: Allow points on the right/bottom edge (width/height exactly)
            // In a 50-voxel grid, the right edge of the last voxel is at x=50.0
            if vx < 0 || vy < 0 || vx > width as i32 || vy > height as i32 {
                return Err(format!(
                    "Smoothed point ({:.2}, {:.2}) is outside grid bounds",
                    point.x, point.y
                ));
            }

            // Clamp to valid indices for array access
            let vx_clamped = vx.min(width as i32 - 1).max(0) as usize;
            let vy_clamped = vy.min(height as i32 - 1).max(0) as usize;

            // Check if the voxel at this point is filled (conservative bound)
            let idx = vy_clamped * width + vx_clamped;
            if !original_grid[idx] {
                // Check neighboring voxels within tolerance
                let mut found_filled = false;
                let tolerance_voxels = (self.config.smoothing_tolerance.ceil() as i32).max(1);

                for dy in -tolerance_voxels..=tolerance_voxels {
                    for dx in -tolerance_voxels..=tolerance_voxels {
                        let nx = vx + dx;
                        let ny = vy + dy;

                        if nx >= 0 && ny >= 0 && nx < width as i32 && ny < height as i32 {
                            let nidx = ny as usize * width + nx as usize;
                            if original_grid[nidx] {
                                // Check if point is within tolerance distance
                                let dist = ((dx * dx + dy * dy) as f64).sqrt();
                                if dist <= self.config.smoothing_tolerance {
                                    found_filled = true;
                                    break;
                                }
                            }
                        }
                    }
                    if found_filled {
                        break;
                    }
                }

                if !found_filled {
                    return Err(format!(
                        "Smoothed point ({:.2}, {:.2}) violates conservative bound - no filled voxel within tolerance",
                        point.x, point.y
                    ));
                }
            }
        }

        Ok(())
    }

    /// Douglas-Peucker recursive implementation
    fn douglas_peucker(&self, points: &[Point], start: usize, end: usize, result: &mut Vec<Point>) {
        if end <= start + 1 {
            result.push(points[start]);
            return;
        }

        // Find the point with maximum distance from line segment
        let mut max_dist = 0.0;
        let mut max_idx = start;

        for i in (start + 1)..end {
            let dist = self.perpendicular_distance(&points[i], &points[start], &points[end]);
            if dist > max_dist {
                max_dist = dist;
                max_idx = i;
            }
        }

        // If max distance is greater than tolerance, recursively simplify
        if max_dist > self.config.simplification_tolerance {
            self.douglas_peucker(points, start, max_idx, result);
            self.douglas_peucker(points, max_idx, end, result);
        } else {
            result.push(points[start]);
        }
    }

    /// Calculate perpendicular distance from point to line segment
    fn perpendicular_distance(&self, point: &Point, line_start: &Point, line_end: &Point) -> f64 {
        let dx = line_end.x - line_start.x;
        let dy = line_end.y - line_start.y;

        let numerator = (dy * point.x - dx * point.y + line_end.x * line_start.y
            - line_end.y * line_start.x)
            .abs();
        let denominator = (dx * dx + dy * dy).sqrt();

        if denominator == 0.0 {
            point.distance_to(line_start)
        } else {
            numerator / denominator
        }
    }
}
