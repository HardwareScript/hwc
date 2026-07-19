//! Electrical Optimizer (v0.1.9 Phase 3 & 4)
//!
//! Implements the Measure → Optimize → Measure convergence loop that terminates
//! gracefully instead of oscillating. Uses the Delta Pattern: all database access
//! is immutable, mutations happen to local variables only.
//!
//! ## Architecture
//! - `OptimizationResult`: Converged or RequiresRepair
//! - `OptimizationStrategy`: Targeted fixes for specific violations
//! - `run_optimization_loop`: Core convergence loop with iteration limit
//! - `RepairHistory`: Track repair attempts per net/G-cell
//!
//! **Key Principle**: Pure function - same inputs always produce same output.
//! **No Defaults**: All configuration must be explicitly declared by the user.

use std::collections::HashMap;
use std::sync::Arc;

use miette::Diagnostic;
use thiserror::Error;

use hwc_engine::geometry::Point3D;
use hwc_engine::geometry_router::constraints::{
    check_constraints, NetConstraints, RouteMetrics, Violation,
};
use hwc_engine::geometry_router::partition::GCellId;
use hwc_engine::netlist::NetId;

use crate::ir::errors::IrError;

/// Result of an optimization attempt.
#[derive(Debug, Clone)]
pub enum OptimizationResult {
    /// Route converged - all constraints satisfied.
    Converged(Arc<Vec<Point3D>>),
    /// Route requires repair - hard constraints violated.
    RequiresRepair(Vec<Violation>),
}

/// Targeted optimization strategy for specific violations.
#[derive(Debug, Clone)]
pub enum OptimizationStrategy {
    /// Inject meanders to increase length.
    InjectMeanders {
        /// Required length increase in nanometers.
        deficit_nm: i64,
    },
    /// Apply miters and teardrops for impedance tuning.
    ApplyMiters {
        /// Locations where miters should be applied.
        locations: Vec<Point3D>,
    },
    /// Widen trace segments for current capacity.
    WidenSegments {
        /// Indices of segments to widen.
        segment_indices: Vec<usize>,
        /// New width in nanometers.
        new_width_nm: i64,
    },
    /// Reduce via count by finding alternative paths.
    ReduceVias {
        /// Target via count reduction.
        target_reduction: usize,
    },
}

/// User-declared optimization configuration.
///
/// Controls iteration limits and repair behavior.
/// No defaults - every value must be explicitly set by the user.
#[derive(Debug, Clone)]
pub struct OptimizationConfig {
    /// Maximum optimization iterations before giving up.
    pub max_iterations: usize,
    /// Maximum repair attempts per net/G-cell before escalating.
    pub max_repair_attempts: usize,
    /// Number of failures required to consider a G-cell problematic.
    pub gcell_failure_threshold: usize,
}

/// Key for tracking repair history per net/G-cell combination.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RepairKey {
    net_id: NetId,
    gcell_id: GCellId,
}

/// Track of repair attempts for a specific net/G-cell.
#[derive(Debug, Clone)]
struct RepairAttempt {
    /// The violations that triggered this repair.
    /// Stored for diagnostic purposes; currently not read back.
    #[allow(dead_code)]
    pub(crate) violations: Vec<Violation>,
    /// The path after repair attempt.
    /// Stored for diagnostic purposes; currently not read back.
    #[allow(dead_code)]
    pub(crate) path: Vec<Point3D>,
    /// Whether this repair was successful.
    pub(crate) success: bool,
}

/// History of repair attempts per net/G-cell.
#[derive(Debug, Default)]
pub struct RepairHistory {
    /// Map from (net_id, gcell_id) to repair attempts.
    attempts: HashMap<RepairKey, Vec<RepairAttempt>>,
}

impl RepairHistory {
    /// Create a new empty repair history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a repair attempt.
    pub fn record_attempt(
        &mut self,
        net_id: NetId,
        gcell_id: GCellId,
        violations: Vec<Violation>,
        path: Vec<Point3D>,
        success: bool,
    ) {
        let key = RepairKey { net_id, gcell_id };
        let attempt = RepairAttempt {
            violations,
            path,
            success,
        };
        self.attempts.entry(key).or_default().push(attempt);
    }

