use super::super::conversions::{spanning_coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::context::PlacementContext;
use hwc_engine::space::PourMetadata;
use hwc_engine::{HardwareSpace, Point3D};

pub fn place_pour(
    space: &mut HardwareSpace,
    pour: &hwc_parser::PourPlacement,
    bbox_tracker: &mut crate::bounding_box_tracker::BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    let material_id = space.material_registry.get_id(&pour.material).ok_or_else(|| {
        IrError::UndeclaredMaterial { material: pour.material.clone() }
    })?;

    let boundary = pour
        .boundary
        .as_ref()
        .ok_or_else(|| IrError::PlacementConstraint {
            message: format!("Pour '{}' missing boundary", pour.name),
            component: pour.name.to_string().into(),
        })?;

    let layer_name = match &pour.elevation {
        hwc_parser::Elevation::Semantic(id) => id.to_string(),
        _ => "top_copper".to_string(),
    };

    let thickness_nm = if let Some(t_expr) = &pour.thickness {
        crate::ir::conversions::evaluate_expression_to_nm(t_expr, ctx.symbol_table).map_err(
            |e| IrError::CoordinateResolutionFailed {
                coordinate_str: format!("pour '{}' thickness", pour.name),
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

    if thickness_nm == 0 && pour.thickness.is_none() {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Could not resolve physical thickness for pour '{}' on layer '{}'. \
                 Ensure the layer is defined in the profile stackup or provide an explicit 'thickness:' property.",
                pour.name, layer_name
            ),
            component: pour.name.to_string().into(),
        });
    }

    let z_start_nm = ctx
        .stackup_manager
        .resolve_elevation(&pour.elevation, ctx.symbol_table)?;
    let z_end_nm = z_start_nm + thickness_nm;

    /*
    eprintln!(
        "[DEBUG pour] '{}' elevation={:?} -> z_start={}nm, thickness={}nm, z_end={}nm",
        pour.name, pour.elevation, z_start_nm, thickness_nm, z_end_nm
    );
    */

    let solver = crate::constraint_solver::ConstraintSolver::new(bbox_tracker, ctx.eval_context);

    let coord_ctx = CoordinateContext {
        origin: ctx.origin,
        space_dimensions: &space.dimensions,
        symbol_table: ctx.symbol_table,
        eval_context: ctx.eval_context,
        bbox_tracker: Some(bbox_tracker),
        stackup_manager: ctx.stackup_manager,
        profile: ctx.profile,
    };

    let mut circle_radius_nm: Option<i64> = None;
    let (start, end, area_nm2) = match boundary {
        hwc_parser::PourBoundary::Rect(from_raw, to_raw) => {
            let from = if from_raw.is_relative() {
                solver.resolve_position(from_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' from position", pour.name),
                        reason: e.to_string(),
                    }
                })?
            } else {
                (**from_raw).clone()
            };

            let to = if to_raw.is_relative() {
                solver.resolve_position(to_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' to position", pour.name),
                        reason: e.to_string(),
                    }
                })?
            } else {
                (**to_raw).clone()
            };

            let s = spanning_coordinate_to_point(&from, &coord_ctx, false)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("pour '{}' from", pour.name),
                    reason: e,
                })?;
            let e = spanning_coordinate_to_point(&to, &coord_ctx, true)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("pour '{}' to", pour.name),
                    reason: e,
                })?;

            let w = (e.x - s.x).abs();
            let h = (e.y - s.y).abs();
            (s, e, w * h)
        }
        hwc_parser::PourBoundary::Circle {
            center: center_raw,
            radius,
        } => {
            let radius_nm =
                crate::ir::conversions::evaluate_expression_to_nm(radius, ctx.symbol_table)
                    .map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: format!("pour '{}' circle radius", pour.name),
                            reason: e.to_string(),
                        }
                    })?;
            circle_radius_nm = Some(radius_nm);

            let center_resolved = if center_raw.is_relative() {
                solver.resolve_position(center_raw).map_err(|e| {
                    IrError::CoordinateResolutionFailed {
                        coordinate_str: format!("pour '{}' circle center", pour.name),
                        reason: e.to_string(),
                    }
                })?
            } else {
                *center_raw.clone()
            };

            let center_pt = spanning_coordinate_to_point(&center_resolved, &coord_ctx, false)
                .map_err(|e| IrError::CoordinateResolutionFailed {
                    coordinate_str: format!("pour '{}' circle center", pour.name),
                    reason: e,
                })?;

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

    let start_with_z = Point3D::new(start.x, start.y, z_start_nm);
    let end_with_z = Point3D::new(end.x, end.y, z_end_nm);

    let bbox = hwc_engine::geometry::BoundingBox::new(start_with_z, end_with_z);

    bbox_tracker.register(pour.name.to_string(), bbox, start_with_z);

    // println!(
    //     "   ├─ Registered pour '{}' bbox: min=({:.3}, {:.3}, {:.3}) max=({:.3}, {:.3}, {:.3})",
    //     pour.name,
    //     start_with_z.x as f64 / 1_000_000.0,
    //     start_with_z.y as f64 / 1_000_000.0,
    //     start_with_z.z as f64 / 1_000_000.0,
    //     end_with_z.x as f64 / 1_000_000.0,
    //     end_with_z.y as f64 / 1_000_000.0,
    //     end_with_z.z as f64 / 1_000_000.0,
    // );

    let skip_substrate_check = pour.waivers.merge == hwc_parser::MergeWaiver::All;

    if let Some(substrate_bbox) = &space.substrate_bbox {
        if bbox.intersects(substrate_bbox)
            && !skip_substrate_check
            && space.substrate_material_id != material_id
        {
            let is_conductor = space.material_registry.is_conductor(material_id);
            let is_substrate_insulator = space
                .material_registry
                .is_insulator(space.substrate_material_id)
                || space
                    .material_registry
                    .is_semiconductor(space.substrate_material_id);

            if is_conductor && is_substrate_insulator {
                let pour_net_id = if let Some(net_name) = &pour.net {
                    space
                        .netlist
                        .get_net_by_name(net_name.base.as_str())
                        .unwrap_or(hwc_engine::netlist::NetId::new(0))
                } else {
                    hwc_engine::netlist::NetId::new(0)
                };
                space.entity_graph.drill_hole(bbox, None, pour_net_id.raw());
                println!(
                    "   ├─ Auto-carved substrate for pour '{}' ({})",
                    pour.name, pour.material
                );
            } else {
                let substrate_material_name = space
                    .material_registry
                    .get_name(space.substrate_material_id)
                    .unwrap_or("Unknown");

                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Substrate interpenetration detected: Pour '{}' ({}) overlaps with the base substrate ({}). \
                         Use the same material as the substrate, or place the pour outside the substrate bounds.",
                        pour.name,
                        pour.material,
                        substrate_material_name
                    ),
                    component: pour.name.to_string().into(),
                });
            }
        }
    }

    for existing in &space.pours {
        if let Some(existing_bbox) = &existing.bbox {
            if bbox.intersects(existing_bbox) {
                let z_overlap =
                    bbox.max.z > existing_bbox.min.z && existing_bbox.max.z > bbox.min.z;
                if z_overlap {
                    let is_waived = pour.waivers.merge == hwc_parser::MergeWaiver::All;

                    if existing.material_name != pour.material {
                        if is_waived {
                            ctx.collector
                                .report(hwc_diagnostics::WaiverApplied::new(&format!(
                                    "Pour '{}' (mat: {}) allowed to overlap '{}' (mat: {})",
                                    pour.name, pour.material, existing.name, existing.material_name
                                )));
                        } else {
                            return Err(IrError::MaterialInterpenetration {
                                pour_a: existing.name.clone(),
                                mat_a: existing.material_name.clone(),
                                pour_b: pour.name.to_string(),
                                mat_b: pour.material.clone(),
                                z_nm: z_start_nm,
                            });
                        }
                    }
                }
            }
        }
    }

    let device_binding = pour
        .device
        .as_ref()
        .map(|binding| hwc_engine::space::DeviceBinding {
            device_name: binding.device_name.clone(),
            terminal: binding.terminal.clone(),
        });

    let mut resolved_net_name = pour.net.as_ref().map(|n| n.base.clone());

    if let Some(binding) = &pour.device {
        let resolved_opt = (|| {
            let netlist = &space.netlist;
            let comp_id = netlist.get_component_by_name(binding.device_name.as_str())?;
            let pins = netlist.get_component_pins(comp_id);

            pins.iter().find_map(|&pin_id| {
                let pin_data = netlist.get_pin(pin_id)?;
                if pin_data.name == binding.terminal {
                    let net_id = pin_data.connected_net?;
                    let net_data = netlist.get_net(net_id)?;
                    Some(net_data.name.to_string())
                } else {
                    None
                }
            })
        })();

        if let Some(net_name) = resolved_opt {
            resolved_net_name = Some(net_name.into());
        }
    }

    space.pours.push(PourMetadata {
        name: pour.name.to_string(),
        material_name: pour.material.clone(),
        z_bottom_nm: z_start_nm,
        net: resolved_net_name.clone(),
        area_nm2,
        bbox: Some(bbox),
        device_binding,
        merged_region_id: None,
        waivers: pour.waivers.clone(),
    });

    let net_id = if let Some(net_name) = resolved_net_name.as_ref() {
        let center_x = (start_with_z.x + end_with_z.x) / 2;
        let center_y = (start_with_z.y + end_with_z.y) / 2;
        let center_z = (start_with_z.z + end_with_z.z) / 2;

        let pour_component_id = space.netlist.add_component(
            pour.name.to_string(),
            format!("Pour({})", pour.material).into(),
            (center_x, center_y, center_z),
        );

        let anchor_pin_id =
            space
                .netlist
                .add_pin(pour_component_id, "anchor".into(), (0, 0, 0), None);

        let net_id_handle =
            if let Some(existing_net) = space.netlist.get_net_by_name(net_name.as_str()) {
                existing_net
            } else {
                space
                    .netlist
                    .add_net(net_name.clone(), 100_000, material_id)
            };

        space.netlist.connect_pin(anchor_pin_id, net_id_handle);

        if let Some(binding) = &pour.device {
            if let Some(target_comp_id) = space.netlist.get_component_by_name(&binding.device_name)
            {
                if let Some(target_pin_id) = space
                    .netlist
                    .get_pin_by_name(target_comp_id, &binding.terminal)
                {
                    space.netlist.connect_pin(target_pin_id, net_id_handle);
                    // println!(
                    //     "   ├─ Bound logical pin '{}.{}' to net '{}'",
                    //     binding.device_name, binding.terminal, net_name
                    // );

                    space.entity_graph.set_pin_net(
                        &binding.device_name,
                        &binding.terminal,
                        net_name.as_str(),
                    );
                }
            }
        }

        let comp_name_for_pin = if let Some(binding) = &pour.device {
            binding.device_name.clone()
        } else {
            pour.name.to_string().into()
        };

        space.entity_graph.add_component_pin(
            center_x,
            center_y,
            center_z,
            comp_name_for_pin,
            "anchor".into(),
            Some(net_name.clone()),
        );

        // println!(
        //     "   ├─ Registered anchor point for pour '{}' at ({:.3}mm, {:.3}mm, {:.3}mm) on net '{}'",
        //     pour.name,
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
    if let Some(radius) = circle_radius_nm {
        space
            .entity_graph
            .add_circle_substrate_layer(material_id, net_id, bbox, radius);
    } else {
        space.entity_graph.add_substrate_layer(
            material_id,
            net_id,
            bbox,
            hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour,
        );
    }

    Ok(())
}
