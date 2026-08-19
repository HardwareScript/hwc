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
                        let eps = 100.0; // 100nm margin for boundary precision
                        if point.0 >= (bb.min.x as f64 - eps)
                            && point.0 <= (bb.max.x as f64 + eps)
                            && point.1 >= (bb.min.y as f64 - eps)
                            && point.1 <= (bb.max.y as f64 + eps)
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

/// Classify the semantic role of a layout pour purely from geometric and topological properties.
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
        let is_power_or_gnd = space
            .net_classifications
            .get(net)
            .map_or(false, |c| {
                *c == hwc_engine::space::NetClassification::Ground
                    || *c == hwc_engine::space::NetClassification::Power
            });

        // Check if this pour encloses multiple contact pillars (power/ground bus mesh)
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

        // Check if any vertical vias pass through this pour
        let has_through_vias = space.contacts.iter().any(|c| {
            if let Some(ref cb) = c.bbox {
                if let Some(ref pb) = pour.bbox {
                    let cx = (cb.min.x + cb.max.x) / 2;
                    let cy = (cb.min.y + cb.max.y) / 2;
                    cx >= pb.min.x && cx <= pb.max.x && cy >= pb.min.y && cy <= pb.max.y
                } else {
                    false
                }
            } else {
                false
            }
        });

        // Pours without through-vias that interface external circuits are boundary pads
        if !has_through_vias {
            return PourRole::ExternalPad { net: net.clone() };
        }

        return PourRole::InterconnectStrap {
            net: Some(net.clone()),
        };
    }

    PourRole::InterconnectStrap { net: None }
}