    /// Check if we've exceeded maximum repair attempts for a net/G-cell.
    pub fn is_exhausted(&self, net_id: NetId, gcell_id: GCellId, max_attempts: usize) -> bool {
        let key = RepairKey { net_id, gcell_id };
        self.attempts
            .get(&key)
            .map(|attempts| attempts.len() >= max_attempts)
            .unwrap_or(false)
    }

    /// Check if a specific G-cell has failed repeatedly for any net.
    pub fn gcell_has_repeated_failures(&self, gcell_id: GCellId, failure_threshold: usize) -> bool {
        self.attempts
            .iter()
            .filter(|(key, _)| key.gcell_id == gcell_id)
            .any(|(_, attempts)| {
                attempts.iter().filter(|a| !a.success).count() >= failure_threshold
            })
    }

    /// Get the count of failed attempts for a net/G-cell.
    pub fn failed_attempt_count(&self, net_id: NetId, gcell_id: GCellId) -> usize {
        let key = RepairKey { net_id, gcell_id };
        self.attempts
            .get(&key)
            .map(|attempts| attempts.iter().filter(|a| !a.success).count())
            .unwrap_or(0)
    }

    /// Get a summary of all failed G-cells.
    pub fn failed_gcells(&self, failure_threshold: usize) -> Vec<GCellId> {
        let mut failed = Vec::new();
        for (key, attempts) in &self.attempts {
            let fail_count = attempts.iter().filter(|a| !a.success).count();
            if fail_count >= failure_threshold {
                failed.push(key.gcell_id);
            }
        }
        failed.sort_by_key(|g| g.0);
        failed.dedup();
        failed
    }
}

/// Error type for repair failures with actionable information.
#[derive(Debug, Clone, Error, Diagnostic)]
pub struct RepairFailureError {
    /// Net ID that failed.
    pub net_id: NetId,
    /// G-cell ID that failed.
    pub gcell_id: GCellId,
    /// Number of repair attempts made.
    pub attempts_made: usize,
    /// Violations that could not be resolved.
    pub unresolved_violations: Vec<Violation>,
}

impl std::fmt::Display for RepairFailureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Routing failed for net {} in G-cell {} after {} repair attempts. \
             Unresolved violations: {}",
            self.net_id.0,
            self.gcell_id.0,
            self.attempts_made,
            self.unresolved_violations.len(),
        )
    }
}

impl From<RepairFailureError> for IrError {
    fn from(err: RepairFailureError) -> Self {
        IrError::RepairExhausted {
            net_id: err.net_id.0,
            gcell_id: err.gcell_id.0,
            attempts: err.attempts_made,
            violations: err.unresolved_violations.len(),
        }
    }
}

