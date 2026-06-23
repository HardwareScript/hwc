use super::super::super::errors::IrError;
use super::super::context::{ComponentPlacementData, PlacementContext};
use super::super::helpers::parse_rectangle_dimensions;
use super::super::pour::place_pour;
use crate::bounding_box_tracker::BoundingBoxTracker;
use hwc_engine::{
    geometry::{BoundingBox, Point3D},
    geometry_router::Via,
    netlist::NetId,
    space::ContactMetadata,
    HardwareSpace,
};
use hwc_parser::{OriginXY, OriginZ};

fn offset_declarative_coord(
    coord: &hwc_parser::Coordinate,
    position: &Point3D,
    ctx: &PlacementContext,
) -> hwc_parser::Coordinate {
    match coord {
        hwc_parser::Coordinate::Declarative { x, y, z, span } => {
            let x_nm = crate::ir::conversions::evaluate_expression_to_nm(x, ctx.symbol_table)
                .unwrap_or(0);
            let y_nm = crate::ir::conversions::evaluate_expression_to_nm(y, ctx.symbol_table)
                .unwrap_or(0);
            let z_nm = crate::ir::conversions::evaluate_expression_to_nm(z, ctx.symbol_table)
                .unwrap_or(0);
            eprintln!("[DEBUG offset] position=({}, {}, {}) local=({}, {}, {}) -> abs=({}, {}, {})",
                position.x, position.y, position.z, x_nm, y_nm, z_nm,
                position.x + x_nm, position.y + y_nm, position.z + z_nm);
            hwc_parser::Coordinate::Positional {
                x: hwc_parser::Expression::Measurement {
                    value: (position.x + x_nm) as f64 / 1_000_000.0,
                    unit: hwc_parser::Unit::Millimeter,
                    span: *span,
                },
                y: hwc_parser::Expression::Measurement {
                    value: (position.y + y_nm) as f64 / 1_000_000.0,
                    unit: hwc_parser::Unit::Millimeter,
                    span: *span,
                },
                z: hwc_parser::Expression::Measurement {
                    value: (position.z + z_nm) as f64 / 1_000_000.0,
                    unit: hwc_parser::Unit::Millimeter,
                    span: *span,
                },
                span: *span,
            }
        }
        _ => coord.clone(),
    }
}

