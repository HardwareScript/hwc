use super::helpers::{get_prop_nm, get_prop_string};
use crate::ir::errors::IrError;
use hwc_engine::HardwareSpace;



pub(super) fn check_material_collisions(
    space: &HardwareSpace,
    contact: &hwc_parser::ContactPlacement,
    contact_bbox: &hwc_engine::geometry::BoundingBox,
    _from_bottom_nm: i64,
    _to_bottom_nm: i64,
) -> Result<(), IrError> {
    for existing_contact in &space.contacts {
        if let Some(existing_bbox) = &existing_contact.bbox {
            if contact_bbox.intersects(existing_bbox)
                && existing_contact.material_name != contact.material
            {
                let contact_name: compact_str::CompactString = contact.name.base.clone();

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
        let is_asic = space
            .fabrication_constraints
            .as_ref()
            .is_some_and(|c| c.technology.is_asic());
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
                contact.name.as_str()
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
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<Option<clipper2_rust::Path64>, crate::ir::errors::IrError> {
    let mut contour = contact.contour.clone();
    if contour.is_none() {
        let shape_name_from_contact = get_prop_string(contact, "shape", eval_context);
        let shape_name = shape_name_from_contact.or_else(|| {
            profile
                .and_then(|p| p.via.as_ref())
                .and_then(|v| v.shape.as_ref())
                .map(|id| id.name.as_str().into())
        });

        if let Some(shape_name) = shape_name {
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
            } else {
                return Err(crate::ir::errors::IrError::UndeclaredShape {
                    shape: shape_name.clone(),
                });
            }
        }
    }
    Ok(contour)
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
