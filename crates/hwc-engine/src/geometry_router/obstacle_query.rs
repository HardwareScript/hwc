//! Routing Obstacle Query System (v0.2.3)
//!
//! Centralizes all logic for determining whether a substrate layer should be
//! treated as an obstacle for routing. Replaces scattered conditional checks
//! with a single, queryable, well-documented system.
//!
//! ## Design Philosophy
//! 
//! Instead of littering obstacle-building code with ad-hoc conditions like:
//! ```ignore
//! if net_id != NetId::UNCONNECTED && net_id == route.net_id { continue; }
//! if layer_type == SubstrateLayerType::Substrate { continue; }
//! if device_binding.is_some() && ... { continue; }
//! ```
//!
//! We centralize ALL obstacle logic in a single struct that can be:
//! - Queried: `query.is_obstacle_for(layer, route)?`
//! - Tested: Clear semantics for each rule
//! - Extended: Add new rules without touching obstacle-building code
//! - Debugged: Single source of truth for why something is/isn't an obstacle
//!
//! ## Fail-Loud Philosophy
//!
//! **NO DEFAULTS. NO FALLBACKS. NO SILENT FAILURES.**
//! 
//! - Every decision must match an explicit rule
//! - Unknown states return `Result::Err` with diagnostic context
//! - Hardcoded assumptions are forbidden
//! - The compiler MUST crash if obstacle logic is ambiguous
//!
//! This ensures routing behavior is always intentional and debuggable.

use crate::geometry::BoundingBox;
use crate::geometry_router::substrate_types::SubstrateLayer;
use crate::netlist::NetId;
use hwc_physics::connectivity::SubstrateLayerType;
use std::fmt;

/// Routing context for obstacle queries
#[derive(Debug, Clone)]
pub struct RouteContext {
    pub net_id: NetId,
    pub start: crate::geometry::Point3D,
    pub goal: crate::geometry::Point3D,
    pub trace_width_nm: i64,
}

/// Error type for obstacle query failures
#[derive(Debug, Clone)]
pub enum ObstacleQueryError {
    /// Encountered a substrate layer type with no defined routing behavior
    UnhandledLayerType {
        layer_type: SubstrateLayerType,
        net_id: NetId,
        bbox: BoundingBox,
        hint: String,
    },
    /// Material type could not be classified as conductor/insulator
    UnclassifiedMaterial {
        material_id: u8,
        hint: String,
    },
    /// Invalid routing context (malformed input)
    InvalidContext {
        reason: String,
    },
}

impl fmt::Display for ObstacleQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObstacleQueryError::UnhandledLayerType { layer_type, net_id, bbox, hint } => {
                write!(
                    f,
                    "Routing obstacle query encountered unhandled layer type {:?} (net={:?}, bbox={:?}).\n\
                     This indicates a missing rule in the obstacle query system.\n\
                     Hint: {}",
                    layer_type, net_id, bbox, hint
                )
            }
            ObstacleQueryError::UnclassifiedMaterial { material_id, hint } => {
                write!(
                    f,
                    "Material ID {} could not be classified for routing obstacle logic.\n\
                     Hint: {}",
                    material_id, hint
                )
            }
            ObstacleQueryError::InvalidContext { reason } => {
                write!(f, "Invalid routing context: {}", reason)
            }
        }
    }
}

impl std::error::Error for ObstacleQueryError {}

/// Result of an obstacle query with detailed reasoning
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObstacleDecision {
    /// This layer IS an obstacle for the route
    IsObstacle {
        reason: ObstacleReason,
    },
    /// This layer is NOT an obstacle (exempted)
    Exempt {
        reason: ExemptionReason,
    },
}

/// Why a layer is considered an obstacle
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObstacleReason {
    /// Different net conductor (normal case)
    DifferentNetConductor,
    /// Keepout zone (net_id = 0)
    KeepoutZone,
    /// Component boundary
    ComponentBoundary,
}

