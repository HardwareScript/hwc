//! Constraint Type System (v0.1.9)
//!
//! Formalizes the distinction between hard constraints (fail if violated) and
//! soft constraints (optimize to minimize delta). This module provides the core
//! types for the Salsa-driven constraint solver architecture.
//!
//! ## Architecture
//! - `ConstraintKind<T>`: Wrapper distinguishing hard vs soft constraints
//! - `NetConstraints`: Per-net constraint collection
//! - `RouteMetrics`: Computed metrics for a routed path
//! - `Violation`: Constraint violations detected during verification

use crate::geometry::Point3D;
use crate::geometry_router::connection_interface::DerivedConstraint;
use crate::netlist::NetId;

/// Wrapper distinguishing hard constraints from soft constraints.
///
/// Hard constraints must be satisfied or the route is invalid.
/// Soft constraints are optimization targets with penalty scoring.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstraintKind<T> {
    /// Hard constraint: violation causes routing failure.
    /// Contains the constraint value and description.
    Hard(T, &'static str),
    /// Soft constraint: violation adds penalty score.
    /// Contains the target value, weight, and description.
    Soft(T, i64, &'static str),
}

impl<T> ConstraintKind<T> {
    /// Get the inner value regardless of variant.
    pub fn value(&self) -> &T {
        match self {
            ConstraintKind::Hard(v, _) => v,
            ConstraintKind::Soft(v, _, _) => v,
        }
    }

    /// Check if this is a hard constraint.
    pub fn is_hard(&self) -> bool {
        matches!(self, ConstraintKind::Hard(..))
    }

    /// Check if this is a soft constraint.
    pub fn is_soft(&self) -> bool {
        matches!(self, ConstraintKind::Soft(..))
    }

    /// Get the penalty weight (0 for hard constraints).
    pub fn weight(&self) -> i64 {
        match self {
            ConstraintKind::Hard(..) => 0,
            ConstraintKind::Soft(_, w, _) => *w,
        }
    }
}

/// Hard constraints that must be satisfied for a valid route.
#[derive(Debug, Clone)]
pub struct HardConstraints {
    /// Minimum clearance from obstacles in nanometers.
    pub min_clearance_nm: i64,
    /// Maximum via count allowed.
    pub max_via_count: usize,
    /// Minimum bend radius in nanometers.
    pub min_bend_radius_nm: i64,
    /// Allowed layers for routing (empty = all layers).
    pub allowed_layers: Vec<i64>,
}

/// Soft constraints that are optimization targets.
#[derive(Debug, Clone)]
pub struct SoftConstraints {
    /// Target length in nanometers (optimize toward this).
    pub target_length_nm: Option<i64>,
    /// Length tolerance in nanometers (acceptable deviation).
    pub length_tolerance_nm: Option<i64>,
    /// Target impedance in ohms.
    pub target_impedance_ohm: Option<f64>,
    /// Impedance tolerance in ohms.
    pub impedance_tolerance_ohm: Option<f64>,
    /// Preferred routing layer (optimize toward this).
    pub preferred_layer: Option<i64>,
    /// Maximum delay in picoseconds.
    pub max_delay_ps: Option<f64>,
    /// Maximum crosstalk coupling coefficient (0.0 = disabled).
    pub max_coupling: f64,
    /// Penalty weights for soft constraint violations.
    pub penalty_weights: PenaltyWeights,
}

/// Penalty weights for soft constraint violations.
///
/// Users declare these to control optimization behavior.
/// No defaults - every weight must be explicitly set.
#[derive(Debug, Clone)]
pub struct PenaltyWeights {
    /// Weight per nanometer of length deviation.
    pub length_per_nm: i64,
    /// Weight per ohm of impedance deviation.
    pub impedance_per_ohm: i64,
    /// Weight per unit of coupling above threshold.
    pub crosstalk_per_unit: i64,
    /// Weight per bend above target.
    pub bends_per_bend: i64,
}

/// Per-net constraint collection.
#[derive(Debug, Clone)]
pub struct NetConstraints {
    /// Net ID these constraints apply to.
    pub net_id: NetId,
    /// Hard constraints (must satisfy).
    pub hard: HardConstraints,
    /// Soft constraints (optimize toward).
    pub soft: SoftConstraints,
    /// Trace width in nanometers.
    pub trace_width_nm: i64,
    /// Material ID for this net.
    pub material_id: u8,
}

/// Computed metrics for a routed path.
///
/// These are geometric properties only. Electrical properties (impedance,
/// delay, coupling) are computed by the physics engine, not the router.
#[derive(Debug, Clone, Default)]
pub struct RouteMetrics {
    /// Total path length in nanometers.
    pub total_length_nm: i64,
    /// Number of via transitions.
    pub via_count: usize,
    /// Number of bend angles (90-degree turns).
    pub bend_count: usize,
    /// Path waypoints.
    pub waypoints: Vec<Point3D>,
}

impl RouteMetrics {
    /// Compute geometric metrics from a path.
    ///
    /// This is a pure function - same inputs always produce same output.
    /// Electrical properties are NOT computed here; they belong to the physics engine.
    pub fn compute(path: &[Point3D]) -> Self {
        let mut metrics = Self {
            waypoints: path.to_vec(),
            ..Default::default()
        };

        if path.is_empty() {
            return metrics;
        }

        // Calculate total length from waypoints
        metrics.total_length_nm = path
            .windows(2)
            .map(|w| w[0].manhattan_distance(&w[1]))
            .sum();

        // Count via transitions (Z-axis changes)
        metrics.via_count = path.windows(2).filter(|w| w[0].z != w[1].z).count();

        // Count bend angles (direction changes)
        for window in path.windows(3) {
            let d1x = window[1].x - window[0].x;
            let d1y = window[1].y - window[0].y;
            let d2x = window[2].x - window[1].x;
            let d2y = window[2].y - window[1].y;

            // Manhattan routing: bends are direction changes
            if (d1x != 0 || d1y != 0)
                && (d2x != 0 || d2y != 0)
                && (d1x.signum() != d2x.signum() || d1y.signum() != d2y.signum())
            {
                metrics.bend_count += 1;
            }
        }

        metrics
    }

    /// Check for hard constraint violations.
    pub fn check_hard_violations(&self, constraints: &NetConstraints) -> Vec<Violation> {
        let mut violations = Vec::new();

        if self.via_count > constraints.hard.max_via_count {
            violations.push(Violation::HardViaCountExceeded {
                net_id: constraints.net_id,
                actual: self.via_count,
                limit: constraints.hard.max_via_count,
            });
        }

        violations
    }

    /// Compute the soft constraint penalty score.
    ///
    /// Returns the total weighted penalty across all soft constraints.
    /// Electrical penalties (impedance, crosstalk) belong in the physics engine,
    /// so only geometric penalties are computed here.
    pub fn compute_soft_penalty(&self, constraints: &NetConstraints) -> i64 {
        let mut penalty = 0i64;

        // Length deviation penalty
        if let Some(target_len) = constraints.soft.target_length_nm {
            let diff = (self.total_length_nm - target_len).abs();
            let tolerance = constraints.soft.length_tolerance_nm.unwrap_or(0);
            let over_tolerance = (diff - tolerance).max(0);
            penalty += over_tolerance * constraints.soft.penalty_weights.length_per_nm;
        }

        // Bend penalty
        penalty += (self.bend_count as i64) * constraints.soft.penalty_weights.bends_per_bend;

        penalty
    }
}

/// Constraint violation detected during verification.
#[derive(Debug, Clone)]
pub enum Violation {
    /// Hard constraint: clearance violation with specific location.
    HardClearanceViolation {
        net_id: NetId,
        location: Point3D,
        actual_nm: i64,
        required_nm: i64,
    },
    /// Hard constraint: via count exceeded.
    HardViaCountExceeded {
        net_id: NetId,
        actual: usize,
        limit: usize,
    },
    /// Hard constraint: path on non-routable layer.
    HardNonRoutableLayer { net_id: NetId, layer_name: String },
    /// Soft constraint: length deficit (needs more length).
    SoftLengthDeficit { net_id: NetId, deficit_nm: i64 },
    /// Soft constraint: length excess (too long).
    SoftLengthExcess { net_id: NetId, excess_nm: i64 },
    /// Soft constraint: impedance mismatch.
    SoftImpedanceMismatch {
        net_id: NetId,
        actual_ohm: f64,
        target_ohm: f64,
    },
    /// Soft constraint: excessive bends.
    SoftExcessiveBends {
        net_id: NetId,
        actual: usize,
        target: usize,
    },
    /// Soft constraint: crosstalk violation with adjacent net.
    SoftCrosstalkViolation {
        net_id: NetId,
        adjacent_net_id: NetId,
        coupling_coefficient: f64,
        max_coefficient: f64,
    },
}

impl Violation {
    /// Check if this is a hard constraint violation.
    pub fn is_hard(&self) -> bool {
        matches!(
            self,
            Violation::HardClearanceViolation { .. }
                | Violation::HardViaCountExceeded { .. }
                | Violation::HardNonRoutableLayer { .. }
        )
    }

    /// Get the net ID for this violation.
    pub fn net_id(&self) -> NetId {
        match self {
            Violation::HardClearanceViolation { net_id, .. }
            | Violation::HardViaCountExceeded { net_id, .. }
            | Violation::HardNonRoutableLayer { net_id, .. }
            | Violation::SoftLengthDeficit { net_id, .. }
            | Violation::SoftLengthExcess { net_id, .. }
            | Violation::SoftImpedanceMismatch { net_id, .. }
            | Violation::SoftExcessiveBends { net_id, .. }
            | Violation::SoftCrosstalkViolation { net_id, .. } => *net_id,
        }
    }

    /// Get the adjacent net ID if this is a crosstalk violation.
    pub fn adjacent_net_id(&self) -> Option<NetId> {
        match self {
            Violation::SoftCrosstalkViolation {
                adjacent_net_id, ..
            } => Some(*adjacent_net_id),
            _ => None,
        }
    }

    /// Get the coupling coefficient if this is a crosstalk violation.
    pub fn coupling_coefficient(&self) -> Option<f64> {
        match self {
            Violation::SoftCrosstalkViolation {
                coupling_coefficient,
                ..
            } => Some(*coupling_coefficient),
            _ => None,
        }
    }

    /// Describe the violation in human-readable form for error messages.
    pub fn describe(&self) -> String {
        match self {
            Violation::HardClearanceViolation {
                net_id,
                actual_nm,
                required_nm,
                ..
            } => {
                format!(
                    "Net {}: clearance {} nm is less than required {} nm",
                    net_id.0, actual_nm, required_nm
                )
            }
            Violation::HardViaCountExceeded {
                net_id,
                actual,
                limit,
            } => {
                format!(
                    "Net {}: via count {} exceeds limit {}",
                    net_id.0, actual, limit
                )
            }
            Violation::HardNonRoutableLayer { net_id, layer_name } => {
                format!(
                    "Net {}: placed on non-routable layer '{}'",
                    net_id.0, layer_name
                )
            }
            Violation::SoftLengthDeficit { net_id, deficit_nm } => {
                format!("Net {}: length deficit of {} nm", net_id.0, deficit_nm)
            }
            Violation::SoftLengthExcess { net_id, excess_nm } => {
                format!("Net {}: length excess of {} nm", net_id.0, excess_nm)
            }
            Violation::SoftImpedanceMismatch {
                net_id,
                actual_ohm,
                target_ohm,
            } => {
                format!(
                    "Net {}: impedance {} ohm differs from target {} ohm",
                    net_id.0, actual_ohm, target_ohm
                )
            }
            Violation::SoftExcessiveBends {
                net_id,
                actual,
                target,
            } => {
                format!(
                    "Net {}: {} bends exceeds target {}",
                    net_id.0, actual, target
                )
            }
            Violation::SoftCrosstalkViolation {
                net_id,
                adjacent_net_id,
                coupling_coefficient,
                max_coefficient,
            } => {
                format!(
                    "Net {} to net {}: coupling {} exceeds max {}",
                    net_id.0, adjacent_net_id.0, coupling_coefficient, max_coefficient
                )
            }
        }
    }
}

/// Check geometric constraints against metrics and return all violations.
///
/// Electrical constraints (impedance, crosstalk) are checked by the physics engine,
/// not here. This function only checks what the router can measure: geometry.
pub fn check_constraints(metrics: &RouteMetrics, constraints: &NetConstraints) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Check hard constraints
    violations.extend(metrics.check_hard_violations(constraints));

    // Check soft constraints (geometry only)
    if let Some(target_len) = constraints.soft.target_length_nm {
        let diff = metrics.total_length_nm - target_len;
        let tolerance = constraints.soft.length_tolerance_nm.unwrap_or(0);

        if diff < -tolerance {
            violations.push(Violation::SoftLengthDeficit {
                net_id: constraints.net_id,
                deficit_nm: (-diff) - tolerance,
            });
        } else if diff > tolerance {
            violations.push(Violation::SoftLengthExcess {
                net_id: constraints.net_id,
                excess_nm: diff - tolerance,
            });
        }
    }

    violations
}

