//! Generic Data-Driven 2D Layer-Pair DRC Evaluator.
//!
//! ## Zero-Magic Compiler Mandate
//!
//! Evaluates planar 2D polygon relationships (Enclosure, Clearance, Extension, Overlap) between
//! arbitrary layer pairs declared in the PDK fabrication profile (`.hw`).
//! The Rust binary contains zero foundry-specific constants, layer names, or rule numbers.

use compact_str::CompactString;
use hwc_materials::{CutDefinition, LayerPairDrcRule, LayerPairRuleType};
use hwc_physics::Point3D;
use rustc_hash::FxHashMap;

use super::types::DrcViolation;
use crate::HardwareSpace;

/// A 2D planar element on a layer (pour polygon or contact cut).
struct PlanarElement<'a> {
    name: &'a str,
    min_x: i64,
    max_x: i64,
    min_y: i64,
    max_y: i64,
    center_x: i64,
    center_y: i64,
    z_nm: i64,
}

impl<'a> PlanarElement<'a> {
    #[inline]
    fn overlaps_2d(&self, other: &PlanarElement<'_>) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }
}

/// Collects ONLY 2D planar pours on the specified stackup layer.
fn get_layer_pours<'a>(space: &'a HardwareSpace, layer_name: &str) -> Vec<PlanarElement<'a>> {
    let mut elements = Vec::new();
    for p in &space.pours {
        if p.layer_name == layer_name {
            if let Some(bb) = &p.bbox {
                elements.push(PlanarElement {
                    name: p.name.as_str(),
                    min_x: bb.min.x,
                    max_x: bb.max.x,
                    min_y: bb.min.y,
                    max_y: bb.max.y,
                    center_x: (bb.min.x + bb.max.x) / 2,
                    center_y: (bb.min.y + bb.max.y) / 2,
                    z_nm: p.z_bottom_nm,
                });
            }
        }
    }
    elements
}

/// Filters contact cuts dynamically matching a declared PDK CutDefinition.
fn get_cut_elements<'a>(
    space: &'a HardwareSpace,
    cut_def: &CutDefinition,
) -> Vec<PlanarElement<'a>> {
    space
        .contacts
        .iter()
        .filter(|c| {
            let matches_from = c
                .from_layer
                .as_ref()
                .map_or(false, |f| cut_def.from_layers.iter().any(|l| l == f));
            let matches_to = c
                .to_layer
                .as_ref()
                .map_or(false, |t| t == &cut_def.to_layer);
            matches_from && matches_to
        })
        .filter_map(|c| {
            c.bbox.as_ref().map(|bb| PlanarElement {
                name: c.name.as_str(),
                min_x: bb.min.x,
                max_x: bb.max.x,
                min_y: bb.min.y,
                max_y: bb.max.y,
                center_x: (bb.min.x + bb.max.x) / 2,
                center_y: (bb.min.y + bb.max.y) / 2,
                z_nm: c.z_start_nm,
            })
        })
        .collect()
}

/// Resolves geometry for a rule target: if the name corresponds to a declared Cut,
/// queries cuts dynamically; otherwise queries planar pours on that layer.
fn get_elements_for_layer_or_cut<'a>(
    space: &'a HardwareSpace,
    name: &str,
    cuts: &FxHashMap<CompactString, CutDefinition>,
) -> Vec<PlanarElement<'a>> {
    if let Some(cut_def) = cuts.get(name) {
        get_cut_elements(space, cut_def)
    } else {
        get_layer_pours(space, name)
    }
}

