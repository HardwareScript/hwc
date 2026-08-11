//! Pour placement orchestration.
//!
//! `place_pour` is the public entry point. It validates the pour, resolves its
//! boundary (recursing when a position + dimensions must first be expanded into
//! an explicit boundary), then delegates to focused submodules for boundary
//! geometry resolution, interface/connection registration, collision checks,
//! and netlist/metadata registration.

mod boundary;
mod collision;
mod connection;
mod interface;
mod netlist;

use boundary::resolve_boundary_coords;
use collision::check_pour_collisions;
use connection::register_pour_surface;
use interface::register_pour_interface;
use netlist::{register_pour_netlist, resolve_pour_net};

pub use boundary::ResolvedBoundary;

use super::super::conversions::CoordinateContext;
use super::super::errors::IrError;
use super::context::PlacementContext;
use crate::bounding_box_tracker::BoundingBoxTracker;
use crate::constraint_solver::ConstraintSolver;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::geometry_router::entity_graph::SubstrateLayerConfig;
use hwc_engine::space::HardwareSpace;
use hwc_engine::Point3D;
use hwc_parser::PourPlacement;

pub fn place_pour(
    space: &mut HardwareSpace,
    pour: &PourPlacement,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&pour.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: pour.material.clone(),
        })?;

    // v0.2.2: EXTERNAL AUDIT FIX - Validate multi-terminal device bodies don't have net assignments
    // A resistor body connecting terminals A and B cannot belong to a single net without creating
    // a logical short circuit. Device bodies spanning multiple terminals must only use device bindings.
    if let Some(ref device_binding) = pour.device {
        if device_binding.terminals.len() > 1 && pour.net.is_some() {
            return Err(IrError::DeviceNetConflict {
                pour_name: pour.name.to_string().into(),
                device_name: device_binding.device_name.to_string().into(),
                terminals: device_binding
                    .terminals
                    .iter()
                    .map(|t| format!("{}.{}", device_binding.device_name, t))
                    .collect(),
                assigned_net: pour.net.as_ref().unwrap().to_string().into(),
            });
        }
    }

    // v0.2.1: Boundary is optional if relational constraints OR (position + dimensions) are provided
    // The relational resolver/compiler will compute the boundary from constraints or position + dimensions
    let has_dimensions = pour.width.is_some() && pour.height.is_some();
    if pour.boundary.is_none()
        && pour.relational_constraints.is_empty()
        && !(pour.position.is_some() && has_dimensions)
    {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Pour '{}' missing boundary (provide either 'boundary:', 'at:' + 'dimensions:', or relational constraints like 'align:', 'right_of:', etc.)",
                pour.name
            ),
            component: pour.name.to_string().into(),
        });
    }

    // If there are relational constraints or (position + dimensions) but no boundary yet, resolve them first
    if pour.boundary.is_none() {
        // Will be resolved in this function or relational_resolver pass
        if pour.relational_constraints.is_empty() {
            // Must be position + dimensions case - resolve it now
            if let (Some(pos), Some(w), Some(h)) = (&pour.position, &pour.width, &pour.height) {
                let center = if pos.is_relative() {
                    let solver = ConstraintSolver::new(bbox_tracker, ctx.eval_context);
                    let intent = solver.resolve_position(pos).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("pour '{}' position", pour.name),
                            reason: e.to_string(),
                        }
                    })?;
                    intent.point()
                } else {
                    let coord_ctx = CoordinateContext {
                        space_dimensions: &space.dimensions,
                        symbol_table: ctx.symbol_table,
                        eval_context: ctx.eval_context,
                        bbox_tracker: Some(bbox_tracker),
                        stackup_manager: ctx.stackup_manager,
                        profile: ctx.profile,
                    };
                    crate::ir::conversions::coordinate_to_point(pos, &coord_ctx).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("pour '{}' position", pour.name),
                            reason: e,
                        }
                    })?
                };

                // Evaluate dimensions
                let width_nm = crate::ir::conversions::evaluate_expression_to_nm(
                    w,
                    ctx.symbol_table,
                    ctx.eval_context,
                )
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("pour '{}' width", pour.name),
                    reason: e,
                })?;
                let height_nm = crate::ir::conversions::evaluate_expression_to_nm(
                    h,
                    ctx.symbol_table,
                    ctx.eval_context,
                )
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("pour '{}' height", pour.name),
                    reason: e,
                })?;

                // Create boundary from center + dimensions
                let from =
                    Point3D::new(center.x - width_nm / 2, center.y - height_nm / 2, center.z);
                let to = Point3D::new(center.x + width_nm / 2, center.y + height_nm / 2, center.z);

                let span_empty = hwc_parser::Span::new(0, 0);

                // Create a mutable pour to place
                let mut resolved_pour = pour.clone();
                resolved_pour.boundary = Some(hwc_parser::PourBoundary::Rect(
                    Box::new(hwc_parser::Coordinate::Positional {
                        x: hwc_parser::Expression::Measurement {
                            value: from.x as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        y: hwc_parser::Expression::Measurement {
                            value: from.y as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        z: hwc_parser::Expression::Measurement {
                            value: from.z as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        span: span_empty,
                    }),
                    Box::new(hwc_parser::Coordinate::Positional {
                        x: hwc_parser::Expression::Measurement {
                            value: to.x as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        y: hwc_parser::Expression::Measurement {
                            value: to.y as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        z: hwc_parser::Expression::Measurement {
                            value: to.z as f64,
                            unit: hwc_parser::Unit::Nanometer,
                            span: span_empty,
                        },
                        span: span_empty,
                    }),
                ));

                // Continue with placement using resolved boundary
                return place_pour(space, &resolved_pour, bbox_tracker, ctx);
            }
        } else {
            // Relational constraints present - will be resolved in relational_resolver pass
            return Ok(());
        }
    }

    let boundary = pour.boundary.as_ref().unwrap();

    let layer_name = match &pour.elevation {
        hwc_parser::Elevation::Semantic(id) => id.to_string(),
        _ => "top_copper".to_string(),
    };

    // Resolve thickness with proper fail-fast validation (v0.2.1)
    // 1. Explicit thickness on pour
    // 2. Profile layer thickness expression
    // 3. StackupManager layer thickness (includes 0nm for masks)
    // NO FALLBACKS - must be explicitly defined somewhere
    let thickness_nm = if let Some(t_expr) = &pour.thickness {
        crate::ir::conversions::evaluate_expression_to_nm(
            t_expr,
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: format!("pour '{}' thickness", pour.name),
            reason: e.to_string(),
        })?
    } else if let Some(t_expr) = ctx.profile.and_then(|p| p.get_layer_thickness(&layer_name)) {
        crate::ir::conversions::evaluate_expression_to_nm(
            t_expr,
            ctx.symbol_table,
            ctx.eval_context,
        )
        .map_err(|e| IrError::CoordinateResolutionFailed {
            coordinate_str: format!("profile layer '{}' thickness", layer_name),
            reason: e.to_string(),
        })?
    } else if let Some(thickness) = ctx.stackup_manager.get_layer_thickness(&layer_name) {
        // StackupManager returns Some(0) for zero-thickness masks
        // This is valid and intentional, not a fallback
        thickness
    } else {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Could not resolve thickness for pour '{}' on layer '{}'. \
                 Layer not found in stackup. Ensure the layer is defined in profile.stackup.",
                pour.name, layer_name
            ),
            component: pour.name.to_string().into(),
        });
    };

    // No special handling needed - 0nm is valid for mask materials

    let z_start_nm = ctx.stackup_manager.resolve_elevation(
        &pour.elevation,
        ctx.symbol_table,
        ctx.eval_context,
    )?;
    let z_end_nm = z_start_nm + thickness_nm;

    let ResolvedBoundary {
        start,
        end,
        area_nm2,
        circle_radius_nm,
    } = resolve_boundary_coords(boundary, &space.dimensions, bbox_tracker, ctx)?;

    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);

    let bbox = BoundingBox::new(start_with_z, end_with_z);

    bbox_tracker.register(pour.name.to_string(), bbox, start_with_z);

    // v0.1.8: Register pour in EntityGraph for O(1) resolution
    let net_id = if let Some(net_name) = &pour.net {
        let _min_width_nm = space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK missing required 'trace.min_width_nm' constraint".into(),
                hint: "Add a 'trace:' block to your profile with explicit min_width.\n\nExample:\n  trace:\n    min_width: 180nm".into(),
            })?;
        Some(space.netlist.get_or_create_net(&net_name.base))
    } else {
        None
    };

    space
        .entity_graph
        .register_space_entity(&pour.name.base, bbox, net_id, z_start_nm);

    // Register PhysicalInterface for routing connectivity
    register_pour_interface(space, pour.name.base.as_str(), bbox, ctx)?;

    // v0.2.0: Register pour surface in layer connection database
    register_pour_surface(space, pour, &layer_name, bbox);

    check_pour_collisions(space, pour, bbox, z_start_nm, ctx.collector)?;

    let resolved_net_name = resolve_pour_net(space, pour);

    // Update the area on the just-pushed... (metadata pushed inside register_pour_netlist)
    let net_id_raw = register_pour_netlist(
        space,
        pour,
        resolved_net_name,
        z_start_nm,
        start_with_z,
        end_with_z,
        material_id,
    );

    // Patch area into the pour metadata we just pushed
    if let Some(last) = space.pours.last_mut() {
        last.area_nm2 = area_nm2;
    }

    let bbox = BoundingBox::new(start_with_z, end_with_z);

    // Get min_spacing from profile for early clearance validation (v0.1.9)
    // NO DEFAULTS - require explicit profile declaration
    let min_clearance_nm = space
        .fabrication_constraints
        .as_ref()
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Cannot validate pour clearance without fabrication constraints".into(),
            hint: "Add a profile with 'trace: min_spacing: <value>' to enable early DRC validation"
                .into(),
        })?
        .trace
        .min_spacing_nm;

    if let Some(radius) = circle_radius_nm {
        space.entity_graph.add_circle_substrate_layer(
            material_id,
            hwc_engine::NetId::new(net_id_raw),
            bbox,
            radius,
        );
    } else {
        // Use checked version to catch clearance violations early (v0.1.9)
        // v0.2.1: Pass device binding for same-device terminal exemption (capacitors, etc.)
        // v0.2.2: For multi-terminal bindings, use first terminal for device binding reference
        let device_binding_ref = pour.device.as_ref().and_then(|b| {
            b.terminals.first().map(|first_term| (&b.device_name, first_term))
        });

        if let Err(msg) = space
            .entity_graph
            .add_substrate_layer_checked(SubstrateLayerConfig {
                material: material_id,
                net: hwc_engine::NetId::new(net_id_raw),
                bbox,
                layer_type: hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour,
                min_clearance_nm,
                device_binding: device_binding_ref,
                pours: &space.pours,
            })
        {
            return Err(IrError::ClearanceViolation {
                entity_type: "pour".into(),
                entity_name: pour.name.to_string(),
                reason: format!(
                    "{}\nRequired spacing: {}nm (from profile trace.min_spacing)\n\
                     Adjust the pour boundary to maintain clearance from other nets.",
                    msg, min_clearance_nm
                )
                .into(),
            });
        }
    }

    Ok(())
}
