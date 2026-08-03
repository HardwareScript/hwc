//! Salsa Query Engine (v0.1.9 Phase 1)
//!
//! Demand-driven incremental computation framework using the Salsa library.
//! Provides memoized query execution with automatic dependency tracking and
//! granular invalidation.
//!
//! ## Architecture
//! - `RoutingDatabase`: Main Salsa database trait
//! - `RoutingContextInput`: Penalty weights for localized repair
//! - `NetConstraintsInput`: Per-net constraint specifications
//! - `GCellObstaclesInput`: Per-G-cell obstacle data
//! - `StackupProfileInput`: Stackup layer definitions
//!
//! **Key Principles**:
//! - All database access is immutable (`&dyn RoutingDatabase`)
//! - Paths are wrapped in `Arc` for zero-cost cloning
//! - Mutations happen to local variables only
//! - Pure functions - same inputs always produce same output

use std::sync::Arc;

use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_engine::geometry_router::constraints::NetConstraints;
use hwc_engine::geometry_router::partition::GCellId;
use hwc_engine::netlist::NetId;
use hwc_types::Technology;

use crate::ir::errors::IrError;

// ============================================================================
// Salsa Input Structures
// ============================================================================

/// Penalty weights for localized repair.
///
/// Changing this input invalidates the query cache, allowing alternative
/// routing without violating Salsa's pure functional model.
/// No defaults - every field must be explicitly declared.
#[derive(Debug, Clone)]
pub struct RoutingPenalties {
    /// Explicit edges to avoid (bounding boxes).
    pub blocked_edges: Vec<BoundingBox>,
    /// GCell ID -> Penalty Cost (higher = avoid more).
    pub cell_weights: rustc_hash::FxHashMap<usize, i64>,
    /// Tracks repair attempts for this net.
    pub attempt_count: u32,
}

/// Salsa input for routing context with penalties.
///
/// THIS is the key to localized repair - changing this invalidates the query.
#[derive(Debug, Clone)]
pub struct RoutingContextInput {
    /// G-cell identifier.
    pub gcell_id: GCellId,
    /// Net identifier.
    pub net_id: NetId,
    /// Penalty weights for repair attempts.
    pub penalties: Arc<RoutingPenalties>,
    /// Trace width in nanometers.
    pub trace_width_nm: i64,
    /// Minimum clearance in nanometers.
    pub min_clearance_nm: i64,
    /// Technology (PCB or ASIC).
    pub technology_strategy: Technology,
    /// Board bounding box in nanometers.
    pub board_bounds: BoundingBox,
}

/// Salsa input for net constraints.
#[derive(Debug, Clone)]
pub struct NetConstraintsInput {
    /// Net identifier.
    pub net_id: NetId,
    /// Net constraints from PDK profile.
    pub constraints: Arc<NetConstraints>,
}

/// Salsa input for G-cell obstacles.
#[derive(Debug, Clone)]
pub struct GCellObstaclesInput {
    /// G-cell identifier.
    pub gcell_id: GCellId,
    /// Obstacle bounding boxes in this G-cell.
    pub obstacles: Arc<Vec<BoundingBox>>,
}

/// Salsa input for stackup profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackupProfileInput {
    /// Layer name.
    pub layer_name: Arc<str>,
    /// Start Z position in nanometers.
    pub start_z_nm: i64,
    /// Thickness in nanometers.
    pub thickness_nm: i64,
    /// Material name.
    pub material: Arc<str>,
    /// Whether this layer is routable.
    pub routable: bool,
}

// ============================================================================
// Query Result Types
// ============================================================================

/// Result of topological corridor extraction.
#[derive(Debug, Clone)]
pub struct CorridorResult {
    /// Waypoints defining the corridor.
    pub waypoints: Arc<Vec<Point3D>>,
    /// Total path length in nanometers.
    pub total_length_nm: i64,
    /// Number of via transitions.
    pub via_count: usize,
}

/// Result of obstacle query.
#[derive(Debug, Clone)]
pub struct ObstaclesResult {
    /// Obstacle bounding boxes.
    pub obstacles: Arc<Vec<BoundingBox>>,
}

/// Result of constraint query.
#[derive(Debug, Clone)]
pub struct ConstraintsResult {
    /// Net constraints.
    pub constraints: Arc<NetConstraints>,
}