/// v0.1.9: Validate interface-derived constraints against routing parameters.
/// Returns violations for any interface capability constraints that cannot be satisfied.
pub fn check_interface_constraints(
    constraints: &[DerivedConstraint],
    trace_width_nm: i64,
    trace_length_nm: Option<i64>,
) -> Vec<InterfaceViolation> {
    let mut violations = Vec::new();
    for constraint in constraints {
        match constraint {
            DerivedConstraint::MinimumTraceWidth(min_width) => {
                if trace_width_nm < *min_width {
                    violations.push(InterfaceViolation::TraceWidthTooNarrow {
                        actual_nm: trace_width_nm,
                        required_nm: *min_width,
                    });
                }
            }
            DerivedConstraint::MaximumTraceLength(max_length) => {
                if let Some(length) = trace_length_nm {
                    if length > *max_length {
                        violations.push(InterfaceViolation::TraceTooLong {
                            actual_nm: length,
                            max_nm: *max_length,
                        });
                    }
                }
            }
            DerivedConstraint::ThermalViaRequired => {
                // Thermal via is a recommendation, not a hard constraint
            }
            DerivedConstraint::None => {}
        }
    }
    violations
}

/// Violation of an interface capability constraint.
#[derive(Debug, Clone)]
pub enum InterfaceViolation {
    TraceWidthTooNarrow { actual_nm: i64, required_nm: i64 },
    TraceTooLong { actual_nm: i64, max_nm: i64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_kind_hard() {
        let c = ConstraintKind::Hard(100i64, "test constraint");
        assert!(c.is_hard());
        assert!(!c.is_soft());
        assert_eq!(*c.value(), 100);
        assert_eq!(c.weight(), 0);
    }

    #[test]
    fn test_constraint_kind_soft() {
        let c = ConstraintKind::Soft(100i64, 50, "test soft constraint");
        assert!(!c.is_hard());
        assert!(c.is_soft());
        assert_eq!(*c.value(), 100);
        assert_eq!(c.weight(), 50);
    }

    #[test]
    fn test_route_metrics_length() {
        let path = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(1000, 0, 0),
            Point3D::new(1000, 1000, 0),
        ];

        let metrics = RouteMetrics::compute(&path);
        assert_eq!(metrics.total_length_nm, 2000);
        assert_eq!(metrics.via_count, 0);
        assert_eq!(metrics.bend_count, 1);
    }

    #[test]
    fn test_soft_penalty_computation() {
        let path = vec![Point3D::new(0, 0, 0), Point3D::new(1000, 0, 0)];

        let constraints = NetConstraints {
            net_id: NetId::new(1),
            hard: HardConstraints {
                min_clearance_nm: 100,
                max_via_count: 10,
                min_bend_radius_nm: 50,
                allowed_layers: vec![],
            },
            soft: SoftConstraints {
                target_length_nm: Some(500),
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
        };

        let metrics = RouteMetrics::compute(&path);
        let penalty = metrics.compute_soft_penalty(&constraints);

        // 1000 - 500 = 500 deficit, tolerance 100, so 400 over tolerance
        // 400 * 10 = 4000 penalty
        assert_eq!(penalty, 4000);
    }
}
