//! Planar Cross-Net Short Circuit & Pour Clearance Validation.
//!
//! **Core Physical Signoff Rule:**
//! Conductive shapes on the same physical layer belonging to different electrical
//! nets must NEVER intersect (distance <= 0 is a fatal short circuit) and must
//! maintain at least `min_spacing` clearance.
//!
//! **Device Contract Exemption:**
//! Pours bound to the *same* physical device instance (e.g. terminals of a resistor
//! touching its resistive body) are exempted as declared physical channels.
//!
//! **Non-Conductive Layer Exemption:**
//! Zero-thickness chemical masks (e.g. psdm, rpm, npc) and dielectric layers are
//! excluded because they do not conduct electricity.

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;
use crate::space::{HardwareSpace, PourMetadata};

use super::types::DrcViolation;

/// Validates that no cross-net planar shapes touch, overlap, or violate clearance.
pub fn validate_planar_shorts(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<Vec<DrcViolation>, String> {
    let mut violations = Vec::new();

    // 1. Fail loudly on missing spacing constraint — zero silent defaults!
    let min_spacing_nm = constraints
        .fabrication
        .as_ref()
        .map(|f| f.min_trace_spacing_nm)
        .or_else(|| {
            space
                .fabrication_constraints
                .as_ref()
                .map(|f| f.trace.min_spacing_nm)
        })
        .ok_or_else(|| {
            "[DRC] FATAL: Profile must declare 'min_spacing' in trace/fabrication constraints."
                .to_string()
        })?;

    // 2. Pre-filter and group ONLY conductive pours by layer in O(N) time
    let mut pours_by_layer: rustc_hash::FxHashMap<&str, Vec<&PourMetadata>> =
        rustc_hash::FxHashMap::default();

    for pour in &space.pours {
        // Skip unassigned shapes
        if pour.net.is_none() || pour.bbox.is_none() {
            continue;
        }

        // Check layer conductivity once per pour
        let is_conductive = if space.stackup_layers.is_empty() {
            true
        } else {
            space
                .stackup_layers
                .iter()
                .find(|l| l.name == pour.layer_name)
                .map_or(false, |l| (l.is_routable || l.is_device_layer) && !l.is_mask)
        };

        if is_conductive {
            pours_by_layer.entry(pour.layer_name.as_str()).or_default().push(pour);
        }
    }

    // 3. Evaluate pairs strictly on common conductive layers
    for (_layer, layer_pours) in pours_by_layer {
        let len = layer_pours.len();
        for i in 0..len {
            for j in (i + 1)..len {
                let p_a = layer_pours[i];
                let p_b = layer_pours[j];

                let net_a = p_a.net.as_ref().unwrap();
                let net_b = p_b.net.as_ref().unwrap();

                // Same net is permitted to weld (PIVB union)
                if net_a == net_b {
                    continue;
                }

                // Typed device instance exemption
                if let (Some(ref dev_a), Some(ref dev_b)) = (&p_a.device_binding, &p_b.device_binding) {
                    if dev_a.device_name == dev_b.device_name {
                        continue;
                    }
                }

                let bb_a = p_a.bbox.as_ref().unwrap();
                let bb_b = p_b.bbox.as_ref().unwrap();

                // 2D AABB Overlap Check
                let x_overlap = bb_a.max.x >= bb_b.min.x && bb_b.max.x >= bb_a.min.x;
                let y_overlap = bb_a.max.y >= bb_b.min.y && bb_b.max.y >= bb_a.min.y;

                if x_overlap && y_overlap {
                    let overlap_x = (bb_a.max.x.min(bb_b.max.x) - bb_a.min.x.max(bb_b.min.x)).max(0);
                    let overlap_y = (bb_a.max.y.min(bb_b.max.y) - bb_a.min.y.max(bb_b.min.y)).max(0);

                    violations.push(DrcViolation::NetShortViolation {
                        net_a: net_a.clone(),
                        net_b: net_b.clone(),
                        element_a: p_a.name.clone(),
                        element_b: p_b.name.clone(),
                        layer: p_a.layer_name.clone(),
                        overlap_nm2: overlap_x * overlap_y,
                        location: Point3D::new(
                            (bb_a.min.x.max(bb_b.min.x) + bb_a.max.x.min(bb_b.max.x)) / 2,
                            (bb_a.min.y.max(bb_b.min.y) + bb_a.max.y.min(bb_b.max.y)) / 2,
                            p_a.z_bottom_nm,
                        ),
                    });
                    continue;
                }

                // Separation Distance
                let dx = (bb_b.min.x - bb_a.max.x).max(bb_a.min.x - bb_b.max.x).max(0);
                let dy = (bb_b.min.y - bb_a.max.y).max(bb_a.min.y - bb_b.max.y).max(0);
                let clearance_nm = if dx == 0 {
                    dy
                } else if dy == 0 {
                    dx
                } else {
                    ((dx * dx + dy * dy) as f64).sqrt().round() as i64
                };

                if clearance_nm < min_spacing_nm {
                    violations.push(DrcViolation::ClearanceViolation {
                        net_a: net_a.clone(),
                        net_b: net_b.clone(),
                        actual_nm: clearance_nm,
                        required_nm: min_spacing_nm,
                        location: Point3D::new(
                            (bb_a.center_x() + bb_b.center_x()) / 2,
                            (bb_a.center_y() + bb_b.center_y()) / 2,
                            p_a.z_bottom_nm,
                        ),
                    });
                }
            }
        }
    }

    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint_manager::{ConstraintRulebook, FabricationConstraints};
    use crate::geometry::BoundingBox;
    use crate::space::{
        BindingPriority, DeviceBinding, Dimensions, HardwareSpace, PourMetadata, SpaceView,
        StackupLayer,
    };
    use crate::stackup::LayerKind;

    fn create_test_space() -> HardwareSpace {
        let mut space = HardwareSpace::new(
            "test_space".into(),
            Dimensions::new(200_000, 200_000, 10_000),
            0,
            SpaceView::Silicon,
        );
        space.stackup_layers.push(StackupLayer::new(
            "metal1".into(),
            800,
            1100,
            300,
            "copper".into(),
            true,
            false,
            LayerKind::Conductor,
        ));
        space.stackup_layers.push(StackupLayer::new(
            "psdm".into(),
            0,
            0,
            0,
            "implant".into(),
            false,
            true,
            LayerKind::Mask,
        ));
        space
    }

    fn create_test_constraints(min_spacing_nm: i64) -> ConstraintRulebook {
        let mut book = ConstraintRulebook::new(1);
        book.fabrication = Some(FabricationConstraints {
            min_trace_width_nm: 100,
            min_trace_spacing_nm: min_spacing_nm,
            min_via_diameter_nm: 100,
            default_via_diameter_nm: 100,
            min_enclosure_nm: 50,
            min_spacing_nm,
            low_voltage_clearance_nm: min_spacing_nm,
            medium_voltage_clearance_nm: min_spacing_nm,
            high_voltage_clearance_nm: min_spacing_nm,
            safety_factor: 1.0,
            stackup: None,
            technology: hwc_types::Technology::Asic,
            layer_via_enclosures: rustc_hash::FxHashMap::default(),
            max_substrate_tap_distance_nm: None,
            substrate_net: None,
            cuts: rustc_hash::FxHashMap::default(),
            layer_pair_rules: Vec::new(),
        });
        book
    }

    fn create_pour(
        name: &str,
        layer: &str,
        net: Option<&str>,
        min_x: i64,
        min_y: i64,
        max_x: i64,
        max_y: i64,
        dev_binding: Option<DeviceBinding>,
    ) -> PourMetadata {
        PourMetadata {
            name: name.into(),
            material_name: "copper".into(),
            layer_name: layer.into(),
            layer_id: None,
            z_bottom_nm: 800,
            net: net.map(|s| s.into()),
            area_nm2: (max_x - min_x) * (max_y - min_y),
            bbox: Some(BoundingBox::new(
                Point3D::new(min_x, min_y, 800),
                Point3D::new(max_x, max_y, 1100),
            )),
            device_binding: dev_binding,
            merged_region_id: None,
            via_landing_nodes: Vec::new(),
            waivers: hwc_parser::Waivers::default(),
        }
    }

    #[test]
    fn test_cross_net_short_violation() {
        let mut space = create_test_space();
        let constraints = create_test_constraints(300);

        // Two overlapping pours on metal1 with different nets
        space.pours.push(create_pour("pad1", "metal1", Some("In"), 0, 0, 10_000, 10_000, None));
        space.pours.push(create_pour("pad2", "metal1", Some("GND"), 5_000, 5_000, 15_000, 15_000, None));

        let violations = validate_planar_shorts(&space, &constraints).unwrap();
        assert_eq!(violations.len(), 1);
        match &violations[0] {
            DrcViolation::NetShortViolation { net_a, net_b, layer, overlap_nm2, .. } => {
                assert_eq!(net_a.as_str(), "In");
                assert_eq!(net_b.as_str(), "GND");
                assert_eq!(layer.as_str(), "metal1");
                assert_eq!(*overlap_nm2, 25_000_000); // 5000 x 5000
            }
            other => panic!("Expected NetShortViolation, got {:?}", other),
        }
    }

    #[test]
    fn test_same_net_overlap_allowed() {
        let mut space = create_test_space();
        let constraints = create_test_constraints(300);

        // Two overlapping pours with the SAME net (PIVB weld)
        space.pours.push(create_pour("pad1", "metal1", Some("VCC"), 0, 0, 10_000, 10_000, None));
        space.pours.push(create_pour("pad2", "metal1", Some("VCC"), 5_000, 5_000, 15_000, 15_000, None));

        let violations = validate_planar_shorts(&space, &constraints).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn test_same_device_exemption() {
        let mut space = create_test_space();
        let constraints = create_test_constraints(300);

        let dev_bind_a = DeviceBinding {
            device_name: "R1".into(),
            terminals: vec!["A".into()],
            priority: BindingPriority::Contact,
            def_path: None,
        };
        let dev_bind_b = DeviceBinding {
            device_name: "R1".into(),
            terminals: vec!["B".into()],
            priority: BindingPriority::Contact,
            def_path: None,
        };

        // Two pours on same device instance touching/overlapping
        space.pours.push(create_pour("r1_a", "metal1", Some("In"), 0, 0, 10_000, 10_000, Some(dev_bind_a)));
        space.pours.push(create_pour("r1_b", "metal1", Some("Out"), 5_000, 5_000, 15_000, 15_000, Some(dev_bind_b)));

        let violations = validate_planar_shorts(&space, &constraints).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn test_mask_layer_exemption() {
        let mut space = create_test_space();
        let constraints = create_test_constraints(300);

        // Two overlapping shapes on non-conductive mask layer psdm
        space.pours.push(create_pour("mask1", "psdm", Some("NetA"), 0, 0, 10_000, 10_000, None));
        space.pours.push(create_pour("mask2", "psdm", Some("NetB"), 5_000, 5_000, 15_000, 15_000, None));

        let violations = validate_planar_shorts(&space, &constraints).unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn test_clearance_violation() {
        let mut space = create_test_space();
        let constraints = create_test_constraints(1000); // 1000nm required

        // Two non-overlapping pours with separation 500nm < 1000nm
        space.pours.push(create_pour("pad1", "metal1", Some("In"), 0, 0, 10_000, 10_000, None));
        space.pours.push(create_pour("pad2", "metal1", Some("GND"), 10_500, 0, 20_500, 10_000, None));

        let violations = validate_planar_shorts(&space, &constraints).unwrap();
        assert_eq!(violations.len(), 1);
        match &violations[0] {
            DrcViolation::ClearanceViolation { net_a, net_b, actual_nm, required_nm, .. } => {
                assert_eq!(net_a.as_str(), "In");
                assert_eq!(net_b.as_str(), "GND");
                assert_eq!(*actual_nm, 500);
                assert_eq!(*required_nm, 1000);
            }
            other => panic!("Expected ClearanceViolation, got {:?}", other),
        }
    }

    #[test]
    fn test_missing_constraints_fails_fast() {
        let space = create_test_space();
        let book = ConstraintRulebook::new(1); // No fabrication constraints

        let result = validate_planar_shorts(&space, &book);
        assert!(result.is_err());
    }
}
