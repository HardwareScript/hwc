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

pub fn place_plane(
    space: &mut HardwareSpace,
    plane: &hwc_parser::PlanePlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space.material_registry.get_id(&plane.material).ok_or_else(|| {
        IrError::UndeclaredMaterial { material: plane.material.clone() }
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

    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    let (start, end, area_nm2) = match (&plane.from, &plane.to) {
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

            let s = spanning_coordinate_to_point(&from, &coord_ctx, false)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("plane '{}' from", plane.name),
                    reason: e,
                })?;
            let e = spanning_coordinate_to_point(&to, &coord_ctx, true)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("plane '{}' to", plane.name),
                    reason: e,
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
    };

    let mut resolved_cutouts = Vec::new();
    for cutout in &plane.cutouts {
        let (at_raw, w_expr, h_expr, r_expr) = match cutout {
            hwc_parser::CutoutShape::Rectangle {
                width,
                height,
                at,
            } => (at, Some(width), Some(height), None),
            hwc_parser::CutoutShape::Circle { radius, at } => (at, None, None, Some(radius)),
        };

        let at_resolved = if at_raw.is_relative() {
            solver.resolve_position(at_raw).map_err(|e| {
                IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("cutout position for plane '{}'", plane.name),
                    reason: e.to_string(),
                }
            })?
        } else {
            at_raw.clone()
        };

        let at_pt = spanning_coordinate_to_point(&at_resolved, &coord_ctx, false)
            .map_err(|e| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("cutout position for plane '{}'", plane.name),
                reason: e,
            })?;

        let width_nm = w_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table).map_err(
                    |err| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("cutout width for plane '{}'", plane.name),
                            reason: err.to_string(),
                        }
                    },
                )
            })
            .transpose()?;

        let height_nm = h_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table).map_err(
                    |err| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("cutout height for plane '{}'", plane.name),
                            reason: err.to_string(),
                        }
                    },
                )
            })
            .transpose()?;

        let radius_nm = r_expr
            .map(|e| {
                crate::ir::conversions::evaluate_expression_to_nm(e, ctx.symbol_table).map_err(
                    |err| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("cutout radius for plane '{}'", plane.name),
                            reason: err.to_string(),
                        }
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
                space.entity_graph.drill_hole(bbox, None, plane_net_id.raw());
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

        let anchor_pin_id =
            space
                .netlist
                .add_pin(plane_component_id, "anchor".into(), (0, 0, 0), None);

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
            "anchor".into(),
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

    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);
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
            let cutout_start = Point3D::new(
                rc.at_pt.x - rf as i64,
                rc.at_pt.y - rf as i64,
                z_start_nm,
            );
            let cutout_end = Point3D::new(
                rc.at_pt.x + rf as i64,
                rc.at_pt.y + rf as i64,
                z_end_nm,
            );
            let cutout_bbox = hwc_engine::geometry::BoundingBox::new(cutout_start, cutout_end);
            space
                .entity_graph
                .add_circle_substrate_layer(0, 0, cutout_bbox, r);
        }
    }

    Ok(())
}
