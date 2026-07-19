use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use hwc_engine::space::PourMetadata;
use hwc_engine::{HardwareSpace, Point3D};

struct ResolvedCutout {
    at_pt: Point3D,
    width_nm: Option<i64>,
    height_nm: Option<i64>,
    radius_nm: Option<i64>,
}

/// Resolve shape dimensions from a shape instance (v0.1.9 middle-level syntax)
///
/// This function extracts width and height from the shape instance parameters.
/// The shape instance contains the actual parameter values passed when instantiating the shape.
fn resolve_shape_dimensions(
    shape_inst: &hwc_parser::ShapeInstance,
    symbol_table: &crate::SymbolTable,
) -> Result<(i64, i64), IrError> {
    // Look up the shape definition to verify it exists
    let _shape_def = symbol_table
        .get_shape(&shape_inst.shape_name)
        .ok_or_else(|| IrError::UndeclaredShape {
            shape: shape_inst.shape_name.clone(),
        })?;

    eprintln!(
        "[SHAPE DEBUG] Resolving dimensions for shape: {}",
        shape_inst.shape_name
    );
    eprintln!(
        "[SHAPE DEBUG] Parameters count: {}",
        shape_inst.parameters.len()
    );

    // Extract width and height from the shape instance parameters
    // Example: Pad(w: 600nm, h: 600nm) becomes parameters with keyword arguments
    let mut width_nm = None;
    let mut height_nm = None;

    for param in &shape_inst.parameters {
        let hwc_parser::Parameter::Keyword { name, value } = param;
        eprintln!("[SHAPE DEBUG] Processing parameter: {} = {:?}", name, value);

        let value_nm = match value {
            hwc_parser::ParameterValue::Measurement(m) => {
                let pm = m
                    .to_picometers_i64()
                    .ok_or_else(|| IrError::ShapeResolutionFailed {
                        shape: shape_inst.shape_name.clone(),
                        reason: format!("Parameter '{}' has non-distance unit", name),
                    })?;
                let nm = pm / 1000; // Convert picometers to nanometers
                eprintln!("[SHAPE DEBUG] Converted {}pm to {}nm", pm, nm);
                nm
            }
            _ => {
                return Err(IrError::ShapeResolutionFailed {
                    shape: shape_inst.shape_name.clone(),
                    reason: format!("Parameter '{}' must be a Measurement", name),
                });
            }
        };

        // Map parameter names to width/height
        // Common conventions: w/width for width, h/height for height
        match name.as_str() {
            "w" | "width" => {
                eprintln!("[SHAPE DEBUG] Setting width = {}nm", value_nm);
                width_nm = Some(value_nm);
            }
            "h" | "height" => {
                eprintln!("[SHAPE DEBUG] Setting height = {}nm", value_nm);
                height_nm = Some(value_nm);
            }
            _ => {
                eprintln!("[SHAPE DEBUG] Ignoring unknown parameter: {}", name);
            }
        }
    }

    eprintln!(
        "[SHAPE DEBUG] Final: width={:?}, height={:?}",
        width_nm, height_nm
    );

    match (width_nm, height_nm) {
        (Some(w), Some(h)) => {
            eprintln!("[SHAPE DEBUG] Returning dimensions: {}nm x {}nm", w, h);
            Ok((w, h))
        }
        _ => Err(IrError::ShapeResolutionFailed {
            shape: shape_inst.shape_name.clone(),
            reason: "Shape instance must provide 'w'/'width' and 'h'/'height' parameters for plane geometry"
                .into(),
        }),
    }
}