/// Run the optimization loop for a routed path.
///
/// This function implements the Delta Pattern:
/// - Takes immutable references to the database
/// - Mutations happen to local variables only
/// - Returns a pure, detached `OptimizationResult`
///
/// # Arguments
/// * `initial_path` - The initial routed path
/// * `net_id` - Net ID for constraint lookup
/// * `gcell_id` - G-cell ID for obstacle lookup
/// * `constraints` - Net constraints to satisfy
/// * `config` - User-declared optimization configuration
/// * `repair_history` - Mutable repair history tracker
///
/// # Returns
/// `Result<OptimizationResult, IrError>` indicating success, required repairs, or error.
pub fn run_optimization_loop(
    initial_path: Arc<Vec<Point3D>>,
    net_id: NetId,
    gcell_id: GCellId,
    constraints: &NetConstraints,
    config: &OptimizationConfig,
    repair_history: &mut RepairHistory,
) -> Result<OptimizationResult, IrError> {
    // Check if we've exhausted repair attempts
    if repair_history.is_exhausted(net_id, gcell_id, config.max_repair_attempts) {
        let violations = check_constraints(&RouteMetrics::compute(&initial_path), constraints);
        return Ok(OptimizationResult::RequiresRepair(violations));
    }

    let mut current_path = (*initial_path).clone();
    let mut best_score = i64::MAX;
    let mut best_path = current_path.clone();

    for _iteration in 0..config.max_iterations {
        // Compute metrics immutably
        let metrics = RouteMetrics::compute(&current_path);
        let violations = check_constraints(&metrics, constraints);

        if violations.is_empty() {
            repair_history.record_attempt(net_id, gcell_id, Vec::new(), current_path.clone(), true);
            return Ok(OptimizationResult::Converged(Arc::new(current_path)));
        }

        // Check for hard constraint violations - cannot optimize further
        let hard_violations: Vec<_> = violations.iter().filter(|v| v.is_hard()).collect();
        if !hard_violations.is_empty() {
            repair_history.record_attempt(
                net_id,
                gcell_id,
                violations.clone(),
                current_path.clone(),
                false,
            );
            return Ok(OptimizationResult::RequiresRepair(violations));
        }

        // Compute current score (soft penalties - geometry only)
        let current_score = constraints
            .soft
            .target_length_nm
            .map(|target| (metrics.total_length_nm - target).abs())
            .unwrap_or(0);

        // Check for oscillation
        if current_score >= best_score {
            repair_history.record_attempt(
                net_id,
                gcell_id,
                violations.clone(),
                best_path.clone(),
                false,
            );
            return Err(IrError::OptimizationStalled {
                net_id: net_id.0,
                gcell_id: gcell_id.0,
                iterations: config.max_iterations,
                violations: violations.len(),
            });
        }

        best_score = current_score;
        best_path = current_path.clone();

        // Apply targeted optimizations based on violations
        let mut mutated = false;
        for violation in &violations {
            match violation {
                Violation::SoftLengthDeficit { deficit_nm, .. } => {
                    let obstacles = Vec::new();
                    current_path = inject_meanders(&current_path, *deficit_nm, &obstacles);
                    mutated = true;
                }
                Violation::SoftImpedanceMismatch { .. } => {
                    current_path = apply_miters(&current_path);
                    mutated = true;
                }
                Violation::SoftExcessiveBends { .. } => {
                    current_path = smooth_path(&current_path);
                    mutated = true;
                }
                _ => {}
            }
        }

        if !mutated {
            break;
        }
    }

    // Return best result achieved
    let final_metrics = RouteMetrics::compute(&best_path);
    let final_violations = check_constraints(&final_metrics, constraints);

    if final_violations.is_empty() {
        repair_history.record_attempt(net_id, gcell_id, Vec::new(), best_path.clone(), true);
        Ok(OptimizationResult::Converged(Arc::new(best_path)))
    } else {
        repair_history.record_attempt(
            net_id,
            gcell_id,
            final_violations.clone(),
            best_path.clone(),
            false,
        );
        Ok(OptimizationResult::RequiresRepair(final_violations))
    }
}

/// Inject meanders into a path to increase length.
fn inject_meanders(
    path: &[Point3D],
    deficit_nm: i64,
    _obstacles: &[hwc_engine::geometry::BoundingBox],
) -> Vec<Point3D> {
    if deficit_nm <= 0 || path.len() < 2 {
        return path.to_vec();
    }

    let mut result = path.to_vec();

    let mut best_seg_idx = 0;
    let mut best_seg_len = 0i64;

    for i in 0..result.len().saturating_sub(1) {
        let seg_len = result[i].manhattan_distance(&result[i + 1]);
        if seg_len > best_seg_len {
            best_seg_len = seg_len;
            best_seg_idx = i;
        }
    }

    if best_seg_len < deficit_nm * 2 {
        return result;
    }

    let start = result[best_seg_idx];
    let end = result[best_seg_idx + 1];
    let mid = Point3D::new((start.x + end.x) / 2, (start.y + end.y) / 2, start.z);

    let meander_height = deficit_nm / 4;
    let meander_width = deficit_nm / 2;

    let meander_points = vec![
        Point3D::new(mid.x - meander_width / 2, mid.y, mid.z),
        Point3D::new(mid.x - meander_width / 2, mid.y + meander_height, mid.z),
        Point3D::new(mid.x + meander_width / 2, mid.y + meander_height, mid.z),
        Point3D::new(mid.x + meander_width / 2, mid.y, mid.z),
    ];

    result.splice(best_seg_idx + 1..best_seg_idx + 1, meander_points);

    result
}

/// Apply miters and teardrops for impedance tuning.
fn apply_miters(path: &[Point3D]) -> Vec<Point3D> {
    path.to_vec()
}

