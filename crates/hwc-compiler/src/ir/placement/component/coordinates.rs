use crate::SymbolTable;
use hwc_parser::{Coordinate, EvaluationContext};
use hwc_engine::{HardwareSpace, geometry::Point3D};
use super::super::super::conversions::{evaluate_expression_to_nm, resolve_coordinate_z_nm, CoordinateContext};
use super::super::super::errors::IrError;
use super::super::coordinate_evaluation::{evaluate_coordinate_with_anchors, CoordinateAxis};
use crate::bounding_box_tracker::BoundingBoxTracker;
use super::super::super::stackup_manager::StackupManager;

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
    symbol_table: &SymbolTable,
    bbox_tracker: &mut BoundingBoxTracker,
    eval_context: &EvaluationContext,
    stackup_manager: &StackupManager,
    origin: hwc_parser::OriginPoint,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<Point3D, IrError> {
    let (x_expr, y_expr, z_expr) = match resolved_position {
        Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
            (x, y, z)
        }
        Coordinate::Relative(_) => {
            panic!("Relative coordinates should be resolved before this point");
        }
    };

    // Check if expressions contain anchor references
    let has_anchor_refs = x_expr.contains_anchor_reference()
        || y_expr.contains_anchor_reference()
        || z_expr.contains_anchor_reference();

    // Evaluate to nanometers without origin transformation
    let x_nm = if let Ok(hwc_parser::Value::Percentage(pct)) = x_expr.evaluate(eval_context) {
        ((pct / 100.0) * space.dimensions.width_nm as f64) as i64
    } else if has_anchor_refs && x_expr.contains_anchor_reference() {
        evaluate_coordinate_with_anchors(
            x_expr,
            symbol_table,
            bbox_tracker,
            CoordinateAxis::X,
            origin.z,
        )
        .expect("Failed to evaluate X coordinate with anchor references")
    } else {
        evaluate_expression_to_nm(x_expr, symbol_table)
            .expect("Failed to evaluate X coordinate")
    };

    let y_nm = if let Ok(hwc_parser::Value::Percentage(pct)) = y_expr.evaluate(eval_context) {
        ((pct / 100.0) * space.dimensions.height_nm as f64) as i64
    } else if has_anchor_refs && y_expr.contains_anchor_reference() {
        evaluate_coordinate_with_anchors(
            y_expr,
            symbol_table,
            bbox_tracker,
            CoordinateAxis::Y,
            origin.z,
        )
        .expect("Failed to evaluate Y coordinate with anchor references")
    } else {
        evaluate_expression_to_nm(y_expr, symbol_table)
            .expect("Failed to evaluate Y coordinate")
    };

    let z_ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager,
        profile,
    };
    let z_nm = resolve_coordinate_z_nm(z_expr, &z_ctx, has_anchor_refs)
        .map_err(IrError::PlacementError)?;

    // Sprint 5.5: Validate Z coordinate bounds before creating point
    if z_nm < 0 {
        let z_span = z_expr.span();
        return Err(IrError::NegativeLayerIndex {
            value: z_nm,
            span: (z_span.start, z_span.end - z_span.start).into(),
        });
    }

    Ok(Point3D::new(x_nm, y_nm, z_nm))
}

pub fn transform_declarative_to_relative(coord: &hwc_parser::Coordinate, pin_name: &str) -> hwc_parser::Coordinate {
    match coord {
        hwc_parser::Coordinate::Declarative { x, y, z, span } => {
            hwc_parser::Coordinate::Relative(hwc_parser::RelativePosition {
                anchor: hwc_parser::AnchorReference {
                    name: pin_name.into(),
                    span: *span,
                },
                edge: hwc_parser::Edge::Left, // Point anchors resolve same for all edges
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