/// Why a layer is exempted from being an obstacle
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExemptionReason {
    /// Same net as the route
    SameNet,
    /// Dielectric/insulating layer (not a conductor)
    DielectricLayer,
    /// Solder mask (cosmetic, not electrical)
    SolderMask,
    /// Route endpoint is docking into this pad
    RoutingEndpoint,
    /// Device-bound terminals of same device instance
    SameDeviceTerminal { device_name: String },
}

/// Central query system for routing obstacles
///
/// **CRITICAL: NO DEFAULTS, NO FALLBACKS**
/// Every substrate layer type MUST have an explicit rule.
/// Unknown states cause compilation failure.
pub struct ObstacleQuery;

impl ObstacleQuery {
    /// Determine if a substrate layer should be an obstacle for a given route
    ///
    /// This is the SINGLE SOURCE OF TRUTH for obstacle decisions.
    /// All obstacle-building code MUST call this instead of inline conditionals.
    ///
    /// # Returns
    /// - `Ok(ObstacleDecision)` - Explicit decision with reasoning
    /// - `Err(ObstacleQueryError)` - Ambiguous state, MUST be fixed in code
    ///
    /// # Panics
    /// Never. All ambiguous states return Err for explicit handling.
    pub fn is_obstacle_for(
        layer: &SubstrateLayer,
        context: &RouteContext,
    ) -> Result<ObstacleDecision, ObstacleQueryError> {
        // VALIDATION: Ensure context is well-formed
        if context.trace_width_nm <= 0 {
            return Err(ObstacleQueryError::InvalidContext {
                reason: format!(
                    "trace_width_nm must be positive, got {}",
                    context.trace_width_nm
                ),
            });
        }

        // RULE 1: Explicit layer type classification
        // Each layer type MUST have defined routing behavior.
        // NO catch-all match arms allowed.
        match layer.layer_type {
            // Dielectric layers (FR4, oxide, air gaps) are NEVER obstacles
            // Rationale: Non-conductive, routes pass through Z-dimension
            SubstrateLayerType::Substrate => {
                return Ok(ObstacleDecision::Exempt {
                    reason: ExemptionReason::DielectricLayer,
                });
            }

            // Solder mask (cosmetic coating) is NEVER an obstacle
            // Rationale: Not part of electrical connectivity
            SubstrateLayerType::SolderMask => {
                return Ok(ObstacleDecision::Exempt {
                    reason: ExemptionReason::SolderMask,
                });
            }

            // Pours and Contacts are conductive - apply net-based rules
            SubstrateLayerType::Pour | SubstrateLayerType::Contact => {
                // Continue to net-based logic below
            }
        }

        // RULE 2: Same-net conductors are not obstacles
        // Rationale: You can route over your own copper pours.
        // Exception: Keepout zones (net_id = 0) skip this rule.
        if layer.net != NetId::UNCONNECTED && layer.net == context.net_id {
            return Ok(ObstacleDecision::Exempt {
                reason: ExemptionReason::SameNet,
            });
        }

        // RULE 3: Keepout zones (net_id = 0) are ALWAYS obstacles
        // Rationale: Unconnected pours are intentional no-go zones.
        if layer.net == NetId::UNCONNECTED {
            return Ok(ObstacleDecision::IsObstacle {
                reason: ObstacleReason::KeepoutZone,
            });
        }

        // RULE 4: Routing endpoint exemption
        // Rationale: The goal anchor sits just outside the pad bbox.
        // After Minkowski inflation, the destination pad would swallow the goal.
        // We exempt pads that the start/goal is docking into.
        if layer.net != NetId::UNCONNECTED {
            let proximity = context.trace_width_nm / 2;

            // Check if goal is near this pad
            if Self::point_near_bbox(context.goal, &layer.bbox, proximity) {
                return Ok(ObstacleDecision::Exempt {
                    reason: ExemptionReason::RoutingEndpoint,
                });
            }

            // Check if start is near this pad
            if Self::point_near_bbox(context.start, &layer.bbox, proximity) {
                return Ok(ObstacleDecision::Exempt {
                    reason: ExemptionReason::RoutingEndpoint,
                });
            }
        }

        // RULE 5: Different-net conductor is an obstacle
        // This is the normal case: routing must avoid other nets' copper.
        Ok(ObstacleDecision::IsObstacle {
            reason: ObstacleReason::DifferentNetConductor,
        })
    }

