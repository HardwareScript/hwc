use super::helpers::{get_prop_nm, get_prop_string};
use crate::ir::errors::IrError;
use crate::SymbolTable;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::{HardwareSpace, Point3D};
use hwc_parser::{ContactPlacement, EvaluationContext};

/// Shared inputs for the simple via placement helpers.
///
/// Bundles the contact-derived geometry and resolution context that the
/// `place_*` helpers in this module all consume, so each helper takes a single
/// context argument instead of a long parameter list.
pub(super) struct SimpleViaCtx<'a> {
    pub contact: &'a ContactPlacement,
    pub material_id: u8,
    pub net_id: u32,
    pub contact_bbox: BoundingBox,
    pub diameter_nm: i64,
    pub start_point: Point3D,
    pub end_point: Point3D,
    pub start_z: i64,
    pub end_z: i64,
    pub contour: Option<clipper2_rust::Path64>,
    pub symbol_table: &'a SymbolTable,
    pub eval_context: &'a EvaluationContext,
    pub is_tented: bool,
    pub clearance_nm: i64,
    pub resolution_nm: i64,
}

pub(super) fn place_tsv(
    space: &mut HardwareSpace,
    ctx: &SimpleViaCtx,
    bridge_material_id: Option<u8>,
) -> Result<(), IrError> {
    let contact = ctx.contact;
    let symbol_table = ctx.symbol_table;
    let eval_context = ctx.eval_context;
    let liner_material_name = get_prop_string(contact, "liner", eval_context);
    if let Some(liner_material_name) = &liner_material_name {
        let liner_material_id = space
            .material_registry
            .get_id(liner_material_name)
            .ok_or_else(|| IrError::UndeclaredMaterial {
                material: liner_material_name.clone(),
            })?;
        let liner_thickness_nm =
            get_prop_nm(contact, "liner_thickness", symbol_table, eval_context).unwrap_or(5_000);

        let bridge_thickness_nm = if bridge_material_id.is_some() {
            1_000
        } else {
            0
        };

        let koz_multiplier = if let Some(expr) = contact.properties.get("koz") {
            expr.evaluate(eval_context)
                .and_then(|v| v.as_number())
                .unwrap_or(3.0) as f32
        } else {
            3.0
        };

        let stack = hwc_engine::geometry_router::entity_graph::LinerStack::new(
            liner_material_id,
            liner_thickness_nm,
            bridge_material_id,
            bridge_thickness_nm,
            ctx.material_id,
        );

        let _params = hwc_engine::geometry_router::entity_graph::TSVParams {
            diameter_nm: ctx.diameter_nm,
            stack,
            koz_multiplier,
        };

        let circle_segments = space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.circle_segments)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: "PDK profile is missing 'manufacturing.circle_segments'".into(),
                hint: "Add 'circle_segments: <n>' to the manufacturing block of your profile."
                    .into(),
            })?;

        space.entity_graph.add_tsv_stack(
            ctx.material_id,
            hwc_engine::NetId::new(ctx.net_id),
            ctx.contact_bbox,
            ctx.diameter_nm as u32,
            (ctx.diameter_nm / 2) as u32,
            circle_segments,
        );
    }
    Ok(())
}

pub(super) fn place_compound_via(
    space: &mut HardwareSpace,
    ctx: &SimpleViaCtx,
    bridge_material_id: u8,
) -> Result<(), IrError> {
    let interface_end_z = ctx.start_z + ctx.resolution_nm;

    let _interface_end_point = Point3D::new(ctx.end_point.x, ctx.end_point.y, interface_end_z);
    let _fill_start_point = Point3D::new(ctx.start_point.x, ctx.start_point.y, interface_end_z);

    // Transform contour from local space to world space if present
    let cx = (ctx.contact_bbox.min.x + ctx.contact_bbox.max.x) / 2;
    let cy = (ctx.contact_bbox.min.y + ctx.contact_bbox.max.y) / 2;

    if let Some(ref contour) = ctx.contour {
        let points = contour
            .iter()
            .map(|p| hwc_engine::geometry::Point2D::new(p.x + cx, p.y + cy))
            .collect();
        let polygon = hwc_engine::geometry::Polygon::new(points);
        space.entity_graph.add_polygon_substrate_layer(
            bridge_material_id,
            hwc_engine::NetId::new(ctx.net_id),
            ctx.contact_bbox,
            polygon,
        );
    } else {
        space.entity_graph.add_cylinder_substrate_layer(
            bridge_material_id,
            hwc_engine::NetId::new(ctx.net_id),
            ctx.contact_bbox,
            ctx.diameter_nm,
            32,
            0,
        );
    }

    if interface_end_z < ctx.end_z {
        if let Some(ref contour) = ctx.contour {
            let points = contour
                .iter()
                .map(|p| hwc_engine::geometry::Point2D::new(p.x + cx, p.y + cy))
                .collect();
            let polygon = hwc_engine::geometry::Polygon::new(points);
            space.entity_graph.add_polygon_substrate_layer(
                ctx.material_id,
                hwc_engine::NetId::new(ctx.net_id),
                ctx.contact_bbox,
                polygon,
            );
        } else {
            space.entity_graph.add_cylinder_substrate_layer(
                ctx.material_id,
                hwc_engine::NetId::new(ctx.net_id),
                ctx.contact_bbox,
                ctx.diameter_nm,
                32,
                0,
            );
        }
    }
    Ok(())
}

