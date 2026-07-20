use super::helpers::{get_prop_nm, get_prop_string};
use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::HardwareSpace;

pub(super) fn resolve_z_span(
    stackup_manager: &StackupManager,
    contact: &hwc_parser::ContactPlacement,
    from_bottom_nm: i64,
    from_top_nm: i64,
    to_bottom_nm: i64,
    to_top_nm: i64,
) -> (i64, i64) {
    if let (Some(from_name), Some(to_name)) = (
        stackup_manager.get_layer_name(&contact.from_elevation),
        stackup_manager.get_layer_name(&contact.to_elevation),
    ) {
        let (_lower_name, lower_bottom, _lower_top, _upper_name, _upper_bottom, upper_top) =
            if from_bottom_nm < to_bottom_nm {
                (
                    from_name,
                    from_bottom_nm,
                    from_top_nm,
                    to_name,
                    to_bottom_nm,
                    to_top_nm,
                )
            } else {
                (
                    to_name,
                    to_bottom_nm,
                    to_top_nm,
                    from_name,
                    from_bottom_nm,
                    from_top_nm,
                )
            };

        let via_bottom = lower_bottom;
        let via_top = upper_top;

        (via_bottom, via_top)
    } else {
        (from_bottom_nm.min(to_bottom_nm), from_top_nm.max(to_top_nm))
    }
}

pub(super) fn check_material_collisions(
    space: &HardwareSpace,
    contact: &hwc_parser::ContactPlacement,
    contact_bbox: &hwc_engine::geometry::BoundingBox,
    from_bottom_nm: i64,
    to_bottom_nm: i64,
) -> Result<(), IrError> {
    for existing_contact in &space.contacts {
        if let Some(existing_bbox) = &existing_contact.bbox {
            if contact_bbox.intersects(existing_bbox)
                && existing_contact.material_name != contact.material
            {
                let contact_name: compact_str::CompactString = contact
                    .name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("Via_{}_{}", from_bottom_nm, to_bottom_nm).into());

                return Err(IrError::PlacementConstraint {
                    message: format!(
                        "Material interpenetration detected: Contact '{}' ({}) overlaps with contact '{}' ({}) in 3D space. \
                         Different materials cannot occupy the same volume.",
                        contact_name,
                        contact.material,
                        existing_contact.name,
                        existing_contact.material_name
                    ),
                    component: contact_name.to_string(),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_net_id(
    space: &mut HardwareSpace,
    contact: &hwc_parser::ContactPlacement,
) -> Result<u32, IrError> {
    if let Some(net_name) = &contact.net {
        let is_asic = space.fabrication_constraints.as_ref().is_some_and(|c| {
            c.technology
                .as_ref()
                .is_some_and(|t| t.to_lowercase() == "asic")
        });
        let min_width = space
            .fabrication_constraints
            .as_ref()
            .map(|c| c.trace.min_width_nm)
            .ok_or_else(|| IrError::MissingAsicConstraint {
                message: format!("Net '{}' requires fabrication constraints but none are loaded.", net_name),
                hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
            })?;

        Ok(space
            .netlist
            .get_or_create_net_with_technology(net_name.base.as_str(), is_asic, min_width)
            .raw())
    } else {
        Ok(0)
    }
}

pub(super) fn resolve_annular_ring(
    space: &HardwareSpace,
    contact: &hwc_parser::ContactPlacement,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<i64, IrError> {
    if let Some(nm) = get_prop_nm(contact, "annular_ring", symbol_table, eval_context) {
        Ok(nm)
    } else if let Some(profile_ring) = space
        .fabrication_constraints
        .as_ref()
        .map(|c| c.via.min_annular_ring_nm)
    {
        Ok(profile_ring)
    } else {
        Err(IrError::MissingAsicConstraint {
            message: format!(
                "Contact '{}' has no explicit annular_ring and no profile via.min_annular_ring.",
                contact.name.as_ref().map(|n| n.to_string()).unwrap_or_else(|| "unnamed".into())
            ),
            hint: "Add 'annular_ring: <value>' to the contact, or declare 'via: min_annular_ring: <value>' in the profile.".into(),
        })
    }
}

pub(super) fn resolve_shape(
    contact: &hwc_parser::ContactPlacement,
    eval_context: &hwc_parser::EvaluationContext,
    symbol_table: &crate::SymbolTable,
    diameter_nm: i64,
) -> Option<clipper2_rust::Path64> {
    let mut contour = contact.contour.clone();
    if contour.is_none() {
        if let Some(shape_name) = get_prop_string(contact, "shape", eval_context) {
            if let Some(shape_def) = symbol_table.get_shape(shape_name.as_str()) {
                let constants = symbol_table.get_all_constants();
                contour = Some(crate::via_resolver::library::evaluate_shape_points(
                    shape_def,
                    diameter_nm,
                    &constants,
                ));
                let contour_len = contour.as_ref().map_or(0, |c| c.len());
                println!(
                    "[PLACE_CONTACT] Resolved shape '{}' to {} vertices",
                    shape_name, contour_len
                );
            }
        }
    }
    contour
}

pub(super) fn resolve_clearance(space: &HardwareSpace) -> Result<i64, IrError> {
    space
        .fabrication_constraints
        .as_ref()
        .map(|c| c.trace.min_spacing_nm)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "PDK missing required 'trace.min_spacing_nm' constraint".into(),
            hint: "Add a 'trace:' block to your profile with explicit min_spacing.\n\nExample:\n  trace:\n    min_spacing: 200nm".into(),
        })
}

pub(super) fn resolve_solder_mask_expansion(space: &HardwareSpace) -> Result<i64, IrError> {
    space
        .fabrication_constraints
        .as_ref()
        .and_then(|c| c.solder_mask_expansion_nm)
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "PDK missing required 'solder_mask_expansion_nm' constraint".into(),
            hint: "Add 'solder_mask_expansion_nm: <value>' to your profile manufacturing block."
                .into(),
        })
}