/// Result of metrics computation.
#[derive(Debug, Clone)]
pub struct MetricsResult {
    /// Total path length in nanometers.
    pub total_length_nm: i64,
    /// Number of via transitions.
    pub via_count: usize,
    /// Number of bends.
    pub bend_count: usize,
}

// ============================================================================
// Salsa Database Trait
// ============================================================================

/// Main Salsa database trait for routing queries.
///
/// This trait must be implemented by the concrete database struct.
/// It extends `salsa::ParallelDatabase` for thread-safe parallel query execution.
pub trait RoutingDatabase: salsa::Database {
    /// Query net constraints by net ID.
    fn net_constraints(&self, net_id: NetId) -> Arc<NetConstraints>;

    /// Query G-cell obstacles by G-cell ID.
    fn gcell_obstacles(&self, gcell_id: GCellId) -> Arc<Vec<BoundingBox>>;

    /// Query stackup profile by layer name.
    fn stackup_profile(&self, layer_name: &str) -> Option<StackupProfileInput>;

    /// Extract topological corridor for a net in a G-cell.
    fn extract_topological_corridor(
        &self,
        context: &RoutingContextInput,
        entry_port: Point3D,
        exit_port: Point3D,
    ) -> Result<CorridorResult, IrError>;

    /// Compute route metrics for a path.
    fn compute_route_metrics(&self, path: &[Point3D], net_id: NetId) -> MetricsResult;
}

// ============================================================================
// Query Implementation Helpers
// ============================================================================

/// Extract topological corridor using the spatial decomposer.
///
/// This function implements the Delta Pattern:
/// - Takes immutable references to the database
/// - Returns a pure, detached result
/// - No global state mutation
pub fn extract_corridor_impl(
    context: &RoutingContextInput,
    entry_port: Point3D,
    exit_port: Point3D,
    obstacles: &[BoundingBox],
    penalties: &RoutingPenalties,
    adjacent_obstacles: &[BoundingBox],
) -> Result<CorridorResult, IrError> {
    use hwc_engine::geometry_router::navigable_space::SpatialDecomposer;

    // Use user-declared parameters from context
    let decomposer = SpatialDecomposer::new(
        obstacles.to_vec(),
        context.trace_width_nm,
        context.min_clearance_nm,
        context.technology_strategy,
    )
    .map_err(|e| IrError::NavigableSpaceFailed {
        gcell_id: context.gcell_id.0,
        reason: e.to_string(),
    })?;

    // Decompose C-Space
    let cells = decomposer.decompose(&context.board_bounds, entry_port.z);

    // Extract corridor
    let corridor = decomposer
        .extract_corridor(entry_port, exit_port, &cells)
        .map_err(|_| IrError::CorridorExtractionFailed {
            gcell_id: context.gcell_id.0,
            start_x: entry_port.x,
            start_y: entry_port.y,
            start_z: entry_port.z,
            end_x: exit_port.x,
            end_y: exit_port.y,
            end_z: exit_port.z,
        })?;

    // Validate corridor width (Phase 5.2)
    if !decomposer.is_corridor_sufficient(&corridor, &cells) {
        // Corridor too narrow - try adjacent G-cells (Phase 4.2)
        if !adjacent_obstacles.is_empty() {
            let combined: Vec<BoundingBox> = obstacles
                .iter()
                .chain(adjacent_obstacles.iter())
                .cloned()
                .collect();
            let expanded_decomposer =
                SpatialDecomposer::new(combined, context.trace_width_nm, context.min_clearance_nm, context.technology_strategy)
                    .map_err(|e| IrError::NavigableSpaceFailed {
                        gcell_id: context.gcell_id.0,
                        reason: e.to_string(),
                    })?;
            let expanded_cells = expanded_decomposer.decompose(&context.board_bounds, entry_port.z);
            if let Ok(expanded_corridor) =
                expanded_decomposer.extract_corridor(entry_port, exit_port, &expanded_cells)
            {
                if expanded_decomposer.is_corridor_sufficient(&expanded_corridor, &expanded_cells) {
                    let waypoints = expanded_decomposer
                        .corridor_to_waypoints(&expanded_corridor, &expanded_cells);
                    let adjusted = apply_penalties_to_waypoints(&waypoints, penalties);
                    let total_length = adjusted
                        .windows(2)
                        .map(|w| w[0].manhattan_distance(&w[1]))
                        .sum();
                    return Ok(CorridorResult {
                        waypoints: Arc::new(adjusted),
                        total_length_nm: total_length,
                        via_count: 0,
                    });
                }
            }

            // Adjacent G-cells also insufficient
            let required_width = context.trace_width_nm + (2 * context.min_clearance_nm);
            let actual_width = decomposer.corridor_width(&corridor, &cells);
            return Err(IrError::CorridorTooNarrow {
                gcell_id: context.gcell_id.0,
                actual_nm: actual_width,
                required_nm: required_width,
            });
        }

        // No adjacent G-cells available
        let required_width = context.trace_width_nm + (2 * context.min_clearance_nm);
        let actual_width = decomposer.corridor_width(&corridor, &cells);
        return Err(IrError::CorridorTooNarrow {
            gcell_id: context.gcell_id.0,
            actual_nm: actual_width,
            required_nm: required_width,
        });
    }

    // Convert to waypoints
    let waypoints = decomposer.corridor_to_waypoints(&corridor, &cells);

    // Apply penalties to waypoints
    let adjusted_waypoints = apply_penalties_to_waypoints(&waypoints, penalties);

    let total_length = adjusted_waypoints
        .windows(2)
        .map(|w| w[0].manhattan_distance(&w[1]))
        .sum();

    Ok(CorridorResult {
        waypoints: Arc::new(adjusted_waypoints),
        total_length_nm: total_length,
        via_count: 0,
    })
}

