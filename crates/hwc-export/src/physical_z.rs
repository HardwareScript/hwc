//! Physical Z helpers for export pipelines (v0.1.7).
//!
//! Shared physical-Z helpers for export pipelines. All internal logic uses nanometers.

use hwc_engine::HardwareSpace;

/// Convert nanometers to millimeters for human-readable labels.
pub fn z_mm(z_nm: i64) -> f64 {
    z_nm as f64 / 1_000_000.0
}

/// Board Z extent in nanometers: bottom face to top face of the board.
pub fn board_z_extent(space: &HardwareSpace) -> (i64, i64) {
    let board_min_z_nm = 0;
    // v0.1.8: Use physical depth from dimensions
    let board_max_z_nm = space.dimensions.depth_nm;
    (board_min_z_nm, board_max_z_nm)
}

/// Derive a 0-based slab index from physical Z (Excellon / Gerber display only).
///
/// v0.2.1: This is retained only for coarse display-layer bucketing. Logical
/// via-to-layer classification should use [`via_layer_index`] against the
/// `StackupManager` instead, so the mapping is canonical rather than a grid
/// division.
pub fn grid_index_from_z(z_nm: i64, slab_z_nm: i64) -> u8 {
    let slab_z_nm = slab_z_nm.max(1);
    ((z_nm / slab_z_nm).max(0)) as u8
}

/// Canonical via-layer index from physical Z, resolved against the stackup layers
/// embedded in a `HardwareSpace` (its single source of truth for Z→layer mapping).
///
/// v0.2.1 (Bloat Purge Category 2): replaces grid-division heuristics for
/// logical layer classification with the canonical stackup ordering.
pub fn via_layer_index(space: &HardwareSpace, z_nm: i64) -> u8 {
    for (i, layer) in space.stackup_layers.iter().enumerate() {
        if z_nm >= layer.z_bottom && z_nm <= layer.z_top {
            return i as u8;
        }
    }
    // Outside any layer (e.g. on a board face) — fall back to coarse slab index.
    let slab = space.dimensions.depth_nm.max(1);
    ((z_nm / slab).max(0) as u8).min(space.stackup_layers.len().saturating_sub(1) as u8)
}

pub fn dxf_layer_name(
    z_center_nm: i64,
    material: &str,
    space: &HardwareSpace,
) -> Result<String, String> {
    let material_lower = material.to_lowercase();

    // 1. Global DRILL Layer
    if material_lower == "void" || material_lower == "air" {
        return Ok("DRILL".to_string());
    }

    // 2. Query space for semantic layer name
    if let Some(layer) = space.find_layer_at_z(z_center_nm) {
        return Ok(format!(
            "{}_{}",
            layer.name.to_uppercase(),
            material.to_uppercase()
        ));
    }

    Ok(format!("LAYER_{}", material.to_uppercase()))
}

/// True when `z_nm` lies on the given board face within half a slab.
pub fn is_on_board_face(z_nm: i64, face_z_nm: i64, slab_z_nm: i64) -> bool {
    let slab_z_nm = slab_z_nm.max(1);
    (z_nm - face_z_nm).abs() <= slab_z_nm / 2
}
