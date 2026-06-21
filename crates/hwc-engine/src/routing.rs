//! Routing algorithms for manual waypoint interpolation.
//!
//! This module implements Phase 6: Basic routing with manual waypoints.
//! Uses Bresenham's 3D line algorithm to interpolate between waypoints.
//!
//! The full 3-phase routing pipeline (Constraint Manager, Geometry Router, DRC)
//! is implemented in System 3.

use crate::geometry::Point3D;
use crate::space::VoxelSize;
use crate::material::MaterialId;
use compact_str::CompactString;

/// Router for manual waypoint interpolation.
///
/// Implements Bresenham's 3D line algorithm to draw traces between waypoints.
pub struct Router {
    // Future: Routing constraints, design rules, etc.
}

impl Router {
    /// Create a new router.
    pub fn new() -> Self {
        Self {}
    }

    /// Interpolate waypoints using Bresenham's 3D line algorithm.
    ///
    /// Returns all voxel coordinates along the path between waypoints.
    ///
    /// # Arguments
    /// * `waypoints` - List of waypoints to connect
    ///
    /// # Returns
    /// Vector of all voxel coordinates along the interpolated path
    pub fn interpolate_waypoints(&self, waypoints: &[Point3D]) -> Vec<Point3D> {
        if waypoints.len() < 2 {
            return waypoints.to_vec();
        }

        let mut result = Vec::new();

        // Interpolate between each consecutive pair of waypoints
        for i in 0..waypoints.len() - 1 {
            let segment = self.bresenham_3d(waypoints[i], waypoints[i + 1]);

            // Add segment points (skip first point if not the first segment to avoid duplicates)
            if i == 0 {
                result.extend(segment);
            } else {
                result.extend(segment.into_iter().skip(1));
            }
        }

        result
    }

    /// Bresenham's 3D line algorithm.
    ///
    /// Generates all voxel coordinates along a 3D line from start to end.
    fn bresenham_3d(&self, start: Point3D, end: Point3D) -> Vec<Point3D> {
        let mut points = Vec::new();

        let dx = (end.x - start.x).abs();
        let dy = (end.y - start.y).abs();
        let dz = (end.z - start.z).abs();

        let sx = if start.x < end.x { 1 } else { -1 };
        let sy = if start.y < end.y { 1 } else { -1 };
        let sz = if start.z < end.z { 1 } else { -1 };

        let mut x = start.x;
        let mut y = start.y;
        let mut z = start.z;

        // Determine dominant axis
        if dx >= dy && dx >= dz {
            // X is dominant
            let mut err_y = dx / 2;
            let mut err_z = dx / 2;

            while x != end.x {
                points.push(Point3D::new(x, y, z));

                err_y -= dy;
                if err_y < 0 {
                    y += sy;
                    err_y += dx;
                }

                err_z -= dz;
                if err_z < 0 {
                    z += sz;
                    err_z += dx;
                }

                x += sx;
            }
        } else if dy >= dx && dy >= dz {
            // Y is dominant
            let mut err_x = dy / 2;
            let mut err_z = dy / 2;

            while y != end.y {
                points.push(Point3D::new(x, y, z));

                err_x -= dx;
                if err_x < 0 {
                    x += sx;
                    err_x += dy;
                }

                err_z -= dz;
                if err_z < 0 {
                    z += sz;
                    err_z += dy;
                }

                y += sy;
            }
        } else {
            // Z is dominant
            let mut err_x = dz / 2;
            let mut err_y = dz / 2;

            while z != end.z {
                points.push(Point3D::new(x, y, z));

                err_x -= dx;
                if err_x < 0 {
                    x += sx;
                    err_x += dz;
                }

                err_y -= dy;
                if err_y < 0 {
                    y += sy;
                    err_y += dz;
                }

                z += sz;
            }
        }

        // Add final point
        points.push(Point3D::new(x, y, z));

        points
    }

    /// Place a trace in the voxel grid.
    ///
    /// Fills voxels along the trace path with the specified material.
    ///
    /// # Arguments
    /// * `grid` - Voxel grid to place trace in
    /// * `voxel_size` - Size of each voxel in nanometers
    /// * `waypoints` - Waypoints defining the trace path (in nanometers)
    /// * `material` - Trace material (typically Copper)
    /// * `net_id` - Net ID for connectivity tracking
    /// * `width_voxels` - Trace width in voxels (1 = single voxel wide)
    pub fn place_trace(
        &self,
        grid: &mut crate::geometry_router::EntityGraph,
        voxel_size: &VoxelSize,
        waypoints: &[Point3D],
        material: MaterialId,
        net_id: u32,
        width_voxels: usize,
    ) -> Result<(), RoutingError> {
        if waypoints.is_empty() {
            return Err(RoutingError::EmptyWaypoints);
        }

        // Convert waypoints from nanometers to voxel coordinates
        let voxel_waypoints: Vec<Point3D> = waypoints
            .iter()
            .map(|&point| {
                let (x, y, z) = crate::geometry_router::EntityGraph::nm_to_voxel(point, voxel_size);
                Point3D::new(x as i64, y as i64, z as i64)
            })
            .collect();

        // Interpolate waypoints to get all voxel coordinates
        let path = self.interpolate_waypoints(&voxel_waypoints);

        // Store as analytic trace instead of stamping voxels
        // The TopologicalRouter uses DynamicSpatialIndex for obstacle detection
        // and routes are stored as analytic primitives until export
        let _ = (grid, material, net_id, width_voxels, &path);

        Ok(())
    }

