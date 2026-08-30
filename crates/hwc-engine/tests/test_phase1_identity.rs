//! Phase 1 Verification Tests: Merkle Path Identity, Registry & Base Silicon Lock

use compact_str::CompactString;
use hwc_engine::entity_graph::{
    BaseSiliconLock, EntityId, HierarchicalPath, IdentityRegistry, PathSegment,
};
use rustc_hash::FxHashSet;

#[test]
fn test_span_independent_entity_id_hashing() {
    // 1. Construct hierarchical path
    let mut path1 = HierarchicalPath::root("Divider_Array");
    path1.push(PathSegment::SemanticKey(CompactString::new("chan_0")));

    let mut path2 = HierarchicalPath::root("Divider_Array");
    path2.push(PathSegment::SemanticKey(CompactString::new("chan_0")));

    // EntityId computation is invariant to source spans/comments/whitespace
    let id1 = EntityId::compute(&path1, "Resistor", Some("R_DIV"), 0);
    let id2 = EntityId::compute(&path2, "Resistor", Some("R_DIV"), 0);

    assert_eq!(id1, id2);
    assert_ne!(id1.raw(), 0);

    // Changing semantic key changes EntityId deterministically
    let mut path3 = HierarchicalPath::root("Divider_Array");
    path3.push(PathSegment::SemanticKey(CompactString::new("chan_1")));
    let id3 = EntityId::compute(&path3, "Resistor", Some("R_DIV"), 0);
    assert_ne!(id1, id3);
}

#[test]
fn test_identity_registry_bi_directional_lookup() {
    let mut registry = IdentityRegistry::new();

    let mut path = HierarchicalPath::root("Top");
    path.push(PathSegment::Instance(CompactString::new("M_CELL")));
    path.push(PathSegment::SubCell(CompactString::new("via_0")));

    let id = EntityId::compute(&path, "Contact", None, 0);

    registry.register(id, &path);

    // O(1) forward lookup
    assert_eq!(
        registry.get_path(id).map(|s| s.as_str()),
        Some("Top.M_CELL.via_0")
    );

    // O(1) reverse lookup
    assert_eq!(registry.get_id("Top.M_CELL.via_0"), Some(id));
}

#[test]
fn test_base_silicon_lock_validation() {
    let mut frozen_ids = FxHashSet::default();
    let id_a = EntityId::new(0x1111);
    let id_b = EntityId::new(0x2222);
    frozen_ids.insert(id_a);

    let lock = BaseSiliconLock::new(
        0xDEADBEEF_CAFE_BABE,
        frozen_ids,
        vec![id_b],
        vec![CompactString::new("diff"), CompactString::new("poly")],
    );

    // Verify entity lock status
    assert!(lock.is_entity_locked(id_a));
    assert!(!lock.is_entity_locked(id_b));

    // Verify layer lock status
    assert!(lock.is_layer_locked("diff"));
    assert!(lock.is_layer_locked("poly"));
    assert!(!lock.is_layer_locked("metal1"));
}
