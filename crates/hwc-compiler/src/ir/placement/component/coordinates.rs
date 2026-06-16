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
    if component.position.is_relative() {
        let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, eval_context);
        solver.resolve_position(&component.position).map_err(|e| {
            IrError::PlacementError(format!("Failed to resolve relative position: {}", e))
        })
    } else {
        Ok(component.position.clone())
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
            panic!("Relative coordinates should be resolved before this point");
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
        .expect("Failed to evaluate X coordinate with anchor references")
    } else {
        evaluate_expression_to_nm(x_expr, ctx.symbol_table)
            .expect("Failed to evaluate X coordinate")
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
        .expect("Failed to evaluate Y coordinate with anchor references")
    } else {
        evaluate_expression_to_nm(y_expr, ctx.symbol_table)
            .expect("Failed to evaluate Y coordinate")
    };

    let z_ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };
    let z_nm = resolve_coordinate_z_nm(z_expr, &z_ctx, has_anchor_refs)
        .map_err(IrError::PlacementError)?;

    if z_nm < 0 {
        let z_span = z_expr.span();
        return Err(IrError::NegativeLayerIndex {
            value: z_nm,
            span: (z_span.start, z_span.end - z_span.start).into(),
        });
    }

    Ok(Point3D::new(x_nm, y_nm, z_nm))
}

pub fn transform_declarative_to_relative(
    coord: &hwc_parser::Coordinate,
    pin_name: &str,
) -> hwc_parser::Coordinate {
    match coord {
        hwc_parser::Coordinate::Declarative { x, y, z, span } => {
            hwc_parser::Coordinate::Relative(hwc_parser::RelativePosition {
                anchor: hwc_parser::AnchorReference {
                    name: pin_name.into(),
                    span: *span,
                },
                edge: hwc_parser::Edge::Left,
                offset: hwc_parser::RelativeOffset::Vector {
                    x: x.clone(),
                    y: y.clone(),
                    z: z.clone(),
                },
                span: *span,
            })
        }
        _ => coord.clone(),
    }
}
