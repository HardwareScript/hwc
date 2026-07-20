use rustc_hash::FxHashMap;

pub(super) fn resolve_z_to_layer_index(
    z_nm: i64,
    entity_graph: &crate::geometry_router::entity_graph::EntityGraph,
) -> u16 {
    let mut z_to_layer: FxHashMap<i64, u16> = FxHashMap::default();
    let mut next_idx: u16 = 0;

    for layer in entity_graph.get_substrate_layers() {
        let z_min = layer.bbox.min.z;
        let z_max = layer.bbox.max.z;
        let z_mid = (z_min + z_max) / 2;

        if let std::collections::hash_map::Entry::Vacant(e) = z_to_layer.entry(z_mid) {
            e.insert(next_idx);
            next_idx = next_idx.wrapping_add(1);
        }
    }

    let mut best_layer: u16 = 0;
    let mut best_dist: i64 = i64::MAX;
    for (layer_z, layer_idx) in &z_to_layer {
        let dist = (z_nm - layer_z).abs();
        if dist < best_dist {
            best_dist = dist;
            best_layer = *layer_idx;
        }
    }
    best_layer
}

pub fn build_layer_z_map(
    entity_graph: &crate::geometry_router::entity_graph::EntityGraph,
) -> Vec<(u16, i64)> {
    let mut z_to_layer: FxHashMap<i64, u16> = FxHashMap::default();
    let mut next_idx: u16 = 0;

    for layer in entity_graph.get_substrate_layers() {
        let z_min = layer.bbox.min.z;
        let z_max = layer.bbox.max.z;
        let z_mid = (z_min + z_max) / 2;

        if let std::collections::hash_map::Entry::Vacant(e) = z_to_layer.entry(z_mid) {
            e.insert(next_idx);
            next_idx = next_idx.wrapping_add(1);
        }
    }

    z_to_layer.into_iter().map(|(z, idx)| (idx, z)).collect()
}
