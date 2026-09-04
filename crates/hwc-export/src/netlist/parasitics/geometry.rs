//! Geometric queries and stackup utilities for parasitic extraction.

use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::{PourMetadata, StackupLayer};
use hwc_engine::HardwareSpace;

/// Lookup a stackup layer by layer name.
pub fn find_stackup_layer_by_name<'a>(
    space: &'a HardwareSpace,
    layer_name: &str,
) -> Option<&'a StackupLayer> {
    space.stackup_layers.iter().find(|l| l.name == layer_name)
}

/// Lookup a stackup layer by layer name, material name, or Z elevation window.
pub fn find_stackup_layer<'a>(
    space: &'a HardwareSpace,
    layer_name_or_material: &str,
    z_bottom: i64,
) -> Option<&'a StackupLayer> {
    // 1. Exact layer name match
    if let Some(l) = space.stackup_layers.iter().find(|l| l.name == layer_name_or_material) {
        return Some(l);
    }
    // 2. Exact material match at elevation
    if let Some(l) = space.stackup_layers.iter().find(|l| {
        l.material_name == layer_name_or_material
            && (l.z_bottom == z_bottom || (l.z_bottom <= z_bottom && z_bottom < l.z_top))
    }) {
        return Some(l);
    }
    // 3. Elevation window on routable layer
    space.stackup_layers.iter().find(|l| {
        !l.is_mask && (l.z_bottom == z_bottom || (l.z_bottom <= z_bottom && z_bottom < l.z_top))
    })
}

/// Search the stackup for the closest dielectric layer directly below a given Z coordinate.
pub fn find_dielectric_below(space: &HardwareSpace, z: f64) -> Option<(f64, f64)> {
    let mut found_dielectric: Option<(f64, f64)> = None;
    let mut min_distance = f64::MAX;

    for layer in &space.stackup_layers {
        let layer_z_top = layer.z_top as f64;
        if layer_z_top <= z {
            if let Some(mat_id) = space.material_registry.get_id(&layer.material_name) {
                if space.material_registry.is_insulator(mat_id) {
                    let distance = z - layer_z_top;
                    if distance < min_distance {
                        if let Some(props) = space.material_registry.get_physical_props(mat_id) {
                            if let Some(eps_r) = props.get("relative_permittivity") {
                                let thickness = (layer.z_top - layer.z_bottom) as f64;
                                found_dielectric = Some((thickness, eps_r));
                                min_distance = distance;
                            }
                        }
                    }
                }
            }
        }
    }

    found_dielectric
}

