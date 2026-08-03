use super::helpers::{get_prop_nm, get_prop_string};
use crate::ir::errors::IrError;
use crate::ir::stackup_manager::StackupManager;
use hwc_engine::HardwareSpace;
use hwc_physics::geometry::Point2D;

/// Resolve relational anchor (e.g., Region.center) to absolute coordinates (v0.2.0)
pub(super) fn resolve_relational_anchor(
    anchor: &hwc_parser::RelationalAnchor,
    bbox_tracker: &crate::BoundingBoxTracker,
    contact_name: &hwc_parser::ComponentName,
) -> Result<Point2D, IrError> {
    let region_name = anchor.region_name.as_str();
    
    // Look up the region's bounding box
    let region_bbox = bbox_tracker
        .get(region_name)
        .ok_or_else(|| IrError::PlacementConstraint {
            message: format!(
                "Contact '{}' references unknown region '{}'",
                contact_name.base.as_str(),
                region_name
            ),
            component: contact_name.base.as_str().to_string(),
        })?;
    
    // Calculate anchor point based on region bounding box
    let (x_nm, y_nm) = match anchor.anchor_point {
        hwc_parser::AnchorPoint::Center => {
            let center_x = (region_bbox.min.x + region_bbox.max.x) / 2;
            let center_y = (region_bbox.min.y + region_bbox.max.y) / 2;
            (center_x, center_y)
        }
        hwc_parser::AnchorPoint::BottomLeft => (region_bbox.min.x, region_bbox.min.y),
        hwc_parser::AnchorPoint::BottomRight => (region_bbox.max.x, region_bbox.min.y),
        hwc_parser::AnchorPoint::TopLeft => (region_bbox.min.x, region_bbox.max.y),
        hwc_parser::AnchorPoint::TopRight => (region_bbox.max.x, region_bbox.max.y),
        hwc_parser::AnchorPoint::CenterLeft => {
            let center_y = (region_bbox.min.y + region_bbox.max.y) / 2;
            (region_bbox.min.x, center_y)
        }
        hwc_parser::AnchorPoint::CenterRight => {
            let center_y = (region_bbox.min.y + region_bbox.max.y) / 2;
            (region_bbox.max.x, center_y)
        }
        hwc_parser::AnchorPoint::TopCenter => {
            let center_x = (region_bbox.min.x + region_bbox.max.x) / 2;
            (center_x, region_bbox.max.y)
        }
        hwc_parser::AnchorPoint::BottomCenter => {
            let center_x = (region_bbox.min.x + region_bbox.max.x) / 2;
            (center_x, region_bbox.min.y)
        }
    };
    
    println!(
        "[RELATIONAL_ANCHOR] Resolved '{}.{:?}' to ({}, {}) for contact '{}'",
        region_name, anchor.anchor_point, x_nm, y_nm, contact_name.base.as_str()
    );
    
    Ok(Point2D::new(x_nm, y_nm))
}

pub(super) fn resolve_z_span(
    stackup_manager: &StackupManager,
    contact: &hwc_parser::ContactPlacement,
    from_bottom_nm: i64,
    from_top_nm: i64,
    to_bottom_nm: i64,
    to_top_nm: i64,
    contact_depth_nm: i64,
) -> (i64, i64) {
    if let (Some(from_name), Some(to_name)) = (
        stackup_manager.get_layer_name(&contact.from_elevation),
        stackup_manager.get_layer_name(&contact.to_elevation),
    ) {
        let (_lower_name, _lower_bottom, lower_top, _upper_name, upper_bottom, _upper_top) =
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

        // ASIC Via Manufacturing Standard (v0.2.0):
        // Vias penetrate into both source and destination conductive layers to ensure
        // reliable electrical contact per standard design rules (e.g., SCMOS min 1λ enclosure).
        //
        // The contact_depth parameter (from profile via.contact_depth) specifies how deep
        // the via extends into each layer beyond the dielectric interface.
        //
        // Example: contact_depth=50nm means via extends from (lower_top - 50nm) to (upper_bottom + 50nm),
        // penetrating 50nm into the lower layer and 50nm into the upper layer.
        let via_bottom = lower_top - contact_depth_nm;
        let via_top = upper_bottom + contact_depth_nm;

        (via_bottom, via_top)
    } else {
        (from_bottom_nm.min(to_bottom_nm), from_top_nm.max(to_top_nm))
    }
}

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
                let contact_name: compact_str::CompactString = contact
                    .name
                    .base
                    .clone();

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
            c.technology.is_asic()
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
            profile.and_then(|p| p.via.as_ref()).and_then(|v| v.shape.as_ref()).map(|id| id.name.as_str().into())
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
