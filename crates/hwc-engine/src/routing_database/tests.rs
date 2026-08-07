//! Unit tests for the hierarchical routing database.

use super::*;
use crate::geometry::{Point3D, TraceSegment};
use crate::material::MaterialId;
use crate::netlist::NetId;

/// Build a simple horizontal trace segment for test fixtures.
fn segment(x0: i64, y0: i64, x1: i64, y1: i64) -> TraceSegment {
    TraceSegment::new(
        Point3D::new(x0, y0, 0),
        Point3D::new(x1, y1, 0),
        200,
        MaterialId(1),
    )
}

#[test]
fn test_empty_database() {
    let db = HierarchicalRoutingDatabase::new();
    let stats = db.get_statistics();

    assert_eq!(stats.total_child_segments, 0);
    assert_eq!(stats.total_parent_segments, 0);
    assert_eq!(stats.unique_child_instances, 0);
}

#[test]
fn test_register_child_routes() {
    let mut db = HierarchicalRoutingDatabase::new();

    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(0, 0, 100, 100)],
    );

    let stats = db.get_statistics();
    assert_eq!(stats.total_child_segments, 1);
    assert_eq!(stats.unique_child_instances, 1);
}

#[test]
fn test_hierarchical_validation_pass() {
    let mut db = HierarchicalRoutingDatabase::new();

    // Single instance - should pass
    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(0, 0, 100, 100)],
    );

    let result = db.validate_hierarchical_connectivity();
    assert!(result.is_ok());
}

#[test]
fn test_hierarchical_validation_fail() {
    let mut db = HierarchicalRoutingDatabase::new();

    // Same net in two instances - should fail without parent route
    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(0, 0, 100, 100)],
    );

    db.register_child_routes(
        "NMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(200, 200, 300, 300)],
    );

    let result = db.validate_hierarchical_connectivity();
    assert!(result.is_err());

    if let Err(errors) = result {
        assert_eq!(errors.len(), 1);
    }
}

#[test]
fn test_validate_matches_hierarchical_validation() {
    let mut db = HierarchicalRoutingDatabase::new();

    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(0, 0, 100, 100)],
    );
    db.register_child_routes(
        "NMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(200, 200, 300, 300)],
    );

    assert!(db.validate().is_err());
    assert!(db.validate_hierarchical_connectivity().is_err());
}

#[test]
fn test_has_routing_and_child_instances() {
    let mut db = HierarchicalRoutingDatabase::new();

    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(7),
        "VDD".into(),
        vec![segment(0, 0, 100, 0)],
    );

    assert!(db.has_routing_for_net(NetId::new(7)));
    assert!(!db.has_routing_for_net(NetId::new(8)));

    let instances = db.get_child_instances();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0], "PMOS_Inst");
}

#[test]
fn test_connectivity_view_includes_child_segments() {
    let mut db = HierarchicalRoutingDatabase::new();

    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(0, 0, 100, 100), segment(100, 100, 200, 100)],
    );

    let view = db.get_connectivity_view();
    assert_eq!(view.len(), 2);
    assert!(view
        .iter()
        .all(|s| matches!(s.source, RouteSource::ChildInstance { .. })));
}

#[test]
fn test_clear_resets_database() {
    let mut db = HierarchicalRoutingDatabase::new();

    db.register_child_routes(
        "PMOS_Inst".into(),
        NetId::new(1),
        "VDD".into(),
        vec![segment(0, 0, 100, 100)],
    );

    db.clear();

    let stats = db.get_statistics();
    assert_eq!(stats.total_child_segments, 0);
    assert_eq!(stats.unique_child_instances, 0);
    assert!(!db.has_routing_for_net(NetId::new(1)));
}