/// Apply penalties to waypoints by adjusting coordinates.
fn apply_penalties_to_waypoints(
    waypoints: &[Point3D],
    penalties: &RoutingPenalties,
) -> Vec<Point3D> {
    if penalties.cell_weights.is_empty() {
        return waypoints.to_vec();
    }

    // Simple penalty application: shift waypoints away from high-penalty cells
    waypoints
        .iter()
        .map(|wp| {
            let mut adjusted = *wp;
            for (&_cell_id, &weight) in &penalties.cell_weights {
                if weight > 0 {
                    // Shift away from penalty (simplified)
                    adjusted.x += weight.signum() * 100;
                }
            }
            adjusted
        })
        .collect()
}

/// Compute route metrics for a path.
pub fn compute_metrics_impl(path: &[Point3D], _constraints: &NetConstraints) -> MetricsResult {
    use hwc_engine::geometry_router::constraints::RouteMetrics;

    let metrics = RouteMetrics::compute(path);

    MetricsResult {
        total_length_nm: metrics.total_length_nm,
        via_count: metrics.via_count,
        bend_count: metrics.bend_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_context_input() {
        let context = RoutingContextInput {
            gcell_id: GCellId::new(0),
            net_id: NetId::new(1),
            penalties: Arc::new(RoutingPenalties {
                blocked_edges: Vec::new(),
                cell_weights: rustc_hash::FxHashMap::default(),
                attempt_count: 0,
            }),
            trace_width_nm: 100,
            min_clearance_nm: 50,
            technology_strategy: TechnologyStrategy::Asic, // ASIC mode for test
            board_bounds: BoundingBox::new(
                Point3D::new(0, 0, 0),
                Point3D::new(100_000, 100_000, 0),
            ),
        };

        assert_eq!(context.gcell_id.0, 0);
        assert_eq!(context.net_id.0, 1);
    }

    #[test]
    fn test_extract_corridor_with_no_obstacles() {
        let context = RoutingContextInput {
            gcell_id: GCellId::new(0),
            net_id: NetId::new(1),
            penalties: Arc::new(RoutingPenalties {
                blocked_edges: Vec::new(),
                cell_weights: rustc_hash::FxHashMap::default(),
                attempt_count: 0,
            }),
            trace_width_nm: 100,
            min_clearance_nm: 50,
            technology_strategy: TechnologyStrategy::Asic, // ASIC mode for test
            board_bounds: BoundingBox::new(
                Point3D::new(0, 0, 0),
                Point3D::new(100_000, 100_000, 0),
            ),
        };

        let entry = Point3D::new(1000, 1000, 0);
        let exit = Point3D::new(50_000, 50_000, 0);
        let obstacles = Vec::new();
        let penalties = RoutingPenalties {
            blocked_edges: Vec::new(),
            cell_weights: rustc_hash::FxHashMap::default(),
            attempt_count: 0,
        };

        let result = extract_corridor_impl(&context, entry, exit, &obstacles, &penalties, &[]);
        assert!(result.is_ok());

        let corridor = result.unwrap();
        assert!(!corridor.waypoints.is_empty());
        assert!(corridor.total_length_nm >= 0);
    }
}