pub(super) fn place_etched_via(
    space: &mut HardwareSpace,
    contact_bbox: hwc_engine::geometry::BoundingBox,
    diameter_nm: i64,
    clearance_nm: i64,
) -> Result<(), IrError> {
    space
        .entity_graph
        .drill_via_hole(hwc_engine::geometry_router::entity_graph::ViaHoleSpec {
            hole_bbox: contact_bbox,
            diameter_nm,
            via_net: hwc_engine::NetId::UNCONNECTED,
            clearance_nm,
            is_tented: true,
            pad_diameter_nm: diameter_nm,
        });
    Ok(())
}

pub(super) fn place_deposited_via(
    space: &mut HardwareSpace,
    ctx: &SimpleViaCtx,
) -> Result<(), IrError> {
    // println!(
    //     "[PLACE_CONTACT] '{}' Deposited path: drilling via hole at bbox=({},{}-{},{}) dia={}",
    //     contact_name_debug,
    //     ctx.contact_bbox.min.x,
    //     ctx.contact_bbox.min.y,
    //     ctx.contact_bbox.max.x,
    //     ctx.contact_bbox.max.y,
    //     ctx.diameter_nm
    // );
    space
        .entity_graph
        .drill_via_hole(hwc_engine::geometry_router::entity_graph::ViaHoleSpec {
            hole_bbox: ctx.contact_bbox,
            diameter_nm: ctx.diameter_nm,
            via_net: hwc_engine::NetId::new(ctx.net_id),
            clearance_nm: ctx.clearance_nm,
            is_tented: ctx.is_tented,
            pad_diameter_nm: ctx.diameter_nm,
        });

    if let Some(path) = &ctx.contour {
        // println!("[PLACE_CONTACT] '{}' Placing polygon via: mat={}, net={}, start=({},{},{}) end=({},{},{}) dia={}",
        //     contact_name_debug, contact.material, ctx.net_id,
        //     ctx.start_point.x, ctx.start_point.y, ctx.start_point.z,
        //     ctx.end_point.x, ctx.end_point.y, ctx.end_point.z, ctx.diameter_nm);

        // DEBUG: Show original polygon points from shape definition
//         println!(
//             "[PLACE_CONTACT_DEBUG] '{}' Original polygon points (local space):",
//             contact_name_debug
//         );
        // for (i, p) in path.iter().enumerate() {
        //     println!("  Point {}: ({}, {})", i, p.x, p.y);
        // }

        // Transform polygon points from local space to world space
        // The shape definition generates points centered at (0,0), but the via is placed at xy_point
        let cx = (ctx.contact_bbox.min.x + ctx.contact_bbox.max.x) / 2;
        let cy = (ctx.contact_bbox.min.y + ctx.contact_bbox.max.y) / 2;

//         println!(
//             "[PLACE_CONTACT_DEBUG] '{}' Translating by center: ({}, {})",
//             contact_name_debug, cx, cy
//         );

        let world_space_points: Vec<hwc_engine::geometry::Point2D> = path
            .iter()
            .map(|p| hwc_engine::geometry::Point2D::new(p.x + cx, p.y + cy))
            .collect();

//         println!(
//             "[PLACE_CONTACT_DEBUG] '{}' Transformed polygon points (world space):",
//             contact_name_debug
//         );
        // for (i, p) in world_space_points.iter().enumerate() {
        //     println!("  Point {}: ({}, {})", i, p.x, p.y);
        // }

        let polygon = hwc_engine::geometry::Polygon::new(world_space_points);

        space.entity_graph.add_polygon_substrate_layer(
            ctx.material_id,
            hwc_engine::NetId::new(ctx.net_id),
            ctx.contact_bbox,
            polygon,
        );
    } else {
        // println!("[PLACE_CONTACT] '{}' Placing cylinder via: mat={}, net={}, start=({},{},{}) end=({},{},{}) dia={}",
        //     contact_name_debug, contact.material, ctx.net_id,
        //     ctx.start_point.x, ctx.start_point.y, ctx.start_point.z,
        //     ctx.end_point.x, ctx.end_point.y, ctx.end_point.z, ctx.diameter_nm);
        space.entity_graph.add_cylinder_substrate_layer(
            ctx.material_id,
            hwc_engine::NetId::new(ctx.net_id),
            ctx.contact_bbox,
            ctx.diameter_nm,
            32,
            0,
        );
    }
    Ok(())
}