/// Find the most specific pour containing a 2D point (x, y) on a given layer for a net.
pub fn find_pour_at_point<'a>(
    space: &'a HardwareSpace,
    net_name: &str,
    layer_name: &str,
    point: (f64, f64),
) -> Option<&'a PourMetadata> {
    let mut candidates: Vec<&PourMetadata> = Vec::new();
    for pour in &space.pours {
        if let Some(ref p_net) = pour.net {
            if p_net == net_name {
                let pour_matches_layer = if !pour.layer_name.is_empty() {
                    pour.layer_name == layer_name
                } else if let Some(stackup_l) = find_stackup_layer(space, &pour.material_name, pour.z_bottom_nm) {
                    stackup_l.name == layer_name
                } else {
                    false
                };

                if pour_matches_layer {
                    if let Some(ref bb) = pour.bbox {
                        if point.0 >= bb.min.x as f64
                            && point.0 <= bb.max.x as f64
                            && point.1 >= bb.min.y as f64
                            && point.1 <= bb.max.y as f64
                        {
                            candidates.push(pour);
                        }
                    }
                }
            }
        }
    }

    // Return the most specific (smallest area) pour
    candidates.sort_by(|a, b| {
        let area_a = a.bbox.as_ref().map_or(f64::MAX, |bb| {
            (bb.max.x - bb.min.x).abs() as f64 * (bb.max.y - bb.min.y).abs() as f64
        });
        let area_b = b.bbox.as_ref().map_or(f64::MAX, |bb| {
            (bb.max.x - bb.min.x).abs() as f64 * (bb.max.y - bb.min.y).abs() as f64
        });
        area_a.partial_cmp(&area_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    candidates.first().copied()
}

/// Compute 2D centroid (in nm) from an optional BoundingBox.
pub fn get_bbox_centroid(bbox: Option<&BoundingBox>) -> (f64, f64) {
    if let Some(bb) = bbox {
        (
            (bb.min.x as f64 + bb.max.x as f64) / 2.0,
            (bb.min.y as f64 + bb.max.y as f64) / 2.0,
        )
    } else {
        (0.0, 0.0)
    }
}

/// 2D Euclidean distance in nm.
pub fn distance_2d(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    let dx = p1.0 - p2.0;
    let dy = p1.1 - p2.1;
    (dx * dx + dy * dy).sqrt()
}

/// Classify the semantic role of a layout pour purely from typed metadata.
///
/// ## Zero String Heuristics
/// This function must NEVER inspect foundry-specific name substrings (e.g. "sky130_",
/// "__", "pad", "_start"). Classification must come from typed fields only:
/// - `pour.device_binding` → DeviceTerminal (highest priority)
/// - Pour is on a dedicated `pad` mask layer → ExternalPad
/// - Pour encloses ≥2 contacts on a power/gnd net → PowerBus
/// - Everything else → InterconnectStrap
pub fn classify_pour(space: &HardwareSpace, pour: &PourMetadata) -> super::types::PourRole {
    use super::types::PourRole;

    // 1. Explicit device terminal binding takes highest priority
    if let Some(ref binding) = pour.device_binding {
        return PourRole::DeviceTerminal {
            device: binding.device_name.clone(),
            terminals: binding.terminals.clone(),
        };
    }

    // 2. Net-assigned pours
    if let Some(ref net) = pour.net {
        // A pour is an ExternalPad if and only if it lives on the dedicated "pad"
        // mask layer — the layer added by the pad() PCell: `cell.add_polygon(layer: "pad", ...)`.
        // This is a process-independent typed property; no name-string matching required.
        let stackup_layer = find_stackup_layer_by_name(space, pour.layer_name.as_str());
        let is_pad_mask_layer = stackup_layer.map_or(false, |sl| {
            // The pad mask layer is defined as a zero-thickness (is_mask) layer
            // whose name is exactly "pad" in the stackup. This is set by the PDK profile,
            // not inferred from pour name strings.
            sl.is_mask && sl.name == "pad"
        });

        if is_pad_mask_layer {
            return PourRole::ExternalPad { net: net.clone() };
        }

        // 3. Power/ground bus mesh: encloses ≥2 contact pillars on a classified net
        let is_power_or_gnd = space
            .net_classifications
            .get(net)
            .map_or(false, |c| {
                *c == hwc_engine::space::NetClassification::Ground
                    || *c == hwc_engine::space::NetClassification::Power
            });

        if is_power_or_gnd {
            let mut enclosed_contact_count = 0;
            if let Some(ref bb) = pour.bbox {
                for contact in &space.contacts {
                    if let Some(ref cb) = contact.bbox {
                        let cx = (cb.min.x + cb.max.x) / 2;
                        let cy = (cb.min.y + cb.max.y) / 2;
                        if cx >= bb.min.x && cx <= bb.max.x && cy >= bb.min.y && cy <= bb.max.y {
                            enclosed_contact_count += 1;
                        }
                    }
                }
            }
            if enclosed_contact_count >= 2 {
                return PourRole::PowerBus { net: net.clone() };
            }
        }

        return PourRole::InterconnectStrap {
            net: Some(net.clone()),
        };
    }

    PourRole::InterconnectStrap { net: None }
}

/// Check if a pour is shielded/occluded from the substrate (GND) by lower conductive layers.
pub fn is_occluded_from_substrate(space: &HardwareSpace, pour: &PourMetadata) -> bool {
    let Some(ref bb) = pour.bbox else { return false };
    
    // Check if any other conductive pour exists beneath this pour (z_bottom < pour.z_bottom_nm)
    for lower_pour in &space.pours {
        if lower_pour.name == pour.name && lower_pour.layer_name == pour.layer_name {
            continue;
        }
        if lower_pour.z_bottom_nm < pour.z_bottom_nm {
            if let Some(mat_id) = space.material_registry.get_id(&lower_pour.material_name) {
                if space.material_registry.is_conductor(mat_id) || space.material_registry.is_semiconductor(mat_id) {
                    if let Some(ref lbb) = lower_pour.bbox {
                        let cx = (bb.min.x + bb.max.x) / 2;
                        let cy = (bb.min.y + bb.max.y) / 2;
                        // If lower conductive pour covers the centroid/area of this pour, it shields it
                        if cx >= lbb.min.x && cx <= lbb.max.x && cy >= lbb.min.y && cy <= lbb.max.y {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
