//! Physical Z helpers for export pipelines (v0.1.7).
//!
//! Shared physical-Z helpers for export pipelines. All internal logic uses nanometers.

use hwc_compiler::ir::stackup_manager::StackupManager;
use hwc_engine::HardwareSpace;

/// Convert nanometers to millimeters for human-readable labels.
pub fn z_mm(z_nm: i64) -> f64 {
    z_nm as f64 / 1_000_000.0
}

/// Board Z extent in nanometers: bottom face to top face of the board.
pub fn board_z_extent(space: &HardwareSpace) -> (i64, i64) {
    let board_min_z_nm = 0;
    // v0.1.8: Use physical depth from dimensions (voxel_size.z_nm was removed)
    let board_max_z_nm = space.dimensions.depth_nm;
    (board_min_z_nm, board_max_z_nm)
}

/// Derive a 0-based grid slab index from physical Z (Excellon / Gerber display only).
pub fn grid_index_from_z(z_nm: i64, voxel_z_nm: i64) -> u8 {
    let voxel_z_nm = voxel_z_nm.max(1);
    ((z_nm / voxel_z_nm).max(0)) as u8
}

/// DXF layer name from physical elevation (mm) and material.
/// v0.1.7: Maps physical slices to logical EDA layers (Top, Bottom, Inner, Drill).
pub fn dxf_layer_name(
    z_center_nm: i64,
    material: &str,
    board_min_z: i64,
    board_max_z: i64,
    stackup_manager: Option<&StackupManager>,
) -> String {
    let material_lower = material.to_lowercase();

    // 1. Global DRILL Layer
    if material_lower == "void" || material_lower == "air" {
        return "DRILL".to_string();
    }

    // 2. Query StackupManager for semantic layer name (v0.1.7 Fix)
    if let Some(mgr) = stackup_manager {
        if let Some(layer_name) = mgr.get_layer_name_at_z(z_center_nm) {
            return format!("{}_{}", layer_name.to_uppercase(), material.to_uppercase());
        }
    }

    // 3. Map Copper/Conductive layers to logical stackup names
    if material_lower.contains("copper")
        || material_lower.contains("solder")
        || material_lower.contains("gold")
    {
        let tol = 1000; // 1um tolerance
        if (z_center_nm - board_max_z).abs() <= tol || z_center_nm > board_max_z - 50000 {
            return format!("TOP_{}", material.to_uppercase());
        }
        if (z_center_nm - board_min_z).abs() <= tol || z_center_nm < board_min_z + 50000 {
            return format!("BOTTOM_{}", material.to_uppercase());
        }

        // Inner layers: calculate index
        let total_depth = board_max_z - board_min_z;
        if total_depth > 0 {
            let relative_pos = (z_center_nm - board_min_z) as f64 / total_depth as f64;
            let inner_idx = (relative_pos * 10.0).round() as i32; // Simplified inner mapping
            return format!("INNER{}_{}", inner_idx, material.to_uppercase());
        }
    }

    // Fallback for substrate and other materials
    format!("Z{:.4}mm_{}", z_mm(z_center_nm), material)
}

/// True when `z_nm` lies on the given board face within half a voxel slab.
pub fn is_on_board_face(z_nm: i64, face_z_nm: i64, voxel_z_nm: i64) -> bool {
    let voxel_z_nm = voxel_z_nm.max(1);
    (z_nm - face_z_nm).abs() <= voxel_z_nm / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_engine::{Dimensions, HardwareSpace, SpaceView};

    fn test_space(z_layers: usize, voxel_z_mm: i64) -> HardwareSpace {
        let voxel_z_nm = voxel_z_mm * 1_000_000;
        HardwareSpace::new(
            "test".into(),
            Dimensions {
                width_nm: 10_000_000_000,
                height_nm: 10_000_000_000,
                depth_nm: z_layers as i64 * voxel_z_nm,
            },
            0,
            hwc_engine::MaterialRegistry::new(),
            SpaceView::Horizontal,
            voxel_z_nm,
        )
    }

    #[test]
    fn board_z_extent_matches_grid() {
        let space = test_space(4, 1);
        let (min, max) = board_z_extent(&space);
        assert_eq!(min, 0);
        assert_eq!(max, 4_000_000); // z_layers(4) * voxel_z_nm(1mm=1_000_000nm) = 4_000_000nm
    }

    #[test]
    fn grid_index_from_physical_z() {
        assert_eq!(grid_index_from_z(2_500_000, 1_000_000), 2);
    }

    #[test]
    fn dxf_layer_name_uses_mm() {
        assert_eq!(
            dxf_layer_name(350_000_000, "Copper", 0, 1_000_000_000, None),
            "INNER4_COPPER"
        );
    }
}

