use super::helpers::{get_prop_bool, get_prop_cap_type, get_prop_nm, get_prop_string};
use crate::ir::errors::IrError;
use crate::SymbolTable;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::HardwareSpace;
use hwc_parser::{ContactPlacement, EvaluationContext};

pub(super) struct DrilledViaPlacement<'a> {
    pub space: &'a mut HardwareSpace,
    pub contact: &'a ContactPlacement,
    pub material_id: u8,
    pub contact_bbox: BoundingBox,
    pub diameter_nm: i64,
    pub net_id: u32,
    pub contact_name_debug: &'a str,
    pub symbol_table: &'a SymbolTable,
    pub eval_context: &'a EvaluationContext,
    pub pad_bbox: BoundingBox,
    pub is_tented: bool,
    pub pad_diameter_nm: i64,
    pub clearance_nm: i64,
}

pub(super) fn place_drilled_via(args: DrilledViaPlacement) -> Result<(), IrError> {
    let DrilledViaPlacement {
        space,
        contact,
        material_id,
        contact_bbox,
        diameter_nm,
        net_id,
        contact_name_debug,
        symbol_table,
        eval_context,
        pad_bbox,
        is_tented,
        pad_diameter_nm,
        clearance_nm,
    } = args;

    space
        .entity_graph
        .drill_via_hole(hwc_engine::geometry_router::entity_graph::ViaHoleSpec {
            hole_bbox: contact_bbox,
            diameter_nm,
            via_net: hwc_engine::NetId::new(net_id),
            clearance_nm,
            is_tented,
            pad_diameter_nm,
        });

    let plating_thickness_nm =
        get_prop_nm(contact, "plating_thickness", symbol_table, eval_context).ok_or_else(|| {
            IrError::MissingAsicConstraint {
                message: format!(
                    "Contact '{}' missing required 'plating_thickness' property",
                    contact_name_debug
                ),
                hint: "Add 'plating_thickness: <value>' to the contact properties.".into(),
            }
        })?;

    if 2 * plating_thickness_nm > diameter_nm {
        return Err(IrError::PlacementConstraint {
            message: format!(
                "Invalid via dimensions for contact '{}': plating thickness ({}nm) is greater than half of the via diameter ({}nm). \
                 This results in a geometrically impossible negative inner diameter. \
                 Plating thickness must be less than or equal to half of the diameter (max {}nm).",
                contact_name_debug,
                plating_thickness_nm,
                diameter_nm,
                diameter_nm / 2
            ),
            component: contact_name_debug.to_string(),
        });
    }

    let inner_diameter_nm = diameter_nm - (2 * plating_thickness_nm);

    let bottom_diameter_nm = get_prop_nm(contact, "bottom_diameter", symbol_table, eval_context);

    let top_cap = match get_prop_cap_type(contact, "top_cap", eval_context) {
        Some(cap) => cap,
        None => {
            if get_prop_bool(contact, "caps", eval_context)
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: format!("Contact '{}' missing required 'top_cap' or 'caps' property", contact_name_debug),
                    hint: "Add 'top_cap: annular|solid|none' or 'caps: true|false' to the contact properties.".into(),
                })?
            {
                hwc_engine::geometry_router::entity_graph::CapType::Annular
            } else {
                hwc_engine::geometry_router::entity_graph::CapType::None
            }
        }
    };

    let bottom_cap = match get_prop_cap_type(contact, "bottom_cap", eval_context) {
        Some(cap) => cap,
        None => {
            if get_prop_bool(contact, "caps", eval_context)
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: format!("Contact '{}' missing required 'bottom_cap' or 'caps' property", contact_name_debug),
                    hint: "Add 'bottom_cap: annular|solid|none' or 'caps: true|false' to the contact properties.".into(),
                })?
            {
                hwc_engine::geometry_router::entity_graph::CapType::Annular
            } else {
                hwc_engine::geometry_router::entity_graph::CapType::None
            }
        }
    };

    println!("[PLACE_CONTACT] '{}' Adding tube substrate: pad_bbox=({},{}-{},{}), outer_dia={}, inner_dia={}, pad_dia={}, top_cap={:?}, bottom_cap={:?}",
        contact_name_debug,
        contact_bbox.min.x, contact_bbox.min.y, contact_bbox.max.x, contact_bbox.max.y,
        diameter_nm, inner_diameter_nm, pad_diameter_nm, top_cap, bottom_cap);
    let circle_segments = super::helpers::resolve_circle_segments(space)?;

    space.entity_graph.add_tube_substrate_layer(
        hwc_engine::geometry_router::entity_graph::TubeLayerSpec::builder(
            material_id,
            hwc_engine::NetId::new(net_id),
            pad_bbox,
            circle_segments,
        )
        .outer_diameter(diameter_nm as u32)
        .inner_diameter(inner_diameter_nm as u32)
        .pad_diameter(pad_diameter_nm as u32)
        .top_cap(top_cap)
        .bottom_cap(bottom_cap)
        .bottom_outer_diameter(bottom_diameter_nm.map(|d| d as u32))
        .build(),
    );

    if get_prop_bool(contact, "filled", eval_context).ok_or_else(|| {
        IrError::MissingAsicConstraint {
            message: format!(
                "Contact '{}' missing required 'filled' property",
                contact_name_debug
            ),
            hint: "Add 'filled: true|false' to the contact properties.".into(),
        }
    })? {
        let fill_material_name = get_prop_string(contact, "fill_material", eval_context);
        let fill_mat_str =
            fill_material_name
                .as_deref()
                .ok_or_else(|| IrError::MissingAsicConstraint {
                    message: format!(
                        "Contact '{}' is filled but missing 'fill_material' property",
                        contact_name_debug
                    ),
                    hint: "Add 'fill_material: <MaterialName>' to the contact properties.".into(),
                })?;
        let fill_material_id = space
            .material_registry
            .get_id(fill_mat_str)
            .ok_or_else(|| IrError::UndeclaredMaterial {
                material: fill_mat_str.into(),
            })?;

        let fill_net_id = if let Some(fill_mat_name) = &fill_material_name {
            if let Some(mat_def) = symbol_table.materials().get(fill_mat_name) {
                if mat_def.category.is_conductive() {
                    net_id
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };

        space.entity_graph.add_cylinder_substrate_layer(
            fill_material_id,
            hwc_engine::NetId::new(fill_net_id),
            contact_bbox,
            inner_diameter_nm,
            16,
            0,
        );
    }

    Ok(())
}
