use super::*;
use crate::connectivity::{BoundingBox, ContactMetadata, PourMetadata};

#[test]
fn test_simple_connected_net() {
    // Two pours on same net, physically touching
    let pours = vec![
        PourMetadata {
            name: "pour1".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 1000,
                max_y: 1000,
                max_z: 100,
            }),
        },
        PourMetadata {
            name: "pour2".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 500,
                min_y: 0,
                min_z: 0,
                max_x: 1500,
                max_y: 1000,
                max_z: 100,
            }),
        },
    ];

    let checker = PhysicalContinuityChecker::new(100, &pours, &[], &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    // Should have 1 island, 0 violations
    assert_eq!(islands.len(), 1);
    assert_eq!(violations.len(), 0);
}

#[test]
fn test_disconnected_net() {
    // Two pours on same net, NOT touching
    let pours = vec![
        PourMetadata {
            name: "pour1".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 1000,
                max_y: 1000,
                max_z: 100,
            }),
        },
        PourMetadata {
            name: "pour2".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 2000,
                min_y: 0,
                min_z: 0,
                max_x: 3000,
                max_y: 1000,
                max_z: 100,
            }),
        },
    ];

    let checker = PhysicalContinuityChecker::new(100, &pours, &[], &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    // Should have 2 islands, 1 P41 violation
    assert_eq!(islands.len(), 2);
    assert_eq!(violations.len(), 1);

    match &violations[0] {
        PhysicalContinuityViolation::DisconnectedNet {
            net_name,
            island_count,
            ..
        } => {
            assert_eq!(net_name, "VCC");
            assert_eq!(*island_count, 2);
        }
        _ => panic!("Expected DisconnectedNet violation"),
    }
}

#[test]
fn test_short_circuit() {
    // Two pours on different nets, physically touching
    let pours = vec![
        PourMetadata {
            name: "pour1".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 1000,
                max_y: 1000,
                max_z: 100,
            }),
        },
        PourMetadata {
            name: "pour2".into(),
            material_name: "Copper".into(),
            net: Some("GND".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 500,
                min_y: 0,
                min_z: 0,
                max_x: 1500,
                max_y: 1000,
                max_z: 100,
            }),
        },
    ];

    let checker = PhysicalContinuityChecker::new(100, &pours, &[], &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    // Should have 1 island with 2 nets, 1 P42 violation
    assert_eq!(islands.len(), 1);
    assert_eq!(violations.len(), 1);

    match &violations[0] {
        PhysicalContinuityViolation::ShortCircuit { net_names, .. } => {
            assert_eq!(net_names.len(), 2);
            assert!(net_names.contains(&"VCC".into()));
            assert!(net_names.contains(&"GND".into()));
        }
        _ => panic!("Expected ShortCircuit violation"),
    }
}

#[test]
fn test_via_bridge() {
    // Two pours on different layers, connected by via
    let pours = vec![
        PourMetadata {
            name: "pour1".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 1000,
                max_y: 1000,
                max_z: 100,
            }),
        },
        PourMetadata {
            name: "pour2".into(),
            material_name: "Copper".into(),
            net: Some("VCC".into()),
            area_nm2: 1000,
            bbox: Some(BoundingBox {
                min_x: 0,
                min_y: 0,
                min_z: 100,
                max_x: 1000,
                max_y: 1000,
                max_z: 200,
            }),
        },
    ];

    let contacts = vec![ContactMetadata {
        name: "via1".into(),
        material_name: "Copper".into(),
        net: Some("VCC".into()),
        bbox: Some(BoundingBox {
            min_x: 400,
            min_y: 400,
            min_z: 0,
            max_x: 600,
            max_y: 600,
            max_z: 200,
        }),
    }];

    let checker =
        PhysicalContinuityChecker::new(100, &pours, &contacts, &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    // Should have 1 island (all connected via the via), 0 violations
    assert_eq!(islands.len(), 1);
    assert_eq!(violations.len(), 0);
}
