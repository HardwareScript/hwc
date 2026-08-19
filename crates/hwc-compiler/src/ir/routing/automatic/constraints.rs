//! Constraint evaluation and material resolution for automatic routing.
//!
//! Phase 1 of the routing pipeline: resolves fabrication constraints,
//! current limits, and trace width from the PDK profile.

use crate::ir::errors::IrError;
use hwc_engine::{HardwareSpace, Point3D};

/// Result of constraint evaluation for a single route.
pub struct ConstraintResult {
    /// Minimum clearance between traces (nm).
    pub min_clearance_nm: i64,
    /// Route current limit (mA).
    pub current_ma: f64,
    /// Resolved trace width (nm).
    pub trace_width_nm: i64,
    /// Resolved perpendicular escape stub length (nm) - v0.1.9 Declarative Escape Policies.
    /// Authority hierarchy: Profile Default → Net Type Intent → Route Override
    pub escape_stub_nm: i64,
}

/// Evaluate routing constraints from the PDK profile and route declaration.
///
/// Extracts clearance, current limit, trace width, and thermal parameters.
pub fn evaluate_constraints(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<ConstraintResult, IrError> {
    let min_clearance_nm = space.fabrication_constraints.as_ref()
        .map(|c| c.trace.min_spacing_nm)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Route requires fabrication constraints for clearance calculation but none are loaded.".into(),
            hint: "Declare a profile with 'clearance:' constraints in the space definition.".into(),
        })?;

    let current_ma: f64 = if let Some(ref ac) = route.current_limit_ac {
        let _rms = crate::ir::conversions::evaluate_expression_to_ma(&ac.rms, symbol_table)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "current_limit_ac.rms".into(),
                reason: e.to_string(),
            })?;

        crate::ir::conversions::evaluate_expression_to_ma(&ac.peak, symbol_table).map_err(|e| {
            IrError::InvalidRouteExpression {
                expression: "current_limit_ac.peak".into(),
                reason: e.to_string(),
            }
        })?
    } else {
        return Err(IrError::MissingAsicConstraint {
            message:
                "Route has no current_limit declaration. All routes must declare current capacity."
                    .into(),
            hint:
                "Add 'current_limit_ac: { rms: <value>, peak: <value> }' to the route declaration."
                    .into(),
        });
    };

    let is_external = true;

    let temp_rise_c = profile
        .and_then(|p| p.thermal.as_ref())
        .map(|t| t.max_temp_rise.value as i64)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Route requires thermal constraints for trace width calculation but none are declared.".into(),
            hint: "Declare 'thermal: { max_temp_rise: <value> }' in the profile.".into(),
        })?;

    let _min_trace_width_nm = hwc_engine::constraint_manager::calculate_trace_width_nm(
        current_ma as i64,
        temp_rise_c,
        is_external,
    );

    let trace_width_nm = if let Some(width_expr) = &route.width {
        crate::ir::conversions::evaluate_expression_to_nm(width_expr, symbol_table, eval_context)
            .map_err(IrError::InvalidExpression)?
    } else {
        profile.and_then(|p| p.trace.as_ref())
            .map(|t| crate::ir::conversions::measurement_to_nm(&t.min_width, symbol_table, eval_context))
            .transpose()
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "profile trace width".into(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Route has no explicit width and PDK has no 'trace.min_width' constraint".into(),
                hint: "Add 'width: <value>' to the route, or declare 'trace: min_width: <value>' in the profile.".into(),
            })?
    };

    if route.current_limit_ac.is_none() {
        let from_name = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
        let to_name = crate::ir::routing::helpers::construct_entity_name(&route.to)?;
        eprintln!(
            "[ROUTER] WARNING: Net {} -> {} has no current_limit declared. DRC will skip current-density check.",
            from_name, to_name
        );
    }

    // v0.1.9: Resolve escape_stub with authority hierarchy:
    // 1. Route-level override (highest priority)
    // 2. Net type intent override
    // 3. Profile default (required - no fallback)
    let escape_stub_nm = if let Some(ref stub_expr) = route.escape_stub {
        // Route-level override (highest authority)
        crate::ir::conversions::evaluate_expression_to_nm(stub_expr, symbol_table, eval_context)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "escape_stub".into(),
                reason: e.to_string(),
            })?
    } else if let Some(ref intent_name) = route.intent {
        // Net type intent override
        if let Some(intent) = profile.and_then(|p| {
            p.intents
                .iter()
                .find(|i| i.name.name == intent_name.as_str())
        }) {
            if let Some(ref stub_meas) = intent.escape_stub {
                crate::ir::conversions::measurement_to_nm(stub_meas, symbol_table, eval_context)
                    .map_err(|e| IrError::InvalidRouteExpression {
                        expression: format!("intent '{}' escape_stub", intent_name),
                        reason: e.to_string(),
                    })?
            } else {
                // Intent doesn't override - fall through to profile default
                profile
                    .and_then(|p| p.routing.as_ref())
                    .and_then(|r| r.escape_stub.as_ref())
                    .map(|m| crate::ir::conversions::measurement_to_nm(m, symbol_table, eval_context))
                    .transpose()
                    .map_err(|e| IrError::InvalidRouteExpression {
                        expression: "profile routing.escape_stub".into(),
                        reason: e.to_string(),
                    })?
                    .ok_or_else(|| IrError::MissingAsicConstraint {
                        message: "Route requires 'escape_stub' but it is not declared in the profile.".into(),
                        hint: "Add 'escape_stub: <value>' to the 'routing:' block in your profile.\n\nExample:\n  routing:\n    min_segment_length: 180nm\n    escape_stub: 0nm  # for immediate turns, or >0nm for perpendicular escape".into(),
                    })?
            }
        } else {
            // Intent not found - fall through to profile default
            profile
                .and_then(|p| p.routing.as_ref())
                .and_then(|r| r.escape_stub.as_ref())
                .map(|m| crate::ir::conversions::measurement_to_nm(m, symbol_table, eval_context))
                .transpose()
                .map_err(|e| IrError::InvalidRouteExpression {
                    expression: "profile routing.escape_stub".into(),
                    reason: e.to_string(),
                })?
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: "Route requires 'escape_stub' but it is not declared in the profile.".into(),
                    hint: "Add 'escape_stub: <value>' to the 'routing:' block in your profile.\n\nExample:\n  routing:\n    min_segment_length: 180nm\n    escape_stub: 0nm  # for immediate turns, or >0nm for perpendicular escape".into(),
                })?
        }
    } else {
        // No route override, no intent - use profile default (REQUIRED)
        profile
            .and_then(|p| p.routing.as_ref())
            .and_then(|r| r.escape_stub.as_ref())
            .map(|m| crate::ir::conversions::measurement_to_nm(m, symbol_table, eval_context))
            .transpose()
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "profile routing.escape_stub".into(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "Route requires 'escape_stub' but it is not declared in the profile.".into(),
                hint: "Add 'escape_stub: <value>' to the 'routing:' block in your profile.\n\nExample:\n  routing:\n    min_segment_length: 180nm\n    escape_stub: 0nm  # for immediate turns, or >0nm for perpendicular escape".into(),
            })?
    };

    Ok(ConstraintResult {
        min_clearance_nm,
        current_ma,
        trace_width_nm,
        escape_stub_nm,
    })
}

