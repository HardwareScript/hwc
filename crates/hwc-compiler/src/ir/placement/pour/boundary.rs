//! Boundary geometry resolution for pour placement.

use super::super::super::conversions::spanning_coordinate_to_point;
use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::constraint_solver::ConstraintSolver;
use crate::ir::conversions::CoordinateContext;
use hwc_engine::{Dimensions, Point3D};
use hwc_parser::PourBoundary;

/// Resolved geometric parameters of a pour boundary.
pub struct ResolvedBoundary {
    pub start: Point3D,
    pub end: Point3D,
    pub area_nm2: i64,
    pub circle_radius_nm: Option<i64>,
}

/// Resolve a pour boundary into concrete start/end points and area.
///
/// Handles both `Rect` (from/to corners) and `Circle` (center + radius)
/// boundaries, resolving relative positions via the constraint solver.
pub fn resolve_boundary_coords(
    boundary: &PourBoundary,
    space_dimensions: &Dimensions,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<ResolvedBoundary, IrError> {
    let solver = ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    let coord_ctx = CoordinateContext {
        space_dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };

    let mut circle_radius_nm: Option<i64> = None;
    let (start, end, area_nm2) = match boundary {
        PourBoundary::Rect(from_raw, to_raw) => {
            let from = if from_raw.is_relative() {
                let intent = solver.resolve_position(from_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "pour from position".to_string(),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(from_raw, &coord_ctx, false).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "pour from".to_string(),
                        reason: e,
                    }
                })?
            };

            let to = if to_raw.is_relative() {
                let intent = solver.resolve_position(to_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "pour to position".to_string(),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(to_raw, &coord_ctx, true).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "pour to".to_string(),
                        reason: e,
                    }
                })?
            };

            let w = (to.x - from.x).abs();
            let h = (to.y - from.y).abs();
            (from, to, w * h)
        }
        PourBoundary::Circle {
            center: center_raw,
            radius,
        } => {
            let radius_nm = crate::ir::conversions::evaluate_expression_to_nm(
                radius,
                ctx.symbol_table,
                ctx.eval_context,
            )
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: "pour circle radius".to_string(),
                reason: e.to_string(),
            })?;
            circle_radius_nm = Some(radius_nm);

            let center_pt = if center_raw.is_relative() {
                let intent = solver.resolve_position(center_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "pour circle center".to_string(),
                        reason: e.to_string(),
                    }
                })?;
                intent.point()
            } else {
                spanning_coordinate_to_point(center_raw, &coord_ctx, false).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: "pour circle center".to_string(),
                        reason: e,
                    }
                })?
            };

            let radius_nm_f = radius_nm as f64;
            let s = Point3D::new(
                center_pt.x - radius_nm_f as i64,
                center_pt.y - radius_nm_f as i64,
                0,
            );
            let e = Point3D::new(
                center_pt.x + radius_nm_f as i64,
                center_pt.y + radius_nm_f as i64,
                0,
            );

            let w = (e.x - s.x).abs();
            let h = (e.y - s.y).abs();
            (s, e, w * h)
        }
    };

    Ok(ResolvedBoundary {
        start,
        end,
        area_nm2,
        circle_radius_nm,
    })
}