    /// Check if a point is within proximity of a bounding box
    ///
    /// Expands XY boundaries by proximity, Z boundaries remain strict.
    fn point_near_bbox(
        point: crate::geometry::Point3D,
        bbox: &BoundingBox,
        proximity: i64,
    ) -> bool {
        point.x >= bbox.min.x - proximity
            && point.x <= bbox.max.x + proximity
            && point.y >= bbox.min.y - proximity
            && point.y <= bbox.max.y + proximity
            && point.z >= bbox.min.z
            && point.z <= bbox.max.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{BoundingBox, Point3D};

    fn make_layer(net_id: u32, layer_type: SubstrateLayerType) -> SubstrateLayer {
        SubstrateLayer::new(
            0, // material_id
            NetId::new(net_id),
            BoundingBox::new(
                Point3D::new(1000, 1000, 0),
                Point3D::new(2000, 2000, 100),
            ),
            layer_type,
        )
    }

    fn make_context(net_id: u32) -> RouteContext {
        RouteContext {
            net_id: NetId::new(net_id),
            start: Point3D::new(500, 500, 50),
            goal: Point3D::new(3000, 3000, 50),
            trace_width_nm: 500,
        }
    }

    #[test]
    fn test_dielectric_always_exempt() {
        let layer = make_layer(1, SubstrateLayerType::Substrate);
        let context = make_context(2); // Different net

        let decision = ObstacleQuery::is_obstacle_for(&layer, &context).expect("Query should succeed");
        assert_eq!(
            decision,
            ObstacleDecision::Exempt {
                reason: ExemptionReason::DielectricLayer
            }
        );
    }

    #[test]
    fn test_same_net_exempt() {
        let layer = make_layer(1, SubstrateLayerType::Pour);
        let context = make_context(1); // Same net

        let decision = ObstacleQuery::is_obstacle_for(&layer, &context).expect("Query should succeed");
        assert_eq!(
            decision,
            ObstacleDecision::Exempt {
                reason: ExemptionReason::SameNet
            }
        );
    }

    #[test]
    fn test_different_net_is_obstacle() {
        let layer = make_layer(1, SubstrateLayerType::Pour);
        let context = make_context(2); // Different net

        let decision = ObstacleQuery::is_obstacle_for(&layer, &context).expect("Query should succeed");
        assert_eq!(
            decision,
            ObstacleDecision::IsObstacle {
                reason: ObstacleReason::DifferentNetConductor
            }
        );
    }

    #[test]
    fn test_keepout_zone_always_obstacle() {
        let layer = make_layer(0, SubstrateLayerType::Pour); // net_id = 0
        let context = make_context(1);

        let decision = ObstacleQuery::is_obstacle_for(&layer, &context).expect("Query should succeed");
        assert_eq!(
            decision,
            ObstacleDecision::IsObstacle {
                reason: ObstacleReason::KeepoutZone
            }
        );
    }

    #[test]
    fn test_routing_endpoint_exempt() {
        let layer = make_layer(2, SubstrateLayerType::Pour);
        let mut context = make_context(1);

        // Place goal inside layer bbox
        context.goal = Point3D::new(1500, 1500, 50);

        let decision = ObstacleQuery::is_obstacle_for(&layer, &context).expect("Query should succeed");
        assert_eq!(
            decision,
            ObstacleDecision::Exempt {
                reason: ExemptionReason::RoutingEndpoint
            }
        );
    }

    #[test]
    fn test_invalid_context_fails_loudly() {
        let layer = make_layer(1, SubstrateLayerType::Pour);
        let mut context = make_context(1);
        context.trace_width_nm = 0; // Invalid!

        let result = ObstacleQuery::is_obstacle_for(&layer, &context);
        assert!(result.is_err(), "Should fail on invalid context");
        
        if let Err(ObstacleQueryError::InvalidContext { reason }) = result {
            assert!(reason.contains("trace_width_nm"));
        } else {
            panic!("Expected InvalidContext error");
        }
    }
}