pub fn unroll_internal_features(
    space: &mut HardwareSpace,
    pd: &ComponentPlacementData,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    if let Ok(component_def) = ctx
        .symbol_table
        .get_component(pd.component.component_type.as_str())
    {
        if let Some(layout) = &component_def.layout {
            if !layout.pin_positions.is_empty() {
                let component_id = space
                    .netlist
                    .get_component_by_name(&pd.name)
                    .expect("Component should exist in netlist after placement");

                let (width_nm, height_nm, _depth_nm) = layout
                    .shape
                    .as_ref()
                    .and_then(|s| parse_rectangle_dimensions(s, ctx.symbol_table))
                    .unwrap_or((1_000_000, 1_000_000, 1_000_000));

                for (pin_name, pin_pos) in &layout.pin_positions {
                    let net_assignment = if let Some(binding) =
                        pd.component.pin_net_bindings.get(pin_name.as_str())
                    {
                        match binding {
                            hwc_parser::NetBinding::Simple(net_name) => Some(net_name.clone()),
                            hwc_parser::NetBinding::Conditional { .. } => None,
                        }
                    } else {
                        layout
                            .internal_pours
                            .iter()
                            .find(|pour| pour.net.is_some())
                            .and_then(|pour| pour.net.as_ref())
                            .map(|net_id| net_id.base.clone())
                    };

                    if let Some(ref net_name_str) = net_assignment {
                        let default_trace_width_nm = space.fabrication_constraints.as_ref()
                            .map(|c| c.trace.min_width_nm)
                            .expect("Fabrication constraints (min_trace_width) must be defined in profile");

                        let net_id = if let Some(existing_net_id) =
                            space.netlist.get_net_by_name(net_name_str)
                        {
                            existing_net_id
                        } else {
                            space
                                .netlist
                                .add_net(net_name_str.clone(), default_trace_width_nm, 2)
                        };

                        let pins = space.netlist.get_component_pins(component_id);
                        if let Some(&pin_id) = pins.iter().find(|&&pid| {
                            space
                                .netlist
                                .get_pin(pid)
                                .map(|p| p.name == *pin_name)
                                .unwrap_or(false)
                        }) {
                            space.netlist.connect_pin(pin_id, net_id);
                        }
                    }

                    let (center_x, center_y) = match ctx.origin.xy {
                        OriginXY::TL => {
                            (pd.position.x + width_nm / 2, pd.position.y - height_nm / 2)
                        }
                        OriginXY::TR => {
                            (pd.position.x - width_nm / 2, pd.position.y - height_nm / 2)
                        }
                        OriginXY::BL => {
                            (pd.position.x + width_nm / 2, pd.position.y + height_nm / 2)
                        }
                        OriginXY::BR => {
                            (pd.position.x - width_nm / 2, pd.position.y + height_nm / 2)
                        }
                    };

                    let half_w = width_nm / 2;
                    let half_h = height_nm / 2;
                    let angle_rad = pd.rotation_deg.to_radians();
                    let cos_theta = angle_rad.cos();
                    let sin_theta = angle_rad.sin();

                    let mirror_multiplier = match pd.mount_side {
                        hwc_parser::MountingSide::Top | hwc_parser::MountingSide::Embedded => 1,
                        hwc_parser::MountingSide::Bottom => -1,
                    };

                    let lx = ((pin_pos.x * 1_000_000.0) as i64 * mirror_multiplier) - half_w;
                    let ly = (pin_pos.y * 1_000_000.0) as i64 - half_h;

                    let rx = (lx as f64 * cos_theta - ly as f64 * sin_theta) as i64;
                    let ry = (lx as f64 * sin_theta + ly as f64 * cos_theta) as i64;

                    let absolute_x_nm = center_x + rx;
                    let absolute_y_nm = match ctx.origin.xy {
                        OriginXY::TL | OriginXY::TR => center_y - ry,
                        OriginXY::BL | OriginXY::BR => center_y + ry,
                    };
                    let absolute_z_nm =
                        pd.position.z + (pin_pos.z.unwrap_or(0.0) * 1_000_000.0) as i64;

                    let pin_point = Point3D::new(absolute_x_nm, absolute_y_nm, absolute_z_nm);
                    let pin_bbox = BoundingBox::new(pin_point, pin_point);
                    let pin_anchor_name = format!("{}.{}", pd.name, pin_name);
                    bbox_tracker.register(pin_anchor_name.clone().into(), pin_bbox, pin_point);

                    let is_tht = component_def
                        .render
                        .as_ref()
                        .map(|r| r.shape.as_deref() == Some("tht_package"))
                        .unwrap_or(false);
                    let pad_shape = layout.pad_shapes.get(pin_name);

                    if is_tht || pad_shape.is_some() {
                        let drill_diameter_nm = if let Some(ps) = pad_shape {
                            if ps.starts_with("Circle(") {
                                let val_str =
                                    ps.trim_start_matches("Circle(").trim_end_matches(")");
                                (val_str.trim_end_matches("mm").parse::<f64>().unwrap_or(1.0)
                                    * 1_000_000.0) as i64
                            } else {
                                1_000_000
                            }
                        } else {
                            1_000_000
                        };

                        if let Some(substrate_bbox) = space.substrate_bbox {
                            let hole_bbox = BoundingBox::new(
                                Point3D::new(
                                    absolute_x_nm - drill_diameter_nm / 2,
                                    absolute_y_nm - drill_diameter_nm / 2,
                                    substrate_bbox.min.z,
                                ),
                                Point3D::new(
                                    absolute_x_nm + drill_diameter_nm / 2,
                                    absolute_y_nm + drill_diameter_nm / 2,
                                    substrate_bbox.max.z,
                                ),
                            );

                            let via_net_id = if let Some(ref net_name_str) = net_assignment {
                                space
                                    .netlist
                                    .get_net_by_name(net_name_str)
                                    .unwrap_or(NetId::new(0))
                            } else {
                                NetId::new(0)
                            };

                            space.drill_hole(hole_bbox, Some(drill_diameter_nm), via_net_id);

                            let copper_material_id =
                                space.material_registry.get_id("Copper").unwrap_or(2);
                            let plating_thickness_nm = 25_000;
                            let outer_diameter_nm = drill_diameter_nm;
                            let inner_diameter_nm = drill_diameter_nm - (2 * plating_thickness_nm);

                            let min_annular_ring_nm = space
                                .fabrication_constraints
                                .as_ref()
                                .map(|c| c.via.min_annular_ring_nm)
                                .unwrap_or(150_000);

                            let pad_diameter_nm = drill_diameter_nm + (2 * min_annular_ring_nm);

                            space.entity_graph.add_tube_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                hole_bbox,
                                outer_diameter_nm as u32,
                                inner_diameter_nm as u32,
                                pad_diameter_nm as u32,
                                16,
                                hwc_engine::geometry_router::entity_graph::CapType::Annular,
                                hwc_engine::geometry_router::entity_graph::CapType::Annular,
                                None,
                            );

                            let pad_half_nm = pad_diameter_nm / 2;
                            let start_z_nm = (substrate_bbox.min.z / space.voxel_size.z_nm)
                                * space.voxel_size.z_nm;
                            let pad_bbox_start = BoundingBox::new(
                                Point3D::new(
                                    absolute_x_nm - pad_half_nm,
                                    absolute_y_nm - pad_half_nm,
                                    start_z_nm,
                                ),
                                Point3D::new(
                                    absolute_x_nm + pad_half_nm,
                                    absolute_y_nm + pad_half_nm,
                                    start_z_nm + space.voxel_size.z_nm,
                                ),
                            );
                            space.entity_graph.add_cylinder_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                pad_bbox_start,
                                pad_diameter_nm,
                                16,
                                0,
                            );

                            let end_z_nm = (substrate_bbox.max.z / space.voxel_size.z_nm - 1)
                                * space.voxel_size.z_nm;
                            let pad_bbox_end = BoundingBox::new(
                                Point3D::new(
                                    absolute_x_nm - pad_half_nm,
                                    absolute_y_nm - pad_half_nm,
                                    end_z_nm,
                                ),
                                Point3D::new(
                                    absolute_x_nm + pad_half_nm,
                                    absolute_y_nm + pad_half_nm,
                                    end_z_nm + space.voxel_size.z_nm,
                                ),
                            );
                            space.entity_graph.add_cylinder_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                pad_bbox_end,
                                pad_diameter_nm,
                                16,
                                0,
                            );

                            let board_max_z_nm = (space.grid_cells().z_layers as i64).saturating_sub(1)
                                * space.voxel_size.z_nm;
                            let via = Via::new(
                                (absolute_x_nm, absolute_y_nm),
                                substrate_bbox.min.z,
                                board_max_z_nm,
                                drill_diameter_nm,
                                via_net_id,
                                0,
                                board_max_z_nm,
                                space.voxel_size.z_nm,
                                min_annular_ring_nm,
                            );
                            space.add_vias(vec![via]);

                            space.contacts.push(ContactMetadata {
                                name: format!("{}_{}_via", pd.name, pin_name).into(),
                                material_name: "Copper".into(),
                                z_start_nm: substrate_bbox.min.z,
                                z_end_nm: substrate_bbox.max.z,
                                net: net_assignment.clone(),
                                bridge: None,
                                bbox: Some(hole_bbox),
                                voxels: Vec::new(),
                                is_tented: false,
                                mask_clearance_diameter_nm: None,
                            });
                        }
                    }

                    space.entity_graph.add_component_pin(
                        absolute_x_nm,
                        absolute_y_nm,
                        absolute_z_nm,
                        pd.name.clone().into(),
                        pin_name.clone(),
                        net_assignment.clone(),
                    );
                }
            }

            if !layout.internal_pours.is_empty() {
                for pour in &layout.internal_pours {
                    if pour.boundary.is_some() {
                        let mut unrolled_pour = pour.clone();
                        unrolled_pour.name = hwc_parser::ComponentName::simple(
                            format!("{}_{}", pd.name, pour.name).into(),
                            pour.span,
                        );

                        if let Some(hwc_parser::PourBoundary::Rect(from, to)) =
                            &mut unrolled_pour.boundary
                        {
                            let orig = pour.boundary.as_ref().unwrap();
                            if let hwc_parser::PourBoundary::Rect(orig_from, orig_to) = orig {
                                **from = offset_declarative_coord(orig_from, &pd.position, ctx);
                                **to = offset_declarative_coord(orig_to, &pd.position, ctx);
                            }
                        }

                        if let Some(hwc_parser::PourBoundary::Circle { center, .. }) =
                            &mut unrolled_pour.boundary
                        {
                            let orig = pour.boundary.as_ref().unwrap();
                            if let hwc_parser::PourBoundary::Circle { center: orig_center, .. } = orig {
                                **center = offset_declarative_coord(orig_center, &pd.position, ctx);
                            }
                        }

                        unrolled_pour.device =
                            pour.device.as_ref().map(|d| hwc_parser::DeviceBinding {
                                device_name: pd.name.clone().into(),
                                terminal: d.terminal.clone(),
                                span: d.span,
                            });

                        if let Some(anchor_bbox) = bbox_tracker.get(&pd.name) {
                            let anchor_z = anchor_bbox.min.z;

                            let layer_name = ctx.stackup_manager.get_layer_name_at_z(anchor_z);

                            let copper_thickness = if let Some(t_expr) = &pour.thickness {
                                crate::ir::conversions::evaluate_expression_to_nm(
                                    t_expr,
                                    ctx.symbol_table,
                                )
                                .map_err(|e| {
                                    IrError::PlacementError(format!(
                                        "Failed to evaluate pad thickness: {}",
                                        e
                                    ))
                                })?
                            } else if let Some(ref name) = layer_name {
                                ctx.stackup_manager
                                    .get_layer_thickness(name)
                                    .ok_or_else(|| {
                                        IrError::PlacementError(format!(
                                            "Layer '{}' has no thickness in stackup",
                                            name
                                        ))
                                    })?
                            } else {
                                return Err(IrError::PlacementError(format!(
                                    "Component pad '{}' at Z={}nm is not within any defined stackup layer. \
                                     Check your profile stackup or component elevation.",
                                    pour.name, anchor_z
                                )));
                            };

                            let p_min = anchor_z;
                            let p_max = anchor_z + copper_thickness;

                            /*
                            eprintln!("[DEBUG unroll] Pad '{}' (anchor={}) layer={:?} anchor_z={}nm -> thickness={}nm", 
                                pour.name, anchor_name, layer_name, anchor_z, copper_thickness);
                            */

                            unrolled_pour.elevation = hwc_parser::Elevation::Physical {
                                start: hwc_parser::Expression::Measurement {
                                    value: p_min as f64 / 1_000_000.0,
                                    unit: hwc_parser::Unit::Millimeter,
                                    span: pour.span,
                                },
                                end: Some(hwc_parser::Expression::Measurement {
                                    value: p_max as f64 / 1_000_000.0,
                                    unit: hwc_parser::Unit::Millimeter,
                                    span: pour.span,
                                }),
                            };

                            unrolled_pour.thickness = Some(hwc_parser::Expression::Measurement {
                                value: copper_thickness as f64 / 1_000_000.0,
                                unit: hwc_parser::Unit::Millimeter,
                                span: pour.span,
                            });
                        }

                        let mut world_origin = ctx.origin;
                        world_origin.xy = OriginXY::BL;
                        world_origin.z = OriginZ::Bottom;

                        let world_ctx = PlacementContext {
                            symbol_table: ctx.symbol_table,
                            eval_context: ctx.eval_context,
                            stackup_manager: ctx.stackup_manager,
                            collector: ctx.collector,
                            profile: ctx.profile,
                            origin: world_origin,
                        };

                        place_pour(space, &unrolled_pour, bbox_tracker, &world_ctx)?;
                    }
                }
            }
        }
    }
    Ok(())
}
