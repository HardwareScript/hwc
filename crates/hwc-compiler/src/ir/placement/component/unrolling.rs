use crate::SymbolTable;
use hwc_engine::{HardwareSpace, geometry::{Point3D, BoundingBox}, netlist::NetId, space::ContactMetadata, geometry_router::Via};
use super::super::super::errors::IrError;
use super::super::super::stackup_manager::StackupManager;
use super::super::helpers::parse_rectangle_dimensions;
use super::super::pour::place_pour;
use crate::bounding_box_tracker::BoundingBoxTracker;
use hwc_parser::{EvaluationContext, OriginPoint, OriginXY, OriginZ};
use hwc_diagnostics::DiagnosticCollector;
use super::coordinates::transform_declarative_to_relative;

pub fn unroll_internal_features(
    space: &mut HardwareSpace,
    component: &hwc_parser::ComponentPlacement,
    name: &str,
    position: Point3D,
    rotation_deg: f64,
    mount_side: hwc_parser::MountingSide,
    origin: OriginPoint,
    symbol_table: &SymbolTable,
    bbox_tracker: &mut BoundingBoxTracker,
    eval_context: &EvaluationContext,
    collector: &DiagnosticCollector,
    profile: Option<&hwc_parser::ProfileDefinition>,
    stackup_manager: &StackupManager,
) -> Result<(), IrError> {
    if let Ok(component_def) = symbol_table.get_component(component.component_type.as_str()) {
        if let Some(layout) = &component_def.layout {
            // v0.1.7: Pass A - Connect pins to nets BEFORE pour unrolling.
            if !layout.pin_positions.is_empty() {
                let component_id = space
                    .netlist
                    .get_component_by_name(name)
                    .expect("Component should exist in netlist after placement");

                // Fetch dimensions for rotation math
                let (width_nm, height_nm, _depth_nm) = layout.shape.as_ref()
                    .and_then(|s| parse_rectangle_dimensions(s))
                    .unwrap_or((1_000_000, 1_000_000, 1_000_000)); // Default 1mm if unknown

                for (pin_name, pin_pos) in &layout.pin_positions {
                    let net_assignment = if let Some(binding) =
                        component.pin_net_bindings.get(pin_name.as_str())
                    {
                        match binding {
                            hwc_parser::NetBinding::Simple(net_name) => Some(net_name.clone()),
                            hwc_parser::NetBinding::Conditional { .. } => None,
                        }
                    } else {
                        layout.internal_pours.iter()
                            .find(|pour| pour.net.is_some())
                            .and_then(|pour| pour.net.as_ref())
                            .map(|net_id| net_id.base.clone())
                    };

                    if let Some(ref net_name_str) = net_assignment {
                        let default_trace_width_nm = space.fabrication_constraints.as_ref()
                            .map(|c| c.trace.min_width_nm)
                            .expect("Fabrication constraints (min_trace_width) must be defined in profile");

                        let net_id = if let Some(existing_net_id) = space.netlist.get_net_by_name(net_name_str) {
                            existing_net_id
                        } else {
                            space.netlist.add_net(net_name_str.clone(), default_trace_width_nm, 2)
                        };

                        let pins = space.netlist.get_component_pins(component_id);
                        if let Some(&pin_id) = pins.iter().find(|&&pid| {
                            space.netlist.get_pin(pid).map(|p| p.name == *pin_name).unwrap_or(false)
                        }) {
                            space.netlist.connect_pin(pin_id, net_id);
                        }
                    }

                    // Calculate absolute pin position for BBox registration
                    let (center_x, center_y) = match origin.xy {
                        OriginXY::TL => (position.x + width_nm / 2, position.y - height_nm / 2),
                        OriginXY::TR => (position.x - width_nm / 2, position.y - height_nm / 2),
                        OriginXY::BL => (position.x + width_nm / 2, position.y + height_nm / 2),
                        OriginXY::BR => (position.x - width_nm / 2, position.y + height_nm / 2),
                    };
                    
                    let half_w = width_nm / 2;
                    let half_h = height_nm / 2;
                    let angle_rad = (rotation_deg as f64).to_radians();
                    let cos_theta = angle_rad.cos();
                    let sin_theta = angle_rad.sin();

                    // v0.1.7: Component Mirroring for Bottom Mounting
                    let mirror_multiplier = match mount_side {
                        hwc_parser::MountingSide::Top | hwc_parser::MountingSide::Embedded => 1,
                        hwc_parser::MountingSide::Bottom => -1, // Flip X coordinates for bottom mount
                    };

                    let lx = ((pin_pos.x * 1_000_000.0) as i64 * mirror_multiplier) - half_w;
                    let ly = (pin_pos.y * 1_000_000.0) as i64 - half_h;

                    let rx = (lx as f64 * cos_theta - ly as f64 * sin_theta) as i64;
                    let ry = (lx as f64 * sin_theta + ly as f64 * cos_theta) as i64;

                    let absolute_x_nm = center_x + rx;
                    let absolute_y_nm = match origin.xy {
                        OriginXY::TL | OriginXY::TR => center_y - ry,
                        OriginXY::BL | OriginXY::BR => center_y + ry,
                    };
                    let absolute_z_nm = position.z + (pin_pos.z.unwrap_or(0.0) * 1_000_000.0) as i64;

                    // Register pin in BoundingBoxTracker for relative positioning of internal pours
                    let pin_point = Point3D::new(absolute_x_nm, absolute_y_nm, absolute_z_nm);
                    let pin_bbox = BoundingBox::new(pin_point, pin_point);
                    let pin_anchor_name = format!("{}.{}", name, pin_name);
                    bbox_tracker.register(pin_anchor_name.clone().into(), pin_bbox, pin_point);

                    // v0.1.7: Auto-Stitching (Limitation 7) - Drill and plate through-holes
                    let is_tht = component_def.render.as_ref().map(|r| r.shape.as_deref() == Some("tht_package")).unwrap_or(false);
                    let pad_shape = layout.pad_shapes.get(pin_name);
                    
                    if is_tht || pad_shape.is_some() {
                        let drill_diameter_nm = if let Some(ps) = pad_shape {
                            if ps.starts_with("Circle(") {
                                 let val_str = ps.trim_start_matches("Circle(").trim_end_matches(")");
                                 (val_str.trim_end_matches("mm").parse::<f64>().unwrap_or(1.0) * 1_000_000.0) as i64
                            } else {
                                1_000_000
                            }
                        } else {
                            1_000_000
                        };

                        if let Some(substrate_bbox) = space.substrate_bbox {
                            let hole_bbox = BoundingBox::new(
                                Point3D::new(absolute_x_nm - drill_diameter_nm / 2, absolute_y_nm - drill_diameter_nm / 2, substrate_bbox.min.z),
                                Point3D::new(absolute_x_nm + drill_diameter_nm / 2, absolute_y_nm + drill_diameter_nm / 2, substrate_bbox.max.z),
                            );

                            // Get net_id for the via/pads
                            let via_net_id = if let Some(ref net_name_str) = net_assignment {
                                space.netlist.get_net_by_name(net_name_str).unwrap_or(NetId::new(0))
                            } else {
                                NetId::new(0)
                            };

                            space.drill_hole(hole_bbox, Some(drill_diameter_nm), via_net_id);

                            let copper_material_id = space.material_registry.get_id("Copper").unwrap_or(2);
                            let plating_thickness_nm = 25_000;
                            let outer_diameter_nm = drill_diameter_nm;
                            let inner_diameter_nm = drill_diameter_nm - (2 * plating_thickness_nm);
                            
                            let min_annular_ring_nm = space.fabrication_constraints.as_ref()
                                .map(|c| c.via.min_annular_ring_nm)
                                .unwrap_or(150_000);
                            
                            let pad_diameter_nm = drill_diameter_nm + (2 * min_annular_ring_nm);

                            space.voxel_grid.add_tube_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                hole_bbox,
                                outer_diameter_nm as u32,
                                inner_diameter_nm as u32,
                                pad_diameter_nm as u32,
                                16,
                                hwc_engine::voxel_grid::CapType::Annular,
                                hwc_engine::voxel_grid::CapType::Annular,
                                None,
                            );

                            let pad_half_nm = pad_diameter_nm / 2;
                            let start_z_nm = (substrate_bbox.min.z / space.voxel_size.z_nm) * space.voxel_size.z_nm;
                            let pad_bbox_start = BoundingBox::new(
                                Point3D::new(absolute_x_nm - pad_half_nm, absolute_y_nm - pad_half_nm, start_z_nm),
                                Point3D::new(absolute_x_nm + pad_half_nm, absolute_y_nm + pad_half_nm, start_z_nm + space.voxel_size.z_nm),
                            );
                            space.voxel_grid.add_cylinder_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                pad_bbox_start,
                                pad_diameter_nm,
                                16,
                                0
                            );

                            let end_z_nm = (substrate_bbox.max.z / space.voxel_size.z_nm - 1) * space.voxel_size.z_nm;
                            let pad_bbox_end = BoundingBox::new(
                                Point3D::new(absolute_x_nm - pad_half_nm, absolute_y_nm - pad_half_nm, end_z_nm),
                                Point3D::new(absolute_x_nm + pad_half_nm, absolute_y_nm + pad_half_nm, end_z_nm + space.voxel_size.z_nm),
                            );
                            space.voxel_grid.add_cylinder_substrate_layer(
                                copper_material_id,
                                via_net_id.raw(),
                                pad_bbox_end,
                                pad_diameter_nm,
                                16,
                                0
                            );
                            
                            let board_max_z_nm = (space.grid.z_layers as i64).saturating_sub(1) * space.voxel_size.z_nm;
                            let via = Via::new(
                                (absolute_x_nm, absolute_y_nm),
                                substrate_bbox.min.z,
                                board_max_z_nm,
                                drill_diameter_nm,
                                via_net_id,
                                0,
                                board_max_z_nm,
                                space.voxel_size.z_nm,
                            );
                            space.add_vias(vec![via]);

                            space.contacts.push(ContactMetadata {
                                name: format!("{}_{}_via", name, pin_name).into(),
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

                    // v0.1.7: Register pin in VoxelGrid for Global Router discovery
                    space.voxel_grid.add_component_pin(
                        absolute_x_nm,
                        absolute_y_nm,
                        absolute_z_nm,
                        name.to_string().into(),
                        pin_name.clone(),
                        net_assignment.clone()
                    );
                }
            }

            if !layout.internal_pours.is_empty() {
                for pour in &layout.internal_pours {
                    if let Some(_) = &pour.boundary {
                        let mut unrolled_pour = pour.clone();
                        unrolled_pour.name = hwc_parser::ComponentName::simple(format!("{}_{}", name, pour.name).into(), pour.span);
                        
                        let anchor_name = if let Some(binding) = &pour.device {
                            format!("{}.{}", name, binding.terminal)
                        } else {
                            name.to_string().into()
                        };

                        if let Some((from, to)) = &mut unrolled_pour.boundary {
                            *from = transform_declarative_to_relative(from, &anchor_name);
                            *to = transform_declarative_to_relative(to, &anchor_name);
                        }

                        // Update device binding to point to this specific component instance
                        unrolled_pour.device = pour.device.as_ref().map(|d| hwc_parser::DeviceBinding {
                            device_name: name.to_string().into(),
                            terminal: d.terminal.clone(),
                            span: d.span,
                        });

                        // v0.1.7: Unrolled pads should inherit the Z of their anchor
                        if let Some(anchor_bbox) = bbox_tracker.get(&anchor_name) {
                            let surface_z_nm = anchor_bbox.min.z;
                            let copper_thickness = stackup_manager.outer_copper_thickness_nm(mount_side);

                            let (p_min, p_max) = match mount_side {
                                hwc_parser::MountingSide::Top => (surface_z_nm - copper_thickness, surface_z_nm),
                                hwc_parser::MountingSide::Bottom => (surface_z_nm, surface_z_nm + copper_thickness),
                                hwc_parser::MountingSide::Embedded => (surface_z_nm - copper_thickness / 2, surface_z_nm + copper_thickness / 2),
                            };

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
                        }

                        let mut world_origin = origin;
                        world_origin.xy = OriginXY::BL;
                        world_origin.z = OriginZ::Bottom;

                        let temp_manager = StackupManager::new(None, symbol_table, space.voxel_size.z_nm, world_origin.z)
                            .expect("Failed to create temp StackupManager");
                        
                        place_pour(space, &unrolled_pour, world_origin, symbol_table, bbox_tracker, eval_context, collector, &temp_manager, profile)?;
                    }
                }
            }
        }
    }
    Ok(())
}
