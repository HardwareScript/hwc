use hwc_engine::stackup::StackupManager;
use hwc_engine::EntityGraph;
use hwc_router::traits::{RouterEngine, RoutingTask};
use hwc_router::types::PinAccessMap;
use hwc_router::TriHybridRouter;

#[test]
fn test_tri_hybrid_router_pipeline() {
    let mut entity_graph = EntityGraph::new();
    // Add two pins on a net
    entity_graph.add_component_pin(
        1000,
        1000,
        0,
        "comp_a".into(),
        "pin_1".into(),
        Some("net_0".into()),
    );
    entity_graph.add_component_pin(
        5000,
        5000,
        0,
        "comp_b".into(),
        "pin_1".into(),
        Some("net_0".into()),
    );

    let stackup = StackupManager::new(vec![]);
    let pin_map = PinAccessMap::new();

    let task = RoutingTask {
        entity_graph: &entity_graph,
        stackup: &stackup,
        pin_access_map: &pin_map,
    };

    let mut router = TriHybridRouter::default();
    assert_eq!(
        router.name(),
        "HardwareScript Tri-Hybrid Physical Router (v0.3.1)"
    );

    let result = router.route(&task);
    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(!output.traces.is_empty());
}
