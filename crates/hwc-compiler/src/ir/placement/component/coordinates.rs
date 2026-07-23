use super::super::super::conversions::evaluate_expression_to_nm;
use super::super::super::errors::IrError;
use super::super::intent::PlacementIntent;
use crate::bounding_box_tracker::BoundingBoxTracker;

pub fn resolve_position(
    component: &hwc_parser::ComponentPlacement,
    bbox_tracker: &mut BoundingBoxTracker,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<PlacementIntent, IrError> {
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
        // Absolute coordinate -> Corner intent (assembly tier)
        let point = absolute_coordinate_to_point(position)?;
        Ok(PlacementIntent::Corner(point))
    }
}

/// Convert an absolute coordinate (Positional/Declarative) to a Point3D.
fn absolute_coordinate_to_point(coord: &hwc_parser::Coordinate) -> Result<hwc_engine::geometry::Point3D, IrError> {
    let (x_expr, y_expr, z_expr) = match coord {
        hwc_parser::Coordinate::Positional { x, y, z, .. }
        | hwc_parser::Coordinate::Declarative { x, y, z, .. } => (x, y, z),
        hwc_parser::Coordinate::Relative(_) => {
            return Err(IrError::PlacementConstraint {
                message: "Relative coordinates should be resolved before this point".into(),
                component: "component".into(),
            });
        }
    };

    // Use a minimal eval context for absolute coordinates (no anchor references expected)
    let empty_ctx = hwc_parser::EvaluationContext::default();
    let x_nm = evaluate_expression_to_nm(x_expr, &crate::SymbolTable::default(), &empty_ctx).map_err(|e| {
        IrError::CoordinateResolutionFailed {
            coordinate_str: "X coordinate".into(),
            reason: e,
        }
    })?;
    let y_nm = evaluate_expression_to_nm(y_expr, &crate::SymbolTable::default(), &empty_ctx).map_err(|e| {
        IrError::CoordinateResolutionFailed {
            coordinate_str: "Y coordinate".into(),
            reason: e,
        }
    })?;
    let z_nm = evaluate_expression_to_nm(z_expr, &crate::SymbolTable::default(), &empty_ctx).map_err(|e| {
        IrError::CoordinateResolutionFailed {
            coordinate_str: "Z coordinate".into(),
            reason: e,
        }
    })?;
    Ok(hwc_engine::geometry::Point3D::new(x_nm, y_nm, z_nm))
}