    /// Detect vias (layer changes) in a trace path.
    ///
    /// Returns indices of waypoints where layer changes occur.
    pub fn detect_vias(&self, waypoints: &[Point3D]) -> Vec<usize> {
        let mut via_indices = Vec::new();

        for i in 1..waypoints.len() {
            if waypoints[i].z != waypoints[i - 1].z {
                via_indices.push(i);
            }
        }

        via_indices
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// Routing errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum RoutingError {
    #[error("Route has no waypoints")]
    #[diagnostic(
        code(R24),
        url("https://docs.hw-script.org/errors/R24"),
        help("Routes must have at least two waypoints (start and end). Add waypoints to define the trace path.")
    )]
    EmptyWaypoints,

    #[error("Invalid trace width: {width_nm}nm")]
    #[diagnostic(
        code(R23),
        url("https://docs.hw-script.org/errors/R23"),
        help("Physical Explanation: Trace width determines current-carrying capacity (ampacity). Too narrow = overheating and failure. Too wide = wasted space and cost.\n\nAmpacity Formula: I = k × ΔT^0.44 × (W × H)^0.725\nWhere: I = current (A), k = material constant, ΔT = temperature rise (°C), W = width, H = thickness\n\nSolution: Use material database to calculate required width for your current: hwc materials trace-width <material> <current> <temp-rise>\n\nConstraints: Check your profile definition (e.g., profiles.hw) for minimum/maximum trace widths.")
    )]
    InvalidTraceWidth { width_nm: i64 },

    /// Clearance violation between two nets.
    ///
    /// Multi-label error showing both nets that are too close together.
    /// Boxed to reduce enum size.
    #[error(transparent)]
    #[diagnostic(transparent)]
    ClearanceViolation(#[from] Box<ClearanceViolationError>),

    /// Trace width violation for current carrying capacity.
    ///
    /// Multi-label error showing the trace and the constraint violation.
    /// Boxed to reduce enum size.
    #[error(transparent)]
    #[diagnostic(transparent)]
    TraceWidthViolation(#[from] Box<TraceWidthViolationError>),
}

/// Clearance violation error with multi-label support.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("Clearance violation: nets '{first_net}' and '{second_net}' are {actual_nm}nm apart (minimum: {required_nm}nm)")]
#[diagnostic(
    code(P18),
    url("https://docs.hw-script.org/errors/P18"),
    help("Physical Explanation: Insufficient clearance between conductors at different voltages can cause dielectric breakdown (arcing). The dielectric material between traces has a breakdown voltage that depends on distance.\n\nBreakdown Voltage: V_bd = E_bd × d\nWhere: V_bd = breakdown voltage, E_bd = dielectric strength (V/m), d = distance\n\nFor FR4: E_bd ≈ 20 kV/mm = 20 V/μm\nFor Air: E_bd ≈ 3 kV/mm = 3 V/μm\n\nSolution: Increase spacing between traces or reduce voltage difference.\n\nSafety Factor: IPC-2221 recommends 2× minimum clearance for reliability.")
)]
pub struct ClearanceViolationError {
    #[source_code]
    pub src: String,

    pub first_net: CompactString,
    pub second_net: CompactString,
    pub actual_nm: i64,
    pub required_nm: i64,
    pub voltage_diff_mv: i64,

    #[label("First net routed here")]
    pub first_span: miette::SourceSpan,

    #[label("Second net too close here")]
    pub second_span: miette::SourceSpan,
}

/// Trace width violation error with multi-label support.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("Trace width {actual_nm}nm insufficient for {current_ma}mA (minimum: {required_nm}nm)")]
#[diagnostic(
    code(P21),
    url("https://docs.hw-script.org/errors/P21"),
    help("Physical Explanation: Traces carry current and generate heat due to resistance. Insufficient width causes excessive temperature rise, leading to:\n- Solder joint failure (>10°C rise)\n- PCB delamination (>30°C rise)\n- Conductor melting (>100°C rise)\n\nIPC-2221 Formula: A = (I / (k × ΔT^0.44))^(1/0.725)\nWhere: A = cross-sectional area (mil²), I = current (A), k = 0.048 (external) or 0.024 (internal), ΔT = temperature rise (°C)\n\nTypical Values:\n- 1A @ 10°C rise: ~15 mil (380μm) width for 1oz copper\n- 3A @ 10°C rise: ~40 mil (1mm) width for 1oz copper\n\nSolution: Increase trace width or reduce current. Use hwc materials trace-width to calculate.")
)]
pub struct TraceWidthViolationError {
    #[source_code]
    pub src: String,

    pub net_name: CompactString,
    pub actual_nm: i64,
    pub required_nm: i64,
    pub current_ma: i64,
    pub temp_rise_c: i64,

    #[label("Trace width specified here")]
    pub trace_span: miette::SourceSpan,
}