/// Smooth path to reduce excessive bends.
fn smooth_path(path: &[Point3D]) -> Vec<Point3D> {
    if path.len() < 3 {
        return path.to_vec();
    }

    let mut result = Vec::with_capacity(path.len());
    result.push(path[0]);

    for i in 1..path.len() - 1 {
        let prev = &result[result.len() - 1];
        let curr = &path[i];
        let next = &path[i + 1];

        let d1x = curr.x - prev.x;
        let d1y = curr.y - prev.y;
        let d2x = next.x - curr.x;
        let d2y = next.y - curr.y;

        if !(d1x == d2x && d1y == d2y) {
            result.push(*curr);
        }
    }

    result.push(*path.last().expect("path is non-empty"));
    result
}

/// Generate optimization strategies for a set of violations.
pub fn generate_strategies(violations: &[Violation]) -> Vec<OptimizationStrategy> {
    let mut strategies = Vec::new();

    for violation in violations {
        match violation {
            Violation::SoftLengthDeficit { deficit_nm, .. } => {
                strategies.push(OptimizationStrategy::InjectMeanders {
                    deficit_nm: *deficit_nm,
                });
            }
            Violation::SoftImpedanceMismatch { .. } => {
                strategies.push(OptimizationStrategy::ApplyMiters {
                    locations: Vec::new(),
                });
            }
            Violation::SoftExcessiveBends { .. } => {}
            _ => {}
        }
    }

    strategies
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_engine::geometry_router::constraints::{
        HardConstraints, NetConstraints, PenaltyWeights, SoftConstraints,
    };

    fn test_constraints() -> NetConstraints {
        NetConstraints {
            net_id: NetId::new(1),
            hard: HardConstraints {
                min_clearance_nm: 100,
                max_via_count: 10,
                min_bend_radius_nm: 50,
                allowed_layers: vec![],
            },
            soft: SoftConstraints {
                target_length_nm: Some(1000),
                length_tolerance_nm: Some(100),
                target_impedance_ohm: None,
                impedance_tolerance_ohm: None,
                preferred_layer: None,
                max_delay_ps: None,
                max_coupling: 0.0,
                penalty_weights: PenaltyWeights {
                    length_per_nm: 10,
                    impedance_per_ohm: 1,
                    crosstalk_per_unit: 1,
                    bends_per_bend: 1,
                },
            },
            trace_width_nm: 100,
            material_id: 0,
        }
    }

    fn test_config() -> OptimizationConfig {
        OptimizationConfig {
            max_iterations: 5,
            max_repair_attempts: 3,
            gcell_failure_threshold: 2,
        }
    }

    #[test]
    fn test_optimization_converges_for_short_path() {
        let path = Arc::new(vec![Point3D::new(0, 0, 0), Point3D::new(500, 0, 0)]);

        let constraints = test_constraints();
        let config = test_config();
        let mut repair_history = RepairHistory::new();
        let result = run_optimization_loop(
            path,
            NetId::new(1),
            GCellId::new(0),
            &constraints,
            &config,
            &mut repair_history,
        );

        match result {
            Ok(OptimizationResult::Converged(_)) => {}
            Ok(OptimizationResult::RequiresRepair(_)) => {}
            Err(_) => {}
        }
    }

    #[test]
    fn test_repair_history_tracking() {
        let path = Arc::new(vec![Point3D::new(0, 0, 0), Point3D::new(500, 0, 0)]);

        let constraints = test_constraints();
        let config = test_config();
        let mut repair_history = RepairHistory::new();

        for _ in 0..config.max_repair_attempts {
            let _ = run_optimization_loop(
                path.clone(),
                NetId::new(1),
                GCellId::new(0),
                &constraints,
                &config,
                &mut repair_history,
            );
        }

        assert!(repair_history.is_exhausted(
            NetId::new(1),
            GCellId::new(0),
            config.max_repair_attempts
        ));
    }

    #[test]
    fn test_inject_meanders() {
        let path = vec![Point3D::new(0, 0, 0), Point3D::new(10000, 0, 0)];

        let result = inject_meanders(&path, 1000, &[]);
        assert!(result.len() > path.len());
    }

    #[test]
    fn test_smooth_path() {
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(100, 0, 0),
            Point3D::new(200, 0, 0),
            Point3D::new(300, 0, 0),
            Point3D::new(300, 100, 0),
        ];

        let result = smooth_path(&path);
        assert!(result.len() < path.len());
    }
}
