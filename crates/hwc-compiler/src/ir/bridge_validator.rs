use crate::bridge_resolver::{resolve_bridge, BridgeTable};
use crate::{IrError, SymbolTable};
use hwc_engine::{geometry::BoundingBox, HardwareSpace};
use hwc_parser::ProfileDefinition;

/// Validate material transitions against the Profile/BridgeTable
/// Implements P45: Forbidden Junction detection rule.
pub fn validate_bridges(
    space: &HardwareSpace,
    profile: Option<&ProfileDefinition>,
    symbol_table: Option<&SymbolTable>,
) -> Result<(), IrError> {
    // v0.2.0: Use from_profile_and_symbol_table to load bridges from both sources
    let profile_bridge_table = Some(BridgeTable::from_profile_and_symbol_table(
        profile,
        symbol_table,
    ));

    // Check every contact
    for contact in &space.contacts {
        // Auto-vias are already validated during insertion.
        if contact.name.starts_with("AutoVia") {
            continue;
        }

        let bbox = match &contact.bbox {
            Some(b) => b,
            None => continue,
        };

        let from_layer_name = contact.from_layer.as_deref();
        let to_layer_name = contact.to_layer.as_deref();

        // Find pours on the lower connected plane that overlap in XY and match from_layer
        let start_pours: Vec<_> = space
            .pours
            .iter()
            .filter(|p| {
                p.z_bottom_nm == contact.z_start_nm
                    && overlaps_xy(p.bbox.as_ref(), bbox)
                    && space
                        .material_registry
                        .get_category_by_name(&p.material_name)
                        .map_or(true, |cat| !cat.is_zero_thickness())
                    && from_layer_name.map_or(true, |fl| {
                        space.stackup_layers.iter().any(|l| {
                            l.name == fl
                                && (l.z_bottom == p.z_bottom_nm
                                    || (l.z_bottom <= p.z_bottom_nm && p.z_bottom_nm < l.z_top))
                                && l.material_name == p.material_name
                        })
                    })
            })
            .collect();

        // Find pours on the upper connected plane that overlap in XY and match to_layer
        let end_pours: Vec<_> = space
            .pours
            .iter()
            .filter(|p| {
                p.z_bottom_nm == contact.z_end_nm
                    && overlaps_xy(p.bbox.as_ref(), bbox)
                    && space
                        .material_registry
                        .get_category_by_name(&p.material_name)
                        .map_or(true, |cat| !cat.is_zero_thickness())
                    && to_layer_name.map_or(true, |tl| {
                        space.stackup_layers.iter().any(|l| {
                            l.name == tl
                                && (l.z_bottom == p.z_bottom_nm
                                    || (l.z_bottom <= p.z_bottom_nm && p.z_bottom_nm < l.z_top))
                                && l.material_name == p.material_name
                        })
                    })
            })
            .collect();

        // For every combination, check bridge compatibility
        for p1 in &start_pours {
            for p2 in &end_pours {
                let bridge_stack = resolve_bridge(
                    &p1.material_name,
                    &p2.material_name,
                    profile_bridge_table.as_ref(),
                    None,                      // stdlib_table
                    contact.bridge.as_deref(), // check if the user provided one
                );

                // If the bridge resolver fails, it means no valid bridge was found
                // OR the user omitted a required bridge for this transition.
                if let Err(e) = bridge_stack {
                    return Err(IrError::BridgeValidationFailed {
                        from_material: p1.material_name.clone(),
                        to_material: p2.material_name.clone(),
                        reason: e.to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn overlaps_xy(pour_bbox: Option<&BoundingBox>, contact_bbox: &BoundingBox) -> bool {
    let Some(pb) = pour_bbox else {
        return false;
    };
    pb.max.x > contact_bbox.min.x
        && contact_bbox.max.x > pb.min.x
        && pb.max.y > contact_bbox.min.y
        && contact_bbox.max.y > pb.min.y
}
