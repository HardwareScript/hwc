use super::*;
use crate::connectivity::{BoundingBox, ContactMetadata, PourMetadata, SubstrateLayerMetadata};

fn test_sub(x: i64, y: i64, z: i64, material: u8, net_name: &str) -> SubstrateLayerMetadata {
    SubstrateLayerMetadata {
        material,
        net: 1,
        net_name: Some(net_name.into()),
        bbox: BoundingBox {
            min_x: x,
            min_y: y,
            min_z: z,
            max_x: x + 1000,
            max_y: y + 1000,
            max_z: z + 100,
        },
    }
}

#[test]
fn test_simple_connected_net() {
    let substrate_layers = vec![
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 0, min_y: 0, min_z: 0, max_x: 1000, max_y: 1000, max_z: 100 },
        },
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 500, min_y: 0, min_z: 0, max_x: 1500, max_y: 1000, max_z: 100 },
        },
    ];

    let checker = PhysicalContinuityChecker::new(&substrate_layers, &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    assert_eq!(islands.len(), 1);
    assert_eq!(violations.len(), 0);
}

#[test]
fn test_disconnected_net() {
    let substrate_layers = vec![
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 0, min_y: 0, min_z: 0, max_x: 1000, max_y: 1000, max_z: 100 },
        },
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 2000, min_y: 0, min_z: 0, max_x: 3000, max_y: 1000, max_z: 100 },
        },
    ];

    let checker = PhysicalContinuityChecker::new(&substrate_layers, &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    assert_eq!(islands.len(), 2);
    assert_eq!(violations.len(), 1);

    match &violations[0] {
        PhysicalContinuityViolation::DisconnectedNet { net_name, island_count, .. } => {
            assert_eq!(net_name, "VCC");
            assert_eq!(*island_count, 2);
        }
        _ => panic!("Expected DisconnectedNet violation"),
    }
}

#[test]
fn test_short_circuit() {
    let substrate_layers = vec![
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 0, min_y: 0, min_z: 0, max_x: 1000, max_y: 1000, max_z: 100 },
        },
        SubstrateLayerMetadata {
            material: 2,
            net: 2,
            net_name: Some("GND".into()),
            bbox: BoundingBox { min_x: 500, min_y: 0, min_z: 0, max_x: 1500, max_y: 1000, max_z: 100 },
        },
    ];

    let checker = PhysicalContinuityChecker::new(&substrate_layers, &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

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
fn test_route_segment_connects_substrate_layers() {
    let substrate_layers = vec![
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VIN".into()),
            bbox: BoundingBox { min_x: 4900, min_y: 4000, min_z: 300000, max_x: 5100, max_y: 5000, max_z: 300200 },
        },
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VIN".into()),
            bbox: BoundingBox { min_x: 4900, min_y: 12000, min_z: 300000, max_x: 5100, max_y: 13000, max_z: 300200 },
        },
    ];

    let route_segments = vec![
        RouteSegmentMetadata {
            net: 1,
            net_name: Some("VIN".into()),
            material: 2,
            bbox: BoundingBox { min_x: 4900, min_y: 5000, min_z: 300000, max_x: 5100, max_y: 12000, max_z: 300200 },
        },
    ];

    let checker = PhysicalContinuityChecker::new(&substrate_layers, &route_segments, &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    assert_eq!(islands.len(), 1);
    assert_eq!(violations.len(), 0);
}

#[test]
fn test_via_bridge() {
    let substrate_layers = vec![
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 0, min_y: 0, min_z: 0, max_x: 1000, max_y: 1000, max_z: 100 },
        },
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 400, min_y: 400, min_z: 0, max_x: 600, max_y: 600, max_z: 200 },
        },
        SubstrateLayerMetadata {
            material: 2,
            net: 1,
            net_name: Some("VCC".into()),
            bbox: BoundingBox { min_x: 0, min_y: 0, min_z: 100, max_x: 1000, max_y: 1000, max_z: 200 },
        },
    ];

    let checker = PhysicalContinuityChecker::new(&substrate_layers, &[], &[], Default::default());
    let islands = checker.build_conductive_islands(None);
    let bindings = checker.bind_nets_to_islands(&islands);
    let violations = checker.validate_continuity(&islands, &bindings, false);

    assert_eq!(islands.len(), 1);
    assert_eq!(violations.len(), 0);
}
