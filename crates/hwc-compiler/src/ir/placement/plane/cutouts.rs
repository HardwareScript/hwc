//! Cutout (void) resolution and carving for plane placement.
//!
//! Cutouts are resolved to concrete points/dimensions *before* the plane is
//! registered (while the `ConstraintSolver` borrow of the bbox tracker is still
//! alive), then applied to the entity graph afterwards.

use super::super::super::conversions::CoordinateContext;
use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use super::geometry::resolve_point;
use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::constraint_solver::ConstraintSolver;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::HardwareSpace;
use hwc_engine::{Dimensions, NetId, Point3D};
use hwc_parser::{CutoutShape, Expression, PlanePlacement};

/// A plane cutout with all of its coordinates and dimensions resolved to
/// nanometers.
pub struct ResolvedCutout {
    /// Rectangle corner, or circle center.
    pub at_pt: Point3D,
    /// Rectangle width (mutually exclusive with `radius_nm`).
    pub width_nm: Option<i64>,
    /// Rectangle height (mutually exclusive with `radius_nm`).
    pub height_nm: Option<i64>,
    /// Circle radius (mutually exclusive with `width_nm`/`height_nm`).
    pub radius_nm: Option<i64>,
}

/// Resolve every cutout declared on the plane into concrete geometry.
pub fn resolve_cutouts(
    plane: &PlanePlacement,
    space_dimensions: &Dimensions,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<Vec<ResolvedCutout>, IrError> {
    if plane.cutouts.is_empty() {
        return Ok(Vec::new());
    }

    let coord_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };

    let solver = ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    let mut resolved = Vec::with_capacity(plane.cutouts.len());
    for cutout in &plane.cutouts {
        let (at_raw, w_expr, h_expr, r_expr) = match cutout {
            CutoutShape::Rectangle { width, height, at } => (at, Some(width), Some(height), None),
            CutoutShape::Circle { radius, at } => (at, None, None, Some(radius)),
        };

        let at_pt = resolve_point(
            at_raw,
            &coord_ctx,
            &solver,
            false,
            &format!("cutout position for plane '{}'", plane.name),
        )?;

        let width_nm = eval_dimension(w_expr, "width", plane, ctx)?;
        let height_nm = eval_dimension(h_expr, "height", plane, ctx)?;
        let radius_nm = eval_dimension(r_expr, "radius", plane, ctx)?;

        resolved.push(ResolvedCutout {
            at_pt,
            width_nm,
            height_nm,
            radius_nm,
        });
    }

    Ok(resolved)
}

/// Evaluate an optional cutout dimension expression to nanometers.
fn eval_dimension(
    expr: Option<&Expression>,
    what: &str,
    plane: &PlanePlacement,
    ctx: &PlacementContext,
) -> Result<Option<i64>, IrError> {
    expr.map(|e| {
        crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table, ctx.eval_context)
            .map_err(|err| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("cutout {} for plane '{}'", what, plane.name),
                reason: err.to_string(),
            })
    })
    .transpose()
}

/// Carve the resolved cutouts out of the placed plane.
///
/// Rectangular cutouts drill a hole in the entity graph; circular cutouts are
/// registered as a circular substrate layer so the router models them with the
/// correct curvature.
pub fn apply_cutouts(
    space: &mut HardwareSpace,
    cutouts: Vec<ResolvedCutout>,
    z_start_nm: i64,
    z_end_nm: i64,
) {
    for rc in cutouts {
        if let (Some(w), Some(h)) = (rc.width_nm, rc.height_nm) {
            let cutout_bbox = BoundingBox::new(
                Point3D::new(rc.at_pt.x, rc.at_pt.y, z_start_nm),
                Point3D::new(rc.at_pt.x + w, rc.at_pt.y + h, z_end_nm),
            );
            space
                .entity_graph
                .drill_hole(cutout_bbox, None, NetId::UNCONNECTED);
        } else if let Some(r) = rc.radius_nm {
            let cutout_bbox = BoundingBox::new(
                Point3D::new(rc.at_pt.x - r, rc.at_pt.y - r, z_start_nm),
                Point3D::new(rc.at_pt.x + r, rc.at_pt.y + r, z_end_nm),
            );
            space
                .entity_graph
                .add_circle_substrate_layer(0, NetId::UNCONNECTED, cutout_bbox, r);
        }
    }
}