/// Resolve the conductor material directly from the routing layer definition.
///
/// Pure type-driven resolution from the RoutingLayerDatabase (single source of truth).
/// Fails loudly if the layer does not exist in the routing layer database.
pub fn resolve_material_for_layer(
    layer_name: &str,
    routing_layer_db: &hwc_engine::RoutingLayerDatabase,
) -> Result<hwc_engine::material::MaterialId, IrError> {
    routing_layer_db
        .get_layer(layer_name)
        .map(|layer| layer.material_id)
        .map_err(|_| IrError::InvalidRoutingLayer {
            layer: layer_name.into(),
            available_layers: routing_layer_db.list_routable_layers().join(", ").into(),
        })
}

/// Resolve target layer override from route declaration.
///
/// Queries the routing layer database for the exact Z coordinate.
/// ZERO heuristics, ZERO fallbacks. If the layer is not routable or missing,
/// routing immediately fails with an error.
pub fn resolve_target_layer(
    route: &hwc_parser::Route,
    routing_layer_db: &hwc_engine::RoutingLayerDatabase,
    _stackup_manager: &crate::ir::stackup_manager::StackupManager,
    start_boundary: Point3D,
) -> Result<Option<i64>, IrError> {
    if let Some(ref layer_id) = route.layer {
        let layer_name = layer_id.name.as_str();
        eprintln!(
            "[ROUTER] route.layer='{}' specified, resolving Z from routing layer database...",
            layer_name
        );

        let z = routing_layer_db
            .get_routing_z(layer_name)
            .map_err(|_| IrError::InvalidRoutingLayer {
                layer: layer_name.into(),
                available_layers: routing_layer_db.list_routable_layers().join(", ").into(),
            })?;

        eprintln!(
            "[ROUTER] Resolved layer '{}' -> Z={}nm from routing layer database (pin_z={})",
            layer_name, z, start_boundary.z
        );

        Ok(Some(z))
    } else {
        Ok(None)
    }
}
