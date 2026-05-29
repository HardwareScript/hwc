//! Coordinate conversion utilities

use super::grid::VoxelGrid;
use crate::geometry::Point3D;
use crate::space::VoxelSize;

impl VoxelGrid {
    /// Convert nanometer coordinates to voxel indices.
    ///
    /// # Arguments
    /// * `point` - Position in nanometers (Point3D with x, y, z)
    /// * `voxel_size` - Size of each voxel in nanometers
    ///
    /// # Returns
    /// Voxel indices (x, y, z) - note the order matches grid indexing
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, Point3D, VoxelSize, Dimensions, GridCells};
    /// let dims = Dimensions::from_mm(50.0, 50.0, 4.0);
    /// let grid_cells = GridCells::new(500, 500, 4);
    /// let voxel_size = VoxelSize::from_dimensions(dims, grid_cells);
    ///
    /// let point = Point3D::new(500_000, 500_000, 1_000_000); // (x, y, z) = 0.5mm, 0.5mm, 1mm
    /// let (x, y, z) = VoxelGrid::nm_to_voxel(point, &voxel_size);
    /// assert_eq!(x, 5);  // 0.5mm / 0.1mm per voxel = 5
    /// assert_eq!(y, 5);  // 0.5mm / 0.1mm per voxel = 5
    /// assert_eq!(z, 1);  // 1mm / 1mm per layer = 1
    /// ```
    #[inline]
    pub fn nm_to_voxel(point: Point3D, voxel_size: &VoxelSize) -> (usize, usize, usize) {
        let x = (point.x / voxel_size.x_nm).max(0) as usize;
        let y = (point.y / voxel_size.y_nm).max(0) as usize;
        let z = (point.z / voxel_size.z_nm).max(0) as usize;
        (x, y, z)
    }

    /// Convert voxel indices to nanometer coordinates (center of voxel).
    ///
    /// # Arguments
    /// * `x`, `y`, `z` - Voxel indices
    /// * `voxel_size` - Size of each voxel in nanometers
    ///
    /// # Returns
    /// Position in nanometers (Point3D with x, y, z)
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::{VoxelGrid, Point3D, VoxelSize, Dimensions, GridCells};
    /// let dims = Dimensions::from_mm(50.0, 50.0, 4.0);
    /// let grid_cells = GridCells::new(500, 500, 4);
    /// let voxel_size = VoxelSize::from_dimensions(dims, grid_cells);
    ///
    /// let point = VoxelGrid::voxel_to_nm(5, 5, 1, &voxel_size);
    /// // Returns center of voxel
    /// assert_eq!(point.x, 550_000);   // 5.5 * 0.1mm = 0.55mm
    /// assert_eq!(point.y, 550_000);   // 5.5 * 0.1mm = 0.55mm
    /// assert_eq!(point.z, 1_500_000); // 1.5 * 1mm = 1.5mm
    /// ```
    #[inline]
    pub fn voxel_to_nm(x: usize, y: usize, z: usize, voxel_size: &VoxelSize) -> Point3D {
        // Return center of voxel
        Point3D::new(
            x as i64 * voxel_size.x_nm + voxel_size.x_nm / 2,
            y as i64 * voxel_size.y_nm + voxel_size.y_nm / 2,
            z as i64 * voxel_size.z_nm + voxel_size.z_nm / 2,
        )
    }
}
