//! Physical Z helpers for export pipelines (v0.1.7).
//!
//! Shared physical-Z helpers for export pipelines. All internal logic uses nanometers.

use hwc_engine::HardwareSpace;

/// Convert nanometers to millimeters for human-readable labels.
pub fn z_mm(z_nm: i64) -> f64 {
    z_nm as f64 / 1_000_000.0
}

/// Board Z extent in nanometers: bottom face to top face of the voxel grid.
pub fn board_z_extent(space: &HardwareSpace) -> (i64, i64) {
    let voxel_z_nm = space.voxel_size.z_nm.max(1);
    let board_min_z_nm = 0;
    // v0.1.7 FIXED: Board max Z is the product of layers and voxel size, not (layers-1)
    let board_max_z_nm = (space.grid.z_layers as i64) * voxel_z_nm;
    (board_min_z_nm, board_max_z_nm)
}

/// Derive a 0-based grid slab index from physical Z (Excellon / Gerber display only).
pub fn grid_index_from_z(z_nm: i64, voxel_z_nm: i64) -> u8 {
    let voxel_z_nm = voxel_z_nm.max(1);
    ((z_nm / voxel_z_nm).max(0)) as u8
}

/// DXF layer name from physical elevation (mm) and material.
pub fn dxf_layer_name(z_center_nm: i64, material: &str) -> String {
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
    use hwc_engine::{Dimensions, GridCells, HardwareSpace, SpaceView};

    fn test_space(z_layers: usize, voxel_z_mm: i64) -> HardwareSpace {
        let voxel_z_nm = voxel_z_mm * 1_000_000;
        HardwareSpace::new(
            "test".into(),
            Dimensions {
                width_nm: 10_000_000_000,
                height_nm: 10_000_000_000,
                depth_nm: z_layers as i64 * voxel_z_nm,
            },
            GridCells {
                x_cols: 10,
                y_rows: 10,
                z_layers,
            },
            0,
            hwc_engine::MaterialRegistry::new(),
            SpaceView::Horizontal,
        )
    }

    #[test]
    fn board_z_extent_matches_grid() {
        let space = test_space(4, 1);
        let (min, max) = board_z_extent(&space);
        assert_eq!(min, 0);
        assert_eq!(max, 3_000_000);
    }

    #[test]
    fn grid_index_from_physical_z() {
        assert_eq!(grid_index_from_z(2_500_000, 1_000_000), 2);
    }

    #[test]
    fn dxf_layer_name_uses_mm() {
        assert_eq!(dxf_layer_name(350_000_000, "Copper"), "Z0.3500mm_Copper");
    }
}
