//! Substrate layer placement.

use crate::geometry::{BoundingBox, Point3D};
use crate::space::VoxelSize;
use crate::voxel::MaterialId;
use crate::voxel_grid::VoxelGrid;

use super::error::PlacementError;

/// Place a substrate layer in the voxel grid.
///
/// # Arguments
/// * `grid` - Voxel grid to place substrate in
/// * `voxel_size` - Size of each voxel in nanometers
/// * `material_id` - Substrate material ID from MaterialRegistry
/// * `start` - Starting position in nanometers
/// * `end` - Ending position in nanometers
/// * `net_id` - Net ID for connectivity checking (0 = unassigned)
///
/// # Returns
/// Ok if successful, error if invalid region
pub(super) fn place_substrate(
    grid: &mut VoxelGrid,
    voxel_size: &VoxelSize,
    material_id: MaterialId,
    start: Point3D,
    end: Point3D,
    net_id: u32,
) -> Result<(), PlacementError> {
    place_substrate_with_cutouts(grid, voxel_size, material_id, start, end, net_id, &[])
}

/// Place a cylindrical substrate layer in the voxel grid (v0.1.6).
pub(super) fn place_cylinder_substrate(
    grid: &mut VoxelGrid,
    material_id: MaterialId,
    start: Point3D,
    end: Point3D,
    net_id: u32,
    diameter: i64,
) -> Result<(), PlacementError> {
    let bbox = BoundingBox::from_points(start, end);

    if (bbox.max.x - bbox.min.x).abs() <= 0
        || (bbox.max.y - bbox.min.y).abs() <= 0
        || (bbox.max.z - bbox.min.z).abs() <= 0
    {
        return Err(PlacementError::InvalidSubstrateRegion { start, end });
    }

    // Default to 16 segments for 3D visualization
    grid.add_cylinder_substrate_layer(material_id, net_id, bbox, diameter, 16, 0);

    Ok(())
}

/// Place a substrate layer with cutouts (mounting holes, edge cuts, etc.).
///
/// # Arguments
/// * `grid` - Voxel grid to place substrate in
/// * `voxel_size` - Size of each voxel in nanometers
/// * `material_id` - Substrate material ID from MaterialRegistry
/// * `start` - Starting position in nanometers
/// * `end` - Ending position in nanometers
/// * `net_id` - Net ID for connectivity checking (0 = unassigned)
/// * `cutouts` - Bounding boxes defining holes in the substrate
///
/// # Returns
/// Ok if successful, error if invalid region
pub(super) fn place_substrate_with_cutouts(
    grid: &mut VoxelGrid,
    voxel_size: &VoxelSize,
    material_id: MaterialId,
    start: Point3D,
    end: Point3D,
    net_id: u32,
    cutouts: &[BoundingBox],
) -> Result<(), PlacementError> {
    // Create bounding box from start and end points
    // BoundingBox::from_points handles coordinate ordering automatically
    let bbox = BoundingBox::from_points(start, end);

    // Validate that the bounding box has positive volume
    let width = (bbox.max.x - bbox.min.x).abs();
    let height = (bbox.max.y - bbox.min.y).abs();
    let depth = (bbox.max.z - bbox.min.z).abs();

    if width <= 0 || height <= 0 || depth <= 0 {
        return Err(PlacementError::InvalidSubstrateRegion { start, end });
    }

    // Fill the region with substrate material (with cutouts)
    // Pass through the net ID for connectivity checking
    grid.fill_substrate_with_cutouts(&bbox, voxel_size, material_id, net_id, cutouts);

    Ok(())
}

/// Place multiple substrate layers.
///
/// Useful for multi-layer PCBs or silicon chips with multiple dielectric layers.
pub(super) fn place_substrate_layers(
    grid: &mut VoxelGrid,
    voxel_size: &VoxelSize,
    layers: &[(MaterialId, Point3D, Point3D)],
) -> Result<(), PlacementError> {
    for (material_id, start, end) in layers {
        place_substrate(grid, voxel_size, *material_id, *start, *end, 0)?;
    }
    Ok(())
}
