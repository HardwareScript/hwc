//! Extensible Cost Evaluation Architecture
//!
//! Provides enum-based dispatch cost evaluators with zero-cost abstraction.
//! Uses `SmallVec` for stack-allocated storage in the common case.
//!
//! Reference: `Docs/v0.1.9/Connection-Interface-Routing.md` §4

use crate::geometry::Point3D;
use crate::geometry_router::connection_interface::RoutingDatabase;
use crate::geometry_router::pathfinding::RoutingParams;
use smallvec::{smallvec, SmallVec};

/// Cost evaluator types (enum dispatch, not trait objects).
///
/// Each variant encapsulates a specific cost contribution. The router
/// composes evaluators into a `CostComposer` for the routing inner loop.
#[derive(Debug, Clone)]
pub enum CostEvaluator {
    /// Base geometric movement cost (always applied)
    GeometricMove,

    /// Via transition penalty (layer change)
    ViaTransition {
        /// Penalty points for this via transition
        penalty: i64,
    },

    /// Direction penalty (against preferred layer direction)
    Direction {
        /// Penalty points for moving against preferred direction
        penalty: i64,
    },

    /// Thermal hotspot avoidance
    Thermal {
        /// Temperature threshold in millikelvin (above this = penalty)
        threshold_mk: i64,
        /// Penalty points when threshold exceeded
        penalty: i64,
    },

    /// Electromigration risk zones
    Electromigration {
        /// Current density limit in uA/nm²
        current_density_limit: i64,
        /// Penalty points when limit exceeded
        penalty: i64,
    },

    /// Crosstalk risk (parallel trace proximity)
    Crosstalk {
        /// Minimum spacing in nanometers
        min_spacing_nm: i64,
        /// Penalty points when too close
        penalty: i64,
    },

    /// Reference plane void crossing
    ReferenceVoid {
        /// Penalty points for crossing a void
        penalty: i64,
    },
}

impl CostEvaluator {
    /// Evaluate cost at a specific position.
    ///
    /// Fully inlined for zero-cost abstraction in the routing inner loop.
    #[inline]
    pub fn evaluate(&self, db: &dyn RoutingDatabase, pos: Point3D) -> i64 {
        match self {
            Self::GeometricMove => 1,

            Self::ViaTransition { penalty } => *penalty,

            Self::Direction { penalty } => *penalty,

            Self::Thermal {
                threshold_mk,
                penalty,
            } => {
                let temp = db.get_local_temperature_at(pos);
                if temp > *threshold_mk {
                    *penalty
                } else {
                    0
                }
            }

            Self::Electromigration {
                current_density_limit,
                penalty,
            } => {
                let density = db.get_current_density_at(pos);
                if density > *current_density_limit {
                    *penalty
                } else {
                    0
                }
            }

            Self::Crosstalk {
                min_spacing_nm,
                penalty,
            } => {
                let spacing = db.get_nearest_parallel_trace_distance(pos);
                if spacing < *min_spacing_nm {
                    *penalty
                } else {
                    0
                }
            }

            Self::ReferenceVoid { penalty } => {
                if db.is_in_reference_void(pos) {
                    *penalty
                } else {
                    0
                }
            }
        }
    }
}

/// Cost composer with stack-allocated storage for the common case.
///
/// Uses `SmallVec<[CostEvaluator; 8]>` to avoid heap allocation for
/// up to 8 evaluators (covers all practical routing scenarios).
/// The composer accumulates total path cost across all evaluators
/// with no virtual dispatch overhead.
pub struct CostComposer {
    evaluators: SmallVec<[CostEvaluator; 8]>,
}

impl CostComposer {
    /// Create a new composer with the base geometric move evaluator.
    pub fn new() -> Self {
        Self {
            evaluators: smallvec![CostEvaluator::GeometricMove],
        }
    }

    /// Build a CostComposer from routing intent with explicit override values.
    /// If the intent has cost_weights, use those; otherwise fall back to base values.
    #[inline]
    pub fn from_intent_overrides(
        via_penalty: i64,
        direction_penalty: i64,
        crosstalk_penalty: i64,
        reference_void_penalty: i64,
    ) -> Self {
        Self {
            evaluators: smallvec![
                CostEvaluator::GeometricMove,
                CostEvaluator::ViaTransition {
                    penalty: via_penalty
                },
                CostEvaluator::Direction {
                    penalty: direction_penalty
                },
                CostEvaluator::Crosstalk {
                    min_spacing_nm: 500,
                    penalty: crosstalk_penalty
                },
                CostEvaluator::ReferenceVoid {
                    penalty: reference_void_penalty
                },
            ],
        }
    }

