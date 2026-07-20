use sha2::{Digest, Sha256};

#[inline]
pub fn compute_fingerprint(
    component_bounds: &[(i64, i64, i64, i64)],
    rules_hash: &[u8; 32],
    stackup_hash: &[u8; 32],
    router_version: u32,
) -> [u8; 32] {
    let mut hasher = Sha256::new();

    for &(min_x, min_y, max_x, max_y) in component_bounds {
        hasher.update(min_x.to_le_bytes());
        hasher.update(min_y.to_le_bytes());
        hasher.update(max_x.to_le_bytes());
        hasher.update(max_y.to_le_bytes());
    }

    hasher.update(rules_hash);
    hasher.update(stackup_hash);
    hasher.update(router_version.to_le_bytes());

    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

pub fn compute_fingerprint_from_space(space: &crate::space::HardwareSpace) -> [u8; 32] {
    let mut component_bounds: Vec<(i64, i64, i64, i64)> = space
        .component_bboxes
        .values()
        .map(|bbox| (bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y))
        .collect();
    component_bounds.sort();

    let rules_hash = {
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}", space.fabrication_constraints).as_bytes());
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    };
    let stackup_hash = {
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}", space.substrate_material_id).as_bytes());
        hasher.update(format!("{:?}", space.dimensions).as_bytes());
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    };
    let router_version = {
        let mut hasher = Sha256::new();
        hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
        let result = hasher.finalize();
        u32::from_le_bytes([result[0], result[1], result[2], result[3]])
    };

    compute_fingerprint(
        &component_bounds,
        &rules_hash,
        &stackup_hash,
        router_version,
    )
}