pub fn place_plane(
    space: &mut HardwareSpace,
    plane: &hwc_parser::PlanePlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&plane.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: plane.material.clone(),
        })?;

    let layer_name = match &plane.elevation {
        hwc_parser::Elevation::Semantic(id) => id.to_string(),
        _ => "top_copper".to_string(),
    };

    let thickness_nm = if let Some(t_expr) = &plane.thickness {
        crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table).map_err(
            |e| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("plane '{}' thickness", plane.name),
                reason: e.to_string(),
            },
        )?
    } else {
        ctx.profile
            .and_then(|p| p.get_layer_thickness(&layer_name))
            .and_then(|t_expr| {
                crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table).ok()
            })
            .unwrap_or_else(|| {
                ctx.stackup_manager
                    .get_layer_thickness(&layer_name)
                    .unwrap_or(0)
            })
    };

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

    let z_start_nm = ctx
        .stackup_manager
        .resolve_elevation(&plane.elevation, ctx.symbol_table)?;
    let z_end_nm = z_start_nm + thickness_nm;

    let coord_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: None,
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };

    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    // v0.1.9: Handle shape-based planes with parameterized geometry
    let (start, end, area_nm2) = if let Some(shape_inst) = &plane.shape {
        // Resolve shape parameters to get dimensions
        let (width_nm, height_nm) = resolve_shape_dimensions(shape_inst, ctx.symbol_table)?;

        // Get position from `at:` field or relational constraints
        let mut position = if let Some(from_coord) = &plane.from {
            let from = if from_coord.is_relative() {
                solver.resolve_position(from_coord).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("plane '{}' position", plane.name),
                        reason: e.to_string(),
                    }
                })?
            } else {
                from_coord.clone()
            };
            spanning_coordinate_to_point(&from, &coord_ctx, false).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("plane '{}' position", plane.name),
                    reason: e,
                }
            })?
        } else {
            return Err(IrError::PlacementConstraint {
                message: format!(
                    "Plane '{}' with shape requires 'at:' coordinate for positioning",
                    plane.name
                ),
                component: plane.name.to_string().into(),
            });
        };

        // v0.1.9: Adjust for center alignment [Spatial_Synthesis_Abstraction.md §1.4.1]
        //
        // CENTER ALIGNMENT SEMANTICS: The relational resolver returns the CENTER coordinate
        // of the target object when `align: center_x/center_y/center_z` is used. To achieve
        // true center-to-center alignment, we must:
        //   1. Take the target's center coordinate (from resolver)
        //   2. Subtract half of THIS object's dimensions
        //   3. Result: bottom-left corner position that centers this object
        //
        // Example: Pad_A center_y = 300µm, Pad_B height = 200µm
        //   - Resolver returns: Y = 300µm (Pad_A's center)
        //   - We adjust: Y = 300µm - (200µm / 2) = 200µm (Pad_B's bottom-left)
        //   - Result: Pad_B spans Y:200-400µm, center at 300µm ✓ ALIGNED
        for constraint in &plane.relational_constraints {
            if let hwc_parser::RelationalConstraint::Align { axis, .. } = constraint {
                match axis {
                    hwc_parser::AlignmentAxis::CenterX => {
                        position.x -= width_nm / 2;
                    }
                    hwc_parser::AlignmentAxis::CenterY => {
                        position.y -= height_nm / 2;
                    }
                    hwc_parser::AlignmentAxis::CenterZ => {
                        // Z-centering doesn't affect XY position
                    }
                    _ => {}
                }
            }
        }

        let end_pt = Point3D::new(position.x + width_nm, position.y + height_nm, position.z);
        let area = width_nm * height_nm;
        (position, end_pt, area)
    } else {
        // Legacy behavior: use explicit from/to coordinates
        match (&plane.from, &plane.to) {
            (Some(from_raw), Some(to_raw)) => {
                let from = if from_raw.is_relative() {
                    solver.resolve_position(from_raw).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("plane '{}' from position", plane.name),
                            reason: e.to_string(),
                        }
                    })?
                } else {
                    from_raw.clone()
                };

                let to = if to_raw.is_relative() {
                    solver.resolve_position(to_raw).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("plane '{}' to position", plane.name),
                            reason: e.to_string(),
                        }
                    })?
                } else {
                    to_raw.clone()
                };

                let s = spanning_coordinate_to_point(&from, &coord_ctx, false).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("plane '{}' from", plane.name),
                        reason: e,
                    }
                })?;
                let e = spanning_coordinate_to_point(&to, &coord_ctx, true).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("plane '{}' to", plane.name),
                        reason: e,
                    }
                })?;

                let w = (e.x - s.x).abs();
                let h = (e.y - s.y).abs();
                (s, e, w * h)
            }
            _ => {
                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Plane '{}' requires 'from' and 'to' coordinates",
                        plane.name
                    ),
                    component: plane.name.to_string().into(),
                });
            }
        }
    };

    let mut resolved_cutouts = Vec::new();
    for cutout in &plane.cutouts {
        let (at_raw, w_expr, h_expr, r_expr) = match cutout {
            hwc_parser::CutoutShape::Rectangle { width, height, at } => {
                (at, Some(width), Some(height), None)
            }
            hwc_parser::CutoutShape::Circle { radius, at } => (at, None, None, Some(radius)),
        };

        let at_resolved = if at_raw.is_relative() {
            solver
                .resolve_position(at_raw)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("cutout position for plane '{}'", plane.name),
                    reason: e.to_string(),
                })?
        } else {
            at_raw.clone()
        };

        let at_pt = spanning_coordinate_to_point(&at_resolved, &coord_ctx, false).map_err(|e| {
            IrError::CoordinateResolutionFailed {
                coordinate_str: format!("cutout position for plane '{}'", plane.name),
                reason: e,
            }
        })?;

        let width_nm = w_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table).map_err(
                    |err| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("cutout width for plane '{}'", plane.name),
                        reason: err.to_string(),
                    },
                )
            })
            .transpose()?;

        let height_nm = h_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table).map_err(
                    |err| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("cutout height for plane '{}'", plane.name),
                        reason: err.to_string(),
                    },
                )
            })
            .transpose()?;

        let radius_nm = r_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table).map_err(
                    |err| IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("cutout radius for plane '{}'", plane.name),
                        reason: err.to_string(),
                    },
                )
            })
            .transpose()?;

        resolved_cutouts.push(ResolvedCutout {
            at_pt,
            width_nm,
            height_nm,
            radius_nm,
        });
    }

    drop(solver);

    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);

    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);

    bbox_tracker.register(plane.name.to_string(), bbox, start_with_z);

    // v0.1.8: Register plane in EntityGraph for O(1) resolution
    let net_id = if let Some(net_name) = &plane.net {
        let _min_width_nm = space.fabrication_constraints.as_ref().map(|c| c.trace.min_width_nm)
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
        .register_space_entity(&plane.name.base, bbox, net_id, z_start_nm);

    // Note: Substrate layer registration happens after netlist processing (see below)
    // to ensure we have the correct resolved net_id

    println!(
        "   ├─ Registered plane '{}' bbox: min=({:.3}, {:.3}, {:.3}) max=({:.3}, {:.3}, {:.3})",
        plane.name,
        start_with_z.x as f64 / 1_000_000.0,
        start_with_z.y as f64 / 1_000_000.0,
        start_with_z.z as f64 / 1_000_000.0,
        end_with_z.x as f64 / 1_000_000.0,
        end_with_z.y as f64 / 1_000_000.0,
        end_with_z.z as f64 / 1_000_000.0,
    );

    if let Some(substrate_bbox) = &space.substrate_bbox {
        if bbox.intersects(substrate_bbox) && space.substrate_material_id != material_id {
            let is_conductor = space.material_registry.is_conductor(material_id);
            let is_substrate_insulator = space
                .material_registry
                .is_insulator(space.substrate_material_id)
                || space
                    .material_registry
                    .is_semiconductor(space.substrate_material_id);

            if is_conductor && is_substrate_insulator {
                let plane_net_id = if let Some(net_name) = &plane.net {
                    space
                        .netlist
                        .get_net_by_name(net_name.base.as_str())
                        .unwrap_or(hwc_engine::netlist::NetId::new(0))
                } else {
                    hwc_engine::netlist::NetId::new(0)
                };
                space
                    .entity_graph
                    .drill_hole(bbox, None, plane_net_id.raw());
                println!(
                    "   ├─ Auto-carved substrate for plane '{}' ({})",
                    plane.name, plane.material
                );
            } else {
                let substrate_material_name = space
                    .material_registry
                    .get_name(space.substrate_material_id)
                    .unwrap_or("Unknown");

                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Substrate interpenetration detected: Plane '{}' ({}) overlaps with the base substrate ({}). \
                         Use the same material as the substrate, or place the plane outside the substrate bounds.",
                        plane.name,
                        plane.material,
                        substrate_material_name
                    ),
                    component: plane.name.to_string().into(),
                });
            }
        }
    }

    for existing in &space.pours {
        if let Some(existing_bbox) = &existing.bbox {
            if bbox.intersects(existing_bbox) {
                let z_overlap =
                    bbox.max.z > existing_bbox.min.z && existing_bbox.max.z > bbox.min.z;
                if z_overlap && existing.material_name != plane.material {
                    return Err(IrError::MaterialInterpenetration {
                        pour_a: existing.name.clone(),
                        mat_a: existing.material_name.clone(),
                        pour_b: plane.name.to_string(),
                        mat_b: plane.material.clone(),
                        z_nm: z_start_nm,
                    });
                }
            }
        }
    }

    let resolved_net_name = plane.net.as_ref().map(|n| n.base.clone());

    space.pours.push(PourMetadata {
        name: plane.name.to_string(),
        material_name: plane.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name.clone(),
        area_nm2,
        bbox: Some(bbox),
        device_binding: None,
        merged_region_id: None,
        waivers: Default::default(),
    });

    let net_id = if let Some(net_name) = resolved_net_name.as_ref() {
        let center_x = (start_with_z.x + end_with_z.x) / 2;
        let center_y = (start_with_z.y + end_with_z.y) / 2;
        let center_z = (start_with_z.z + end_with_z.z) / 2;

        let plane_component_id = space.netlist.add_component(
            plane.name.to_string(),
            format!("Plane({})", plane.material).into(),
            (center_x, center_y, center_z),
        );

        // v0.1.9: Use __virtual_ naming convention for routing compatibility
        let virtual_pin_name = format!("__virtual_{}", plane.name);
        let anchor_pin_id = space.netlist.add_pin(
            plane_component_id,
            virtual_pin_name.clone().into(),
            (0, 0, 0),
            None,
        );

        let net_id_handle =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str()) {
                existing_net
            } else {
                space
                    .netlist
                    .add_net(net_name.clone(), 100_000, material_id)
            };

        space.netlist.connect_pin(anchor_pin_id, net_id_handle);

        space.entity_graph.add_component_pin(
            center_x,
            center_y,
            center_z,
            plane.name.to_string(),
            virtual_pin_name.into(),
            Some(net_name.clone()),
        );

        // println!(
        //     "   ├─ Registered anchor point for plane '{}' at ({:.3}mm, {:.3}mm, {:.3}mm) on net '{}'",
        //     plane.name,
        //     center_x as f64 / 1_000_000.0,
        //     center_y as f64 / 1_000_000.0,
        //     center_z as f64 / 1_000_000.0,
        //     net_name
        // );

        net_id_handle.raw()
    } else {
        0
    };

    // v0.1.9: Register as substrate layer so routing can see it as an obstacle
    // Planes with net_id are conductive pours, planes without net_id are keepout zones
    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);
    eprintln!(
        "[SUBSTRATE DEBUG] Adding plane '{}' as substrate layer: material_id={}, net={}, bbox=({},{},{}) to ({},{},{})",
        plane.name,
        material_id,
        net_id,
        bbox.min.x, bbox.min.y, bbox.min.z,
        bbox.max.x, bbox.max.y, bbox.max.z
    );
    space.entity_graph.add_substrate_layer(
        material_id,
        net_id,
        bbox,
        hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour,
    );

    for rc in resolved_cutouts {
        if let (Some(w), Some(h)) = (rc.width_nm, rc.height_nm) {
            let cutout_start = Point3D::new(rc.at_pt.x, rc.at_pt.y, z_start_nm);
            let cutout_end = Point3D::new(rc.at_pt.x + w, rc.at_pt.y + h, z_end_nm);
            let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
            space.entity_graph.drill_hole(cutout_bbox, None, 0);
        } else if let Some(r) = rc.radius_nm {
            let rf = r as f64;
            let cutout_start =
                Point3D::new(rc.at_pt.x - rf as i64, rc.at_pt.y - rf as i64, z_start_nm);
            let cutout_end = Point3D::new(rc.at_pt.x + rf as i64, rc.at_pt.y + rf as i64, z_end_nm);
            let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
            space
                .entity_graph
                .add_circle_substrate_layer(0, 0, cutout_bbox, r);
        }
    }

    Ok(())
}