    /// Build a CostComposer from routing parameter heuristic weights.
    #[inline]
    pub fn from_routing_params(params: &RoutingParams) -> Self {
        Self::from_intent_overrides(
            params.via_penalty,
            params.direction_penalty,
            params.crosstalk_penalty,
            params.reference_void_penalty,
        )
    }

    /// Add an evaluator to the composer (builder pattern).
    pub fn with_evaluator(mut self, evaluator: CostEvaluator) -> Self {
        self.evaluators.push(evaluator);
        self
    }

    /// Accumulate total path cost across all evaluators.
    ///
    /// Fully inlined, no virtual dispatch. The inner loop is:
    /// for each evaluator: sum += evaluate(db, pos)
    #[inline]
    pub fn calculate_step_cost(&self, db: &dyn RoutingDatabase, pos: Point3D) -> i64 {
        self.evaluators
            .iter()
            .map(|eval| eval.evaluate(db, pos))
            .sum()
    }

    /// Number of evaluators in this composer.
    #[inline]
    pub fn len(&self) -> usize {
        self.evaluators.len()
    }

    /// Whether the composer has no evaluators (should never happen).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.evaluators.is_empty()
    }
}

impl Default for CostComposer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry_router::connection_interface::DefaultRoutingDatabase;

    #[test]
    fn test_geometric_move_cost() {
        let db = DefaultRoutingDatabase::default();
        let evaluator = CostEvaluator::GeometricMove;
        let pos = Point3D::new(0, 0, 0);
        assert_eq!(evaluator.evaluate(&db, pos), 1);
    }

    #[test]
    fn test_via_transition_cost() {
        let db = DefaultRoutingDatabase::default();
        let evaluator = CostEvaluator::ViaTransition { penalty: 50_000 };
        let pos = Point3D::new(0, 0, 0);
        assert_eq!(evaluator.evaluate(&db, pos), 50_000);
    }

    #[test]
    fn test_thermal_below_threshold() {
        let db = DefaultRoutingDatabase::default();
        let evaluator = CostEvaluator::Thermal {
            threshold_mk: 400_000, // 400K
            penalty: 10_000,
        };
        let pos = Point3D::new(0, 0, 0);
        // Default DB returns 300K, below 400K threshold
        assert_eq!(evaluator.evaluate(&db, pos), 0);
    }

    #[test]
    fn test_thermal_above_threshold() {
        let db = DefaultRoutingDatabase::default();
        let evaluator = CostEvaluator::Thermal {
            threshold_mk: 200_000, // 200K
            penalty: 10_000,
        };
        let pos = Point3D::new(0, 0, 0);
        // Default DB returns 300K, above 200K threshold
        assert_eq!(evaluator.evaluate(&db, pos), 10_000);
    }

    #[test]
    fn test_cost_composer_basic() {
        let db = DefaultRoutingDatabase::default();
        let composer = CostComposer::new();
        let pos = Point3D::new(0, 0, 0);
        // Only GeometricMove = 1
        assert_eq!(composer.calculate_step_cost(&db, pos), 1);
    }

    #[test]
    fn test_cost_composer_with_via() {
        let db = DefaultRoutingDatabase::default();
        let composer =
            CostComposer::new().with_evaluator(CostEvaluator::ViaTransition { penalty: 50_000 });
        let pos = Point3D::new(0, 0, 0);
        // GeometricMove (1) + ViaTransition (50000) = 50001
        assert_eq!(composer.calculate_step_cost(&db, pos), 50_001);
    }

    #[test]
    fn test_cost_composer_multiple_evaluators() {
        let db = DefaultRoutingDatabase::default();
        let composer = CostComposer::new()
            .with_evaluator(CostEvaluator::ViaTransition { penalty: 50_000 })
            .with_evaluator(CostEvaluator::Direction { penalty: 10 })
            .with_evaluator(CostEvaluator::ReferenceVoid { penalty: 5_000_000 });
        let pos = Point3D::new(0, 0, 0);
        // 1 + 50000 + 10 + 0 (not in void) = 50011
        assert_eq!(composer.calculate_step_cost(&db, pos), 50_011);
    }

    #[test]
    fn test_cost_composer_len() {
        let composer = CostComposer::new()
            .with_evaluator(CostEvaluator::ViaTransition { penalty: 10 })
            .with_evaluator(CostEvaluator::Direction { penalty: 5 });
        assert_eq!(composer.len(), 3); // GeometricMove + Via + Direction
        assert!(!composer.is_empty());
    }
}
