//! Geometry resolution for plane placement.
//!
//! Resolves a plane's XY extents from either a parameterized shape instance
//! (v0.1.9 middle-level syntax) or explicit `from:`/`to:` corner coordinates.
//!
//! # Positioning semantics (v0.2.1)
//!
//! `at:` positioning is explicit and predictable. The old `matches_center_edge()`
//! heuristic was removed:
//!
//! - **Explicit positioning (`at:`)**: `at: [x, y]` places the shape's
//!   origin-aligned corner at `(x, y)`. Which corner depends on the space's
//!   `origin:` declaration (`origin: bl by b` -> bottom-left corner, `tl by t`
//!   -> top-left corner, etc.). Coordinates are evaluated using anchor
//!   arithmetic, but the semantic is ALWAYS "place the origin corner here" with
//!   no implicit centering adjustments.
//! - **Relational centering (`align:`)**: `align: center_x with expr` explicitly
//!   calculates the centering offset. This is ergonomic sugar that compiles to
//!   explicit corner positioning; see [`apply_center_alignment`].
//!
//! This removes the implicit magic where the compiler tried to detect center
//! references in `at:` expressions and auto-adjust positioning. That violated
//! the principle of least surprise and broke with complex expressions like
//! `(A.center_x + B.center_x) / 2`.

use super::super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::super::errors::IrError;
use super::super::context::PlacementContext;
use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::constraint_solver::ConstraintSolver;
use hwc_engine::{Dimensions, Point3D};
use hwc_parser::{AlignmentAxis, Coordinate, PlanePlacement, RelationalConstraint};

/// Resolved XY geometry of a plane, prior to Z application.
pub struct ResolvedGeometry {
    /// Origin-corner point of the plane (Z not yet applied).
    pub start: Point3D,
    /// Opposite corner point of the plane (Z not yet applied).
    pub end: Point3D,
    /// Planar area in square nanometers.
    pub area_nm2: i64,
}

/// Resolve the plane's XY geometry.
///
/// Dispatches to the shape-based path when `plane.shape` is present, otherwise
/// falls back to the legacy explicit `from:`/`to:` corner path.
pub fn resolve_plane_geometry(
    plane: &PlanePlacement,
    space_dimensions: &Dimensions,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<ResolvedGeometry, IrError> {
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

    // v0.1.9: Handle shape-based planes with parameterized geometry
    if let Some(shape_inst) = &plane.shape {
        resolve_shape_geometry(plane, shape_inst, &coord_ctx, &solver, ctx)
    } else {
        resolve_corner_geometry(plane, &coord_ctx, &solver)
    }
}

/// Resolve geometry for a shape-based plane: an origin corner plus the shape's
/// evaluated width/height.
fn resolve_shape_geometry(
    plane: &PlanePlacement,
    shape_inst: &hwc_parser::ShapeInstance,
    coord_ctx: &CoordinateContext<'_>,
    solver: &ConstraintSolver<'_>,
    ctx: &PlacementContext,
) -> Result<ResolvedGeometry, IrError> {
    // Resolve shape parameters to get dimensions
    let (width_nm, height_nm) =
        super::shape::resolve_shape_dimensions(shape_inst, ctx.symbol_table, ctx.eval_context)?;

    // Get position from `at:` field or resolved relational constraints
    let Some(from_coord) = &plane.from else {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Plane '{}' with shape requires 'at:' coordinate for positioning",
                plane.name
            ),
            component: plane.name.to_string().into(),
        });
    };

    let mut position = resolve_point(
        from_coord,
        coord_ctx,
        solver,
        false,
        &format!("plane '{}' position", plane.name),
    )?;

    apply_center_alignment(
        &plane.relational_constraints,
        &mut position,
        width_nm,
        height_nm,
    );

    let end_pt = Point3D::new(position.x + width_nm, position.y + height_nm, position.z);

    Ok(ResolvedGeometry {
        start: position,
        end: end_pt,
        area_nm2: width_nm * height_nm,
    })
}

/// Resolve geometry from explicit `from:`/`to:` corner coordinates (legacy path).
fn resolve_corner_geometry(
    plane: &PlanePlacement,
    coord_ctx: &CoordinateContext<'_>,
    solver: &ConstraintSolver<'_>,
) -> Result<ResolvedGeometry, IrError> {
    let (Some(from_raw), Some(to_raw)) = (&plane.from, &plane.to) else {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Plane '{}' requires 'from' and 'to' coordinates",
                plane.name
            ),
            component: plane.name.to_string().into(),
        });
    };

    let from = resolve_point(
        from_raw,
        coord_ctx,
        solver,
        false,
        &format!("plane '{}' from", plane.name),
    )?;
    let to = resolve_point(
        to_raw,
        coord_ctx,
        solver,
        true,
        &format!("plane '{}' to", plane.name),
    )?;

    let w = (to.x - from.x).abs();
    let h = (to.y - from.y).abs();

    Ok(ResolvedGeometry {
        start: from,
        end: to,
        area_nm2: w * h,
    })
}

/// Apply explicit centering adjustments for `align: center*` constraints.
///
/// When using `align: center with <target>`, the relational resolver returns the
/// center X and/or Y coordinates. Half the width/height is subtracted here to
/// convert that center back into the corner position the rest of the pipeline
/// expects.
///
/// This is EXPLICIT centering behavior - there is no implicit magic based on the
/// content of the coordinate expression.
pub fn apply_center_alignment(
    constraints: &[RelationalConstraint],
    position: &mut Point3D,
    width_nm: i64,
    height_nm: i64,
) {
    for constraint in constraints {
        let RelationalConstraint::Align { axis, .. } = constraint else {
            continue;
        };

        match axis {
            AlignmentAxis::Center => {
                position.x -= width_nm / 2;
                position.y -= height_nm / 2;
            }
            AlignmentAxis::X => position.x -= width_nm / 2,
            AlignmentAxis::Y => position.y -= height_nm / 2,
            // Z-centering adjustment would go here if needed.
            AlignmentAxis::Z => {}
            // Edge alignments (top, bottom, left, right) need no adjustment.
            _ => {}
        }
    }
}

/// Resolve a coordinate to a concrete point, routing relative coordinates
/// through the constraint solver and absolute ones through the spanning
/// coordinate converter.
pub fn resolve_point(
    coord: &Coordinate,
    coord_ctx: &CoordinateContext<'_>,
    solver: &ConstraintSolver<'_>,
    is_span_end: bool,
    label: &str,
) -> Result<Point3D, IrError> {
    if coord.is_relative() {
        let intent =
            solver
                .resolve_position(coord)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: label.to_string(),
                    reason: e.to_string(),
                })?;
        Ok(intent.point())
    } else {
        spanning_coordinate_to_point(coord, coord_ctx, is_span_end).map_err(|e| {
            IrError::CoordinateResolutionFailed {
                coordinate_str: label.to_string(),
                reason: e,
            }
        })
    }
}
