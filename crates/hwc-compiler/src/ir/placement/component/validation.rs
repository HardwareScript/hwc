use super::super::super::errors::{
    ComponentBuriedInSubstrateDetails, ComponentFloatingInAirDetails, IrError,
    SubstrateOverlapDetails,
};
use super::super::context::{ComponentPlacementData, PlacementContext, ValidationParams};
use super::super::helpers::parse_rectangle_dimensions;
use crate::bounding_box_tracker::BoundingBoxTracker;
use hwc_diagnostics::WaiverApplied;
use hwc_engine::{
    geometry::{BoundingBox, Point3D},
    HardwareSpace, KeepOutZone,
};

pub fn validate_and_register(
    space: &mut HardwareSpace,
    pd: &ComponentPlacementData,
    vp: &ValidationParams,
    bbox_tracker: &mut BoundingBoxTracker,
    ctx: &PlacementContext,
) -> Result<(), IrError> {
    if let Ok(component_def) = ctx
        .symbol_table
        .get_component(pd.component.component_type.as_str())
    {
        if let Some(layout) = &component_def.layout {
            if let Some(shape_str) = &layout.shape {
                if let Some(dims) = parse_rectangle_dimensions(shape_str) {
                    let (width_nm, height_nm, depth_nm) = dims;

                    let bbox = if vp.rotation_deg.abs() < 0.001 {
                        BoundingBox::new(
                            Point3D::new(
                                vp.untransformed_origin.x,
                                vp.untransformed_origin.y,
                                vp.untransformed_origin.z,
                            ),
                            Point3D::new(
                                vp.untransformed_origin.x + width_nm,
                                vp.untransformed_origin.y + height_nm,
                                vp.untransformed_origin.z + depth_nm,
                            ),
                        )
                    } else {
                        let center_x = vp.untransformed_origin.x + width_nm / 2;
                        let center_y = vp.untransformed_origin.y + height_nm / 2;
                        let half_w = width_nm / 2;
                        let half_h = height_nm / 2;
                        let corners = [
                            (-half_w, -half_h),
                            (half_w, -half_h),
                            (half_w, half_h),
                            (-half_w, half_h),
                        ];
                        let angle_rad = vp.rotation_deg.to_radians();
                        let cos_theta = angle_rad.cos();
                        let sin_theta = angle_rad.sin();
                        let mut min_x = i64::MAX;
                        let mut max_x = i64::MIN;
                        let mut min_y = i64::MAX;
                        let mut max_y = i64::MIN;
                        for (cx, cy) in corners.iter() {
                            let rx = (*cx as f64 * cos_theta - *cy as f64 * sin_theta) as i64;
                            let ry = (*cx as f64 * sin_theta + *cy as f64 * cos_theta) as i64;
                            let gx = center_x + rx;
                            let gy = center_y + ry;
                            min_x = min_x.min(gx);
                            max_x = max_x.max(gx);
                            min_y = min_y.min(gy);
                            max_y = max_y.max(gy);
                        }
                        BoundingBox::new(
                            Point3D::new(min_x, min_y, vp.untransformed_origin.z),
                            Point3D::new(max_x, max_y, vp.untransformed_origin.z + depth_nm),
                        )
                    };

                    let skip_substrate_check =
                        pd.component.waivers.merge == hwc_parser::MergeWaiver::All;

                    if let Some(substrate_bbox) = &space.substrate_bbox {
                        let component_min_z = vp.untransformed_origin.z;
                        let component_max_z = vp.untransformed_origin.z + depth_nm;
                        let substrate_min_z = substrate_bbox.min.z;
                        let substrate_max_z = substrate_bbox.max.z;

                        let component_z_layer = (component_min_z / space.voxel_size.z_nm) as usize;
                        let substrate_min_layer =
                            (substrate_min_z / space.voxel_size.z_nm) as usize;
                        let substrate_max_layer =
                            (substrate_max_z / space.voxel_size.z_nm) as usize;

                        let source = ctx.collector.source.as_str();
                        let original_line = source
                            .get(pd.component.span.start..pd.component.span.end)
                            .unwrap_or("add ...");

                        let group_context = if let Some(n) = &pd.component.name {
                            let name_str = n.base.as_str();
                            if let Some(idx) = name_str.find('[') {
                                &name_str[..idx]
                            } else {
                                name_str.trim_end_matches(|c: char| c.is_ascii_digit())
                            }
                        } else {
                            pd.component.component_type.as_str()
                        };

                        if component_min_z > substrate_max_z {
                            let gap_nm = component_min_z - substrate_max_z;
                            let gap_mm = gap_nm as f64 / 1_000_000.0;

                            if !pd.component.waivers.floating {
                                let suggestion = format!(
                                    "To fix:\n- Place component at z:{substrate_max_layer} (substrate surface)\n- Corrected: {}",
                                    original_line.replace(&format!("z: {}", component_z_layer), &format!("z: {}", substrate_max_layer))
                                );

                                let ir_x_mm = vp.untransformed_origin.x as f64 / 1_000_000.0;
                                let ir_y_mm = vp.untransformed_origin.y as f64 / 1_000_000.0;
                                let ir_z_mm = vp.untransformed_origin.z as f64 / 1_000_000.0;
                                ctx.collector
                                    .report(IrError::ComponentFloatingInAir(Box::new(
                                        ComponentFloatingInAirDetails {
                                            component: pd.name.clone().into(),
                                            component_z_layer,
                                            component_z_mm: component_min_z as f64 / 1_000_000.0,
                                            substrate_max_layer,
                                            substrate_max_mm: substrate_max_z as f64 / 1_000_000.0,
                                            gap_mm,
                                            x_mm: ir_x_mm,
                                            y_mm: ir_y_mm,
                                            z_mm: ir_z_mm,
                                            span: (
                                                pd.component.span.start,
                                                pd.component.span.end - pd.component.span.start,
                                            )
                                                .into(),
                                            suggestion,
                                        },
                                    )));
                                ctx.collector.report_violation(
                                    "P44",
                                    "floating in air above substrate",
                                    group_context,
                                );
                                return Ok(());
                            } else {
                                ctx.collector.report(WaiverApplied::new(&format!(
                                    "Component '{}' allowed to float in air",
                                    pd.name
                                )));
                            }
                        }

                        if component_max_z < substrate_min_z {
                            if !pd.component.waivers.floating {
                                let gap_nm = substrate_min_z - component_max_z;
                                let gap_mm = gap_nm as f64 / 1_000_000.0;

                                let suggestion = format!(
                                    "To fix:\n- Place component at z:{substrate_max_layer} or higher (above substrate base)\n- Corrected: {}",
                                    original_line.replace(&format!("z: {}", component_z_layer), &format!("z: {}", substrate_max_layer))
                                );

                                let ir_x_mm = vp.untransformed_origin.x as f64 / 1_000_000.0;
                                let ir_y_mm = vp.untransformed_origin.y as f64 / 1_000_000.0;
                                let ir_z_mm = vp.untransformed_origin.z as f64 / 1_000_000.0;
                                ctx.collector
                                    .report(IrError::ComponentBuriedInSubstrate(Box::new(
                                        ComponentBuriedInSubstrateDetails {
                                            component: pd.name.clone().into(),
                                            component_z_layer,
                                            component_z_mm: component_min_z as f64 / 1_000_000.0,
                                            substrate_min_layer,
                                            substrate_min_mm: substrate_min_z as f64 / 1_000_000.0,
                                            substrate_max_layer,
                                            substrate_max_mm: substrate_max_z as f64 / 1_000_000.0,
                                            gap_mm,
                                            x_mm: ir_x_mm,
                                            y_mm: ir_y_mm,
                                            z_mm: ir_z_mm,
                                            span: (
                                                pd.component.span.start,
                                                pd.component.span.end - pd.component.span.start,
                                            )
                                                .into(),
                                            suggestion,
                                        },
                                    )));
                                ctx.collector.report_violation(
                                    "P44",
                                    "buried below substrate base",
                                    group_context,
                                );
                                return Ok(());
                            } else {
                                ctx.collector.report(WaiverApplied::new(&format!(
                                    "Component '{}' allowed to be below substrate",
                                    pd.name
                                )));
                            }
                        }

                        if component_min_z < substrate_max_z && component_max_z > substrate_min_z {
                            if skip_substrate_check {
                                ctx.collector.report(WaiverApplied::new(&format!(
                                    "Component '{}' allowed to overlap substrate",
                                    pd.name
                                )));
                            } else {
                                let is_substrate_insulator = space
                                    .material_registry
                                    .is_insulator(space.substrate_material_id)
                                    || space
                                        .material_registry
                                        .is_semiconductor(space.substrate_material_id);

                                if is_substrate_insulator {
                                    println!(
                                        "   ├─ Resolved top/bottom boundary handshake for pad '{}'",
                                        pd.name
                                    );
                                } else {
                                    let suggested_z_layer = substrate_max_layer + 1;
                                    let suggestion = format!(
                                        "To fix:\n- Place component at z:{suggested_z_layer} or higher (above substrate)\n- Corrected: {}\n\nAdvanced: Use 'merge: true' waiver if this is intentional.",
                                        original_line.replace(&format!("z: {}", component_z_layer), &format!("z: {}", suggested_z_layer))
                                    );

                                    let ir_x_mm = vp.untransformed_origin.x as f64 / 1_000_000.0;
                                    let ir_y_mm = vp.untransformed_origin.y as f64 / 1_000_000.0;
                                    let ir_z_mm = vp.untransformed_origin.z as f64 / 1_000_000.0;
                                    ctx.collector.report(IrError::SubstrateOverlap(Box::new(
                                        SubstrateOverlapDetails {
                                            component: pd.name.clone().into(),
                                            component_z_layer,
                                            component_z_mm: component_min_z as f64 / 1_000_000.0,
                                            substrate_min_layer,
                                            substrate_max_layer,
                                            substrate_min_mm: substrate_min_z as f64 / 1_000_000.0,
                                            substrate_max_mm: substrate_max_z as f64 / 1_000_000.0,
                                            suggested_z_layer,
                                            x_mm: ir_x_mm,
                                            y_mm: ir_y_mm,
                                            z_mm: ir_z_mm,
                                            span: (
                                                pd.component.span.start,
                                                pd.component.span.end - pd.component.span.start,
                                            )
                                                .into(),
                                            suggestion,
                                        },
                                    )));
                                    ctx.collector.report_violation(
                                        "P44",
                                        "overlaps with substrate material",
                                        group_context,
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }

                    bbox_tracker.register(pd.name.clone().into(), bbox, vp.untransformed_origin);

                    let (min_y, max_y) = match ctx.origin.xy {
                        hwc_parser::OriginXY::TL | hwc_parser::OriginXY::TR => {
                            (vp.position.y - height_nm, vp.position.y)
                        }
                        hwc_parser::OriginXY::BL | hwc_parser::OriginXY::BR => {
                            (vp.position.y, vp.position.y + height_nm)
                        }
                    };

                    let (min_x, max_x) = match ctx.origin.xy {
                        hwc_parser::OriginXY::TL | hwc_parser::OriginXY::BL => {
                            (vp.position.x, vp.position.x + width_nm)
                        }
                        hwc_parser::OriginXY::TR | hwc_parser::OriginXY::BR => {
                            (vp.position.x - width_nm, vp.position.x)
                        }
                    };

                    let engine_bbox = if vp.rotation_deg.abs() < 0.001 {
                        BoundingBox::new(
                            Point3D::new(min_x, min_y, vp.body_min_z),
                            Point3D::new(max_x, max_y, vp.body_max_z),
                        )
                    } else {
                        let (center_x, center_y) = match ctx.origin.xy {
                            hwc_parser::OriginXY::TL => {
                                (vp.position.x + width_nm / 2, vp.position.y - height_nm / 2)
                            }
                            hwc_parser::OriginXY::TR => {
                                (vp.position.x - width_nm / 2, vp.position.y - height_nm / 2)
                            }
                            hwc_parser::OriginXY::BL => {
                                (vp.position.x + width_nm / 2, vp.position.y + height_nm / 2)
                            }
                            hwc_parser::OriginXY::BR => {
                                (vp.position.x - width_nm / 2, vp.position.y + height_nm / 2)
                            }
                        };
                        let half_w = width_nm / 2;
                        let half_h = height_nm / 2;
                        let corners = [
                            (-half_w, -half_h),
                            (half_w, -half_h),
                            (half_w, half_h),
                            (-half_w, half_h),
                        ];
                        let angle_rad = vp.rotation_deg.to_radians();
                        let cos_theta = angle_rad.cos();
                        let sin_theta = angle_rad.sin();
                        let mut final_min_x = i64::MAX;
                        let mut final_max_x = i64::MIN;
                        let mut final_min_y = i64::MAX;
                        let mut final_max_y = i64::MIN;
                        for (cx, cy) in corners.iter() {
                            let rx = (*cx as f64 * cos_theta - *cy as f64 * sin_theta) as i64;
                            let ry = (*cx as f64 * sin_theta + *cy as f64 * cos_theta) as i64;
                            let gx = center_x + rx;
                            let gy = match ctx.origin.xy {
                                hwc_parser::OriginXY::TL | hwc_parser::OriginXY::TR => {
                                    center_y - ry
                                }
                                hwc_parser::OriginXY::BL | hwc_parser::OriginXY::BR => {
                                    center_y + ry
                                }
                            };
                            final_min_x = final_min_x.min(gx);
                            final_max_x = final_max_x.max(gx);
                            final_min_y = final_min_y.min(gy);
                            final_max_y = final_max_y.max(gy);
                        }
                        BoundingBox::new(
                            Point3D::new(final_min_x, final_min_y, vp.body_min_z),
                            Point3D::new(final_max_x, final_max_y, vp.body_max_z),
                        )
                    };

                    let material_id = space.material_registry.get_id("Component").ok_or_else(|| {
                        IrError::UndeclaredMaterial { material: "Component".into() }
                    })?;
                    space.register_component_bbox(
                        pd.name.clone().into(),
                        engine_bbox,
                        material_id,
                        pd.component.component_type.name.clone(),
                        smallvec::SmallVec::new(),
                    );

                    let mut exempted_nets = Vec::new();
                    for binding in pd.component.pin_net_bindings.values() {
                        match binding {
                            hwc_parser::NetBinding::Simple(net_name) => {
                                exempted_nets.push(net_name.clone());
                            }
                            hwc_parser::NetBinding::Conditional {
                                then_net, else_net, ..
                            } => {
                                exempted_nets.push(then_net.clone());
                                exempted_nets.push(else_net.clone());
                            }
                        }
                    }

                    space.register_keep_out_zone(KeepOutZone {
                        bbox: BoundingBox::new(
                            Point3D::new(engine_bbox.min.x, engine_bbox.min.y, 0),
                            Point3D::new(
                                engine_bbox.max.x,
                                engine_bbox.max.y,
                                space.dimensions.depth_nm,
                            ),
                        ),
                        net_id: None,
                        allow_vias: false,
                        allow_routing: true,
                        exempted_nets,
                    });
                }
            }
        }
    }
    Ok(())
}
