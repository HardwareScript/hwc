//! Layer, thickness, and elevation resolution for plane placement.

use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use hwc_parser::{Elevation, PlanePlacement};

/// The plane's resolved vertical extent.
pub struct ResolvedElevation {
    /// Semantic layer name the plane sits on (e.g. `top_copper`, `metal1`).
    pub layer_name: String,
    /// Bottom Z of the plane in nanometers.
    pub z_start_nm: i64,
    /// Top Z of the plane in nanometers.
    pub z_end_nm: i64,
}

/// Resolve the plane's layer name, thickness, and Z extents.
///
/// Thickness precedence:
///   1. Explicit `thickness:` property on the plane
///   2. The profile's declared thickness for the layer
///   3. The stackup manager's layer thickness
///
/// Errors when no thickness can be resolved and none was declared explicitly.
pub fn resolve_elevation(
    plane: &PlanePlacement,
    ctx: &PlacementContext,
) -> Result<ResolvedElevation, IrError> {
    let layer_name = resolve_layer_name(&plane.elevation);
    let thickness_nm = resolve_thickness(plane, &layer_name, ctx)?;

    if thickness_nm == 0 && plane.thickness.is_none() {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Could not resolve physical thickness for plane '{}' on layer '{}'. \
                 Ensure the layer is defined in the profile stackup or provide an explicit 'thickness:' property.",
                plane.name, layer_name
            ),
            component: plane.name.to_string().into(),
        });
    }

    let z_start_nm = ctx.stackup_manager.resolve_elevation(
        &plane.elevation,
        ctx.symbol_table,
        ctx.eval_context,
    )?;

    Ok(ResolvedElevation {
        layer_name,
        z_start_nm,
        z_end_nm: z_start_nm + thickness_nm,
    })
}

/// Map the plane's elevation declaration to a semantic layer name.
fn resolve_layer_name(elevation: &Elevation) -> String {
    match elevation {
        Elevation::Semantic(id) => id.to_string(),
        _ => "top_copper".to_string(),
    }
}

/// Resolve the plane's physical thickness in nanometers.
fn resolve_thickness(
    plane: &PlanePlacement,
    layer_name: &str,
    ctx: &PlacementContext,
) -> Result<i64, IrError> {
    if let Some(t_expr) = &plane.thickness {
        return crate::ir::conversions::evaluate_expression_to_nm(
            t_expr,
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: format!("plane '{}' thickness", plane.name),
            reason: e.to_string(),
        });
    }

    let from_profile = ctx
        .profile
        .and_then(|p| p.get_layer_thickness(layer_name))
        .and_then(|t_expr| {
            crate::ir::conversions::evaluate_expression_to_nm(
                t_expr,
                ctx.symbol_table,
                ctx.eval_context,
            )
            .ok()
        });

    Ok(from_profile.unwrap_or_else(|| {
        ctx.stackup_manager
            .get_layer_thickness(layer_name)
            .unwrap_or(0)
    }))
}
