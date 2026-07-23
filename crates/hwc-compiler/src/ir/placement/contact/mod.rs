mod helpers;
mod netlist_ops;
mod place_drilled;
mod place_simple;
mod resolve;

use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::{HardwareSpace, Point3D};

use helpers::*;
use netlist_ops::*;
use resolve::*;

pub fn place_contact(
    space: &mut HardwareSpace,
    contact: &hwc_parser::ContactPlacement,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    let material_id = space
        .material_registry
        .get_id(&contact.material)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: contact.material.clone(),
        })?;

    let (_x_expr, _y_expr) = match &contact.position {
        hwc_parser::Coordinate::Positional { x, y, .. }
        | hwc_parser::Coordinate::Declarative { x, y, .. } => (x, y),
        hwc_parser::Coordinate::Relative(_) => {
            return Err(IrError::PlacementConstraint {
                message: "Relative coordinates are not supported for contact placement".into(),
                component: contact
                    .name
                    .as_ref()
                    .map(|n| n.as_str().to_string())
                    .unwrap_or_else(|| "unnamed".to_string()),
            });
        }
    };

    let ctx = crate::ir::conversions::CoordinateContext {
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,
        bbox_tracker: None,
        stackup_manager,
        profile,
    };
    let xy_point =
        crate::ir::conversions::coordinate_to_point(&contact.position, &ctx).map_err(|e| {
            IrError::CoordinateResolutionFailed {
                coordinate_str: "contact position".into(),
                reason: e,
            }
        })?;

    let diameter_nm = get_prop_nm(contact, "drill_diameter", symbol_table, eval_context)
        .or_else(|| get_prop_nm(contact, "diameter", symbol_table, eval_context))
        .or_else(|| {
            profile
                .and_then(|p| p.via.as_ref())
                .and_then(|v| v.default_diameter.as_ref())
                .and_then(|d| crate::ir::conversions::measurement_to_nm(d, symbol_table, eval_context).ok())
        })
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: format!(
                "Contact '{}' has no explicit diameter and no profile default via diameter.",
                contact.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| "unnamed".into())
            ),
            hint: "Add 'diameter: <value>' to the contact, or declare 'via: default_diameter: <value>' in the profile.".into(),
        })?;
    let radius_nm = diameter_nm / 2;

    let from_bottom_nm = stackup_manager.resolve_elevation_bottom(
        &contact.from_elevation,
        symbol_table,
        eval_context,
        space.resolution_nm,
    )?;
    let to_bottom_nm = stackup_manager.resolve_elevation_bottom(
        &contact.to_elevation,
        symbol_table,
        eval_context,
        space.resolution_nm,
    )?;
    let from_top_nm =
        stackup_manager.resolve_elevation_top(&contact.from_elevation, symbol_table, eval_context)?;
    let to_top_nm = stackup_manager.resolve_elevation_top(&contact.to_elevation, symbol_table, eval_context)?;

    let contact_name_debug = contact
        .name
        .as_ref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "<unnamed>".into());
    println!("[PLACE_CONTACT] '{}' material='{}' dia={}nm from_z={}nm to_z={}nm from_top={}nm to_top={}nm",
        contact_name_debug, contact.material, diameter_nm,
        from_bottom_nm, to_bottom_nm, from_top_nm, to_top_nm);

    let (start_z, end_z) = resolve_z_span(
        stackup_manager,
        contact,
        from_bottom_nm,
        from_top_nm,
        to_bottom_nm,
        to_top_nm,
    );

    println!(
        "[PLACE_CONTACT] '{}' final span: start_z={}nm end_z={}nm ({}nm tall)",
        contact_name_debug,
        start_z,
        end_z,
        end_z - start_z
    );

    let start_point = Point3D::new(xy_point.x - radius_nm, xy_point.y - radius_nm, start_z);
    let end_point = Point3D::new(xy_point.x + radius_nm, xy_point.y + radius_nm, end_z);

    let contact_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(
            xy_point.x - radius_nm,
            xy_point.y - radius_nm,
            start_z.min(end_z),
        ),
        Point3D::new(
            xy_point.x + radius_nm,
            xy_point.y + radius_nm,
            start_z.max(end_z),
        ),
    );

    check_material_collisions(space, contact, &contact_bbox, from_bottom_nm, to_bottom_nm)?;

    let net_id = resolve_net_id(space, contact)?;

    let bridge_material_name = get_prop_string(contact, "bridge", eval_context);
    let bridge_material_id = if let Some(b) = &bridge_material_name {
        Some(
            space
                .material_registry
                .get_id(b)
                .ok_or_else(|| IrError::UndeclaredMaterial {
                    material: b.clone(),
                })?,
        )
    } else {
        None
    };

    let contour = resolve_shape(contact, eval_context, symbol_table, diameter_nm);

    let annular_ring_nm = resolve_annular_ring(space, contact, symbol_table, eval_context)?;

    let board_max_z_nm = space.dimensions.depth_nm;
    let via_net_id = hwc_engine::netlist::NetId::new(net_id);

    let via = hwc_engine::geometry_router::Via::new(hwc_engine::geometry_router::ViaSpec {
        position: (xy_point.x, xy_point.y),
        from_z_nm: start_z,
        to_z_nm: end_z,
        diameter_nm,
        net_id: via_net_id,
        material_id,
        annular_ring_nm,
        board_min_z_nm: 0,
        board_max_z_nm,
    });
    space.add_vias(vec![via]);

    let is_tented = get_prop_bool(contact, "is_tented", eval_context).unwrap_or(false);
    let pad_diameter_nm = diameter_nm + (2 * annular_ring_nm);
    let pad_radius_nm = pad_diameter_nm / 2;
    let pad_bbox = hwc_engine::geometry::BoundingBox::new(
        Point3D::new(
            xy_point.x - pad_radius_nm,
            xy_point.y - pad_radius_nm,
            start_z.min(end_z),
        ),
        Point3D::new(
            xy_point.x + pad_radius_nm,
            xy_point.y + pad_radius_nm,
            start_z.max(end_z),
        ),
    );

    let liner_material_name = get_prop_string(contact, "liner", eval_context);
    let clearance_nm = resolve_clearance(space)?;
    if liner_material_name.is_some() {
        place_simple::place_tsv(
            space,
            &place_simple::SimpleViaCtx {
                contact,
                material_id,
                net_id,
                contact_bbox,
                diameter_nm,
                start_point,
                end_point,
                start_z,
                end_z,
                contour,
                symbol_table,
                eval_context,
                contact_name_debug: contact_name_debug.clone(),
                is_tented,
                clearance_nm,
                resolution_nm: space.resolution_nm,
            },
            bridge_material_id,
        )?;
    } else if let Some(bridge_mat) = bridge_material_id {
        place_simple::place_compound_via(
            space,
            &place_simple::SimpleViaCtx {
                contact,
                material_id,
                net_id,
                contact_bbox,
                diameter_nm,
                start_point,
                end_point,
                start_z,
                end_z,
                contour,
                symbol_table,
                eval_context,
                contact_name_debug: contact_name_debug.clone(),
                is_tented,
                clearance_nm,
                resolution_nm: space.resolution_nm,
            },
            bridge_mat,
        )?;
    } else {
        let mut process = hwc_engine::ManufacturingProcess::Deposited;
        if let Some(material_def) = symbol_table.materials().get(&contact.material) {
            process = match material_def.process {
                hwc_parser::ManufacturingProcess::DrilledPlated => {
                    hwc_engine::ManufacturingProcess::DrilledPlated
                }
                hwc_parser::ManufacturingProcess::Etched => {
                    hwc_engine::ManufacturingProcess::Etched
                }
                hwc_parser::ManufacturingProcess::Deposited => {
                    hwc_engine::ManufacturingProcess::Deposited
                }
            };
        }

        space.material_registry.set_process(material_id, process);

        println!(
            "[PLACE_CONTACT] '{}' process={:?}, net_id={}, material_id={}",
            contact_name_debug, process, net_id, material_id
        );

        if process == hwc_engine::ManufacturingProcess::DrilledPlated {
            place_drilled::place_drilled_via(place_drilled::DrilledViaPlacement {
                space,
                contact,
                material_id,
                contact_bbox,
                diameter_nm,
                net_id,
                contact_name_debug: &contact_name_debug,
                symbol_table,
                eval_context,
                pad_bbox,
                is_tented,
                pad_diameter_nm,
                clearance_nm,
            })?;
        } else if process == hwc_engine::ManufacturingProcess::Etched {
            place_simple::place_etched_via(
                space,
                contact_bbox,
                diameter_nm,
                clearance_nm,
                &contact_name_debug,
            )?;
        } else {
            place_simple::place_deposited_via(
                space,
                &place_simple::SimpleViaCtx {
                    contact,
                    material_id,
                    net_id,
                    contact_bbox,
                    diameter_nm,
                    start_point,
                    end_point,
                    start_z,
                    end_z,
                    contour,
                    symbol_table,
                    eval_context,
                    contact_name_debug: contact_name_debug.clone(),
                    is_tented,
                    clearance_nm,
                    resolution_nm: space.resolution_nm,
                },
            )?;
        }
    }

    register_contact_in_netlist(netlist_ops::NetlistRegistration {
        space,
        contact,
        from_bottom_nm,
        to_bottom_nm,
        diameter_nm,
        material_id,
        xy_point,
        start_z,
        end_z,
        symbol_table,
        eval_context,
    })?;

    store_contact_metadata(netlist_ops::ContactMetadataStorage {
        space,
        contact,
        from_bottom_nm,
        to_bottom_nm,
        diameter_nm,
        pad_bbox,
        is_tented,
        bridge_material_name,
        contact_name_debug: &contact_name_debug,
        symbol_table,
        eval_context,
    });

    Ok(())
}
