use compact_str::CompactString;
use hwc_engine::geometry::Point3D;
use hwc_engine::netlist::NetId;
use hwc_engine::EntityGraph;
use hwc_router::detailed::{
    validate_wire_segment, DrcRules, SparseGridDetailedRouter, TimingSlackMap,
};
use hwc_router::types::{AccessPoint, AssignedTrackSegment, PinAccessMap};

#[test]
fn test_drc_lookahead_rules() {
    let rules = DrcRules::default();

    // Wire width >= min_wire_width (140 nm = 140_000 pm)
    let p1 = Point3D::new(0, 0, 0);
    let p2 = Point3D::new(1_000_000, 0, 0);

    assert!(validate_wire_segment(p1, p2, 140_000, &rules));
    assert!(!validate_wire_segment(p1, p2, 100_000, &rules)); // too narrow
}

#[test]
fn test_timing_rrr_criticality() {
    let mut timing_map = TimingSlackMap::new();
    let net_crit = NetId::new(1);
    let net_relaxed = NetId::new(2);

    // Negative slack = critical setup path
    timing_map.set_slack(net_crit, -250.0);
    timing_map.set_slack(net_relaxed, 150.0);

    let crit_weight = timing_map.get_criticality(net_crit);
    let relaxed_weight = timing_map.get_criticality(net_relaxed);

    assert!(crit_weight > 1.0);
    assert_eq!(relaxed_weight, 1.0);
}

#[test]
fn test_sparse_grid_detailed_router_pin_bridging() {
    let router = SparseGridDetailedRouter::default();
    let entity_graph = EntityGraph::new();

    let mut pin_map = PinAccessMap::new();
    let pin_ap = AccessPoint {
        point: Point3D::new(500_000, 100_000, 0),
        layer_idx: 0,
        score: 800,
        is_preferred: true,
    };
    pin_map.insert(0, CompactString::new("A"), vec![pin_ap]);

    let assigned_tracks = vec![AssignedTrackSegment {
        net_id: NetId::new(0),
        layer_idx: 1,
        track_index: 2,
        start_coord_pm: 0,
        end_coord_pm: 1_000_000,
        fixed_axis_coord_pm: 460_000,
    }];

    let output = router.route_detailed(&entity_graph, &pin_map, &assigned_tracks);
    assert!(!output.traces.is_empty());
    assert!(!output.vias.is_empty());
}