/// Evaluates generic 2D planar polygon clearances, extensions, and enclosures between layer pairs.
pub fn validate_layer_pair_rules(
    space: &HardwareSpace,
    rules: &[LayerPairDrcRule],
    cuts: &FxHashMap<CompactString, CutDefinition>,
) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();

    for rule in rules {
        match rule.rule_type {
            LayerPairRuleType::Enclosure => {
                let elems_a = get_layer_pours(space, &rule.layer_a);
                let elems_b = get_layer_pours(space, &rule.layer_b);

                for b in &elems_b {
                    let mut best_enc = i64::MIN;
                    let mut found_enclosing = false;

                    for a in &elems_a {
                        if !a.overlaps_2d(b) {
                            continue;
                        }

                        // Planar enclosure margins (how much `a` extends beyond `b` in all 4 directions)
                        let enc_left = b.min_x - a.min_x;
                        let enc_right = a.max_x - b.max_x;
                        let enc_bot = b.min_y - a.min_y;
                        let enc_top = a.max_y - b.max_y;

                        // Strict 4-sided enclosure
                        let actual_enc = enc_left.min(enc_right).min(enc_bot).min(enc_top);

                        if actual_enc >= rule.min_distance_nm {
                            found_enclosing = true;
                            break;
                        }
                        if actual_enc > best_enc {
                            best_enc = actual_enc;
                        }
                    }

                    if !found_enclosing {
                        let actual_nm = if best_enc == i64::MIN { 0 } else { best_enc };
                        violations.push(DrcViolation::MaskRuleViolation {
                            rule: rule.rule_code.clone(),
                            mask_layer: rule.layer_a.clone(),
                            target_layer: rule.layer_b.clone(),
                            actual_nm,
                            required_nm: rule.min_distance_nm,
                            location: Point3D::new(b.center_x, b.center_y, b.z_nm),
                            description: format!(
                                "Layer '{}' encloses '{}' ({}) by {} nm, violating minimum enclosure of {} nm (Rule: {})",
                                rule.layer_a,
                                rule.layer_b,
                                b.name,
                                actual_nm,
                                rule.min_distance_nm,
                                rule.rule_code
                            )
                            .into(),
                        });
                    }
                }
            }

            LayerPairRuleType::TransverseEnclosure => {
                let elems_a = get_layer_pours(space, &rule.layer_a);
                let elems_b = get_layer_pours(space, &rule.layer_b);

                for b in &elems_b {
                    let mut best_enc = i64::MIN;
                    let mut found_enclosing = false;

                    for a in &elems_a {
                        if !a.overlaps_2d(b) {
                            continue;
                        }

                        // Planar enclosure margins (how much `a` extends beyond `b` in all 4 directions)
                        let enc_left = b.min_x - a.min_x;
                        let enc_right = a.max_x - b.max_x;
                        let enc_bot = b.min_y - a.min_y;
                        let enc_top = a.max_y - b.max_y;

                        // Transverse enclosure along width (either Y axis or X axis)
                        let actual_enc = if enc_bot >= 0 && enc_top >= 0 && enc_left >= 0 && enc_right >= 0 {
                            (enc_bot.min(enc_top)).max(enc_left.min(enc_right))
                        } else if enc_bot >= 0 && enc_top >= 0 {
                            enc_bot.min(enc_top)
                        } else if enc_left >= 0 && enc_right >= 0 {
                            enc_left.min(enc_right)
                        } else {
                            enc_left.min(enc_right).min(enc_bot).min(enc_top)
                        };

                        if actual_enc >= rule.min_distance_nm {
                            found_enclosing = true;
                            break;
                        }
                        if actual_enc > best_enc {
                            best_enc = actual_enc;
                        }
                    }

                    if !found_enclosing {
                        let actual_nm = if best_enc == i64::MIN { 0 } else { best_enc };
                        violations.push(DrcViolation::MaskRuleViolation {
                            rule: rule.rule_code.clone(),
                            mask_layer: rule.layer_a.clone(),
                            target_layer: rule.layer_b.clone(),
                            actual_nm,
                            required_nm: rule.min_distance_nm,
                            location: Point3D::new(b.center_x, b.center_y, b.z_nm),
                            description: format!(
                                "Layer '{}' transversely encloses '{}' ({}) by {} nm, violating minimum transverse enclosure of {} nm (Rule: {})",
                                rule.layer_a,
                                rule.layer_b,
                                b.name,
                                actual_nm,
                                rule.min_distance_nm,
                                rule.rule_code
                            )
                            .into(),
                        });
                    }
                }
            }

            LayerPairRuleType::Extension => {
                let elems_a = get_layer_pours(space, &rule.layer_a);
                let elems_b = get_layer_pours(space, &rule.layer_b);

                for b in &elems_b {
                    let mut best_ext = i64::MIN;
                    let mut found_extension = false;
                    let mut has_overlapping_a = false;

                    for a in &elems_a {
                        if !a.overlaps_2d(b) {
                            continue;
                        }
                        has_overlapping_a = true;

                        let ext_left = b.min_x - a.min_x;
                        let ext_right = a.max_x - b.max_x;
                        let ext_bot = b.min_y - a.min_y;
                        let ext_top = a.max_y - b.max_y;

                        let actual_ext = ext_left.max(ext_right).max(ext_bot).max(ext_top);

                        if actual_ext >= rule.min_distance_nm {
                            found_extension = true;
                            break;
                        }
                        if actual_ext > best_ext {
                            best_ext = actual_ext;
                        }
                    }

                    if has_overlapping_a && !found_extension {
                        let actual_nm = if best_ext == i64::MIN { 0 } else { best_ext };
                        violations.push(DrcViolation::MaskRuleViolation {
                            rule: rule.rule_code.clone(),
                            mask_layer: rule.layer_a.clone(),
                            target_layer: rule.layer_b.clone(),
                            actual_nm,
                            required_nm: rule.min_distance_nm,
                            location: Point3D::new(b.center_x, b.center_y, b.z_nm),
                            description: format!(
                                "Layer '{}' extends past '{}' ({}) by {} nm, violating minimum extension of {} nm (Rule: {})",
                                rule.layer_a,
                                rule.layer_b,
                                b.name,
                                actual_nm,
                                rule.min_distance_nm,
                                rule.rule_code
                            )
                            .into(),
                        });
                    }
                }
            }

            LayerPairRuleType::Clearance | LayerPairRuleType::Overlap => {
                let elems_a = get_elements_for_layer_or_cut(space, &rule.layer_a, cuts);
                let elems_b = get_elements_for_layer_or_cut(space, &rule.layer_b, cuts);

                for a in &elems_a {
                    for b in &elems_b {
                        if a.name == b.name && a.min_x == b.min_x && a.min_y == b.min_y && a.max_x == b.max_x && a.max_y == b.max_y {
                            continue;
                        }

                        let actual_clearance = if a.overlaps_2d(b) {
                            0
                        } else {
                            let dx = if a.max_x < b.min_x {
                                b.min_x - a.max_x
                            } else if b.max_x < a.min_x {
                                a.min_x - b.max_x
                            } else {
                                0
                            };

                            let dy = if a.max_y < b.min_y {
                                b.min_y - a.max_y
                            } else if b.max_y < a.min_y {
                                a.min_y - b.max_y
                            } else {
                                0
                            };

                            if dx == 0 {
                                dy
                            } else if dy == 0 {
                                dx
                            } else {
                                ((dx * dx + dy * dy) as f64).sqrt().round() as i64
                            }
                        };

                        if actual_clearance < rule.min_distance_nm {
                            violations.push(DrcViolation::MaskRuleViolation {
                                rule: rule.rule_code.clone(),
                                mask_layer: rule.layer_a.clone(),
                                target_layer: rule.layer_b.clone(),
                                actual_nm: actual_clearance,
                                required_nm: rule.min_distance_nm,
                                location: Point3D::new(
                                    (a.center_x + b.center_x) / 2,
                                    (a.center_y + b.center_y) / 2,
                                    a.z_nm,
                                ),
                                description: format!(
                                    "Clearance between '{}' ({}) and '{}' ({}) is {} nm, violating minimum clearance of {} nm (Rule: {})",
                                    rule.layer_a,
                                    a.name,
                                    rule.layer_b,
                                    b.name,
                                    actual_clearance,
                                    rule.min_distance_nm,
                                    rule.rule_code
                                )
                                .into(),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(violations)
}
