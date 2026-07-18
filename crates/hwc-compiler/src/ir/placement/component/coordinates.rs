use super::super::super::conversions::{
    evaluate_expression_to_nm, resolve_coordinate_z_nm, CoordinateContext,
};
use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use super::super::coordinate_evaluation::{evaluate_coordinate_with_anchors, CoordinateAxis};
use crate::bounding_box_tracker::BoundingBoxTracker;
use hwc_engine::{geometry::Point3D, HardwareSpace};
use hwc_parser::{Coordinate, EvaluationContext};

pub fn resolve_position(
    component: &hwc_parser::ComponentPlacement,
    bbox_tracker: &mut BoundingBoxTracker,
    eval_context: &EvaluationContext,
) -> Result<Coordinate, IrError> {
    let position =
        component
            .position
            .as_ref()
            .ok_or_else(|| IrError::CoordinateResolutionFailed {
                coordinate_str: "component position".into(),
                reason: "Component has no explicit position and unresolved relational constraints"
                    .into(),
            })?;
    if position.is_relative() {
        let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, eval_context);
        solver
            .resolve_position(position)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "relative position".into(),
                reason: e.to_string(),
            })
    } else {
        Ok(position.clone())
    }
}

pub fn calculate_untransformed_origin(
    resolved_position: &Coordinate,
    space: &HardwareSpace,
    ctx: &PlacementContext,
    bbox_tracker: &mut BoundingBoxTracker,
) -> Result<Point3D, IrError> {
    let (x_expr, y_expr, z_expr) = match resolved_position {
        Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
            (x, y, z)
        }
        Coordinate::Relative(_) => {
            return Err(IrError::PlacementConstraint {
                message: "Relative coordinates should be resolved before this point".into(),
                component: "component".into(),
            });
        }
    };

    let has_anchor_refs = x_expr.contains_anchor_reference()
        || y_expr.contains_anchor_reference()
        || z_expr.contains_anchor_reference();

    let x_nm = if let Ok(hwc_parser::Value::Percentage(pct)) = x_expr.evaluate(ctx.eval_context) {
        ((pct / 100.0) * space.dimensions.width_nm as f64) as i64
    } else if has_anchor_refs && x_expr.contains_anchor_reference() {
        evaluate_coordinate_with_anchors(
            x_expr,
            ctx.symbol_table,
            bbox_tracker,
            CoordinateAxis::X,
            ctx.origin.z,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "X coordinate with anchor references".into(),
            reason: e.to_string(),
        })?
    } else {
        evaluate_expression_to_nm(x_expr, ctx.symbol_table).map_err(|e| {
            IrError::CoordinateResolutionFailed {
                coordinate_str: "X coordinate".into(),
                reason: e.to_string(),
            }
        })?
    };

    let y_nm = if let Ok(hwc_parser::Value::Percentage(pct)) = y_expr.evaluate(ctx.eval_context) {
        ((pct / 100.0) * space.dimensions.height_nm as f64) as i64
    } else if has_anchor_refs && y_expr.contains_anchor_reference() {
        evaluate_coordinate_with_anchors(
            y_expr,
            ctx.symbol_table,
            bbox_tracker,
            CoordinateAxis::Y,
            ctx.origin.z,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: "Y coordinate with anchor references".into(),
            reason: e.to_string(),
        })?
    } else {
        evaluate_expression_to_nm(y_expr, ctx.symbol_table).map_err(|e| {
            IrError::CoordinateResolutionFailed {
                coordinate_str: "Y coordinate".into(),
                reason: e.to_string(),
            }
        })?
    };

    let z_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let z_nm = resolve_coordinate_z_nm(z_expr, &z_ctx, has_anchor_refs).map_err(|e| {
        IrError::CoordinateResolutionFailed {
            coordinate_str: "Z coordinate".into(),
            reason: e.to_string(),
        }
    })?;

    if z_nm < 0 {
        let z_span = z_expr.span();
        return Err(IrError::NegativeLayerIndex {
            value: z_nm,
            span: (z_span.start, z_span.end - z_span.start).into(),
        });
    }

    Ok(Point3D::new(x_nm, y_nm, z_nm))
}
