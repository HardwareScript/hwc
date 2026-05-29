//! Component Placement Operations (Sprint 2: Sparse Architecture)
//!
//! This module implements SPARSE component placement - components are stored as
//! metadata, NOT filled into voxels. This avoids the "Density Bomb" problem.
//!
//! ## Architecture: Sparse Component Storage
//!
//! Components are stored in `VoxelGrid::component_metadata: Vec<ComponentMetadata>`.
//! When the router/DRC needs to check a voxel, it:
//! 1. Checks substrate layers (O(layers))
//! 2. Checks component metadata (O(components))
//! 3. Checks sparse voxel chunks (O(1))
//!
//! This is the same pattern as SubstrateLayer - sparse bounding box storage.

use crate::geometry::Point3D as StampPoint3D;
use crate::voxel_grid::grid::core::VoxelGrid;
use crate::voxel_grid::substrate_layers::{ComponentMetadata, TSVParams};
use compact_str::CompactString;

/// Error types for component placement
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementError {
    /// Component would be placed outside grid bounds
    OutOfBounds {
        component: CompactString,
        position: StampPoint3D,
        grid_size: (usize, usize, usize),
    },
    /// Component overlaps with existing geometry
    Collision {
        component: CompactString,
        position: StampPoint3D,
        colliding_component: CompactString,
    },
}

impl std::fmt::Display for PlacementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlacementError::OutOfBounds {
                component,
                position,
                grid_size,
            } => write!(
                f,
                "Component '{}' at position ({}, {}, {}) would exceed grid bounds ({}, {}, {})",
                component,
                position.x,
                position.y,
                position.z,
                grid_size.0,
                grid_size.1,
                grid_size.2
            ),
            PlacementError::Collision {
                component,
                position,
                colliding_component,
            } => write!(
                f,
                "Component '{}' at position ({}, {}, {}) collides with '{}'",
                component, position.x, position.y, position.z, colliding_component
            ),
        }
    }
}

impl std::error::Error for PlacementError {}

impl VoxelGrid {
    /// Update the net assignment for a specific pin in the metadata.
    ///
    /// This is used to synchronize the physical voxel metadata with the logical netlist
    /// when a route is registered.
    pub fn set_pin_net(
        &mut self,
        component_name: &str,
        pin_name: &str,
        net_name: &str,
    ) {
        // Update physical continuity pins (v0.1.6)
        // These are used by the netlist extractor and physics validator.
        for pin in &mut self.component_pins {
            if pin.component_name == component_name && pin.pin_name == pin_name {
                pin.net = Some(net_name.into());
            }
        }
    }

    /// Place a component using SPARSE metadata (no voxel filling)
    ///
    /// This is the God-Tier approach: store component as metadata, not voxels.
    /// The router/DRC will check component bounding boxes when needed.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Component metadata with bounding box and materials
    /// * `check_collision` - Whether to check for collisions before placement
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Component metadata stored successfully
    /// * `Err(PlacementError)` - Placement failed (out of bounds or collision)
    ///
    /// # Performance
    ///
    /// - O(1) - just add metadata to vector
    /// - No voxel filling, no chunk allocation
    /// - Target: < 0.1ms per component
    pub fn place_component_sparse(
        &mut self,
        metadata: ComponentMetadata,
        check_collision: bool,
    ) -> Result<(), PlacementError> {
        // 1. Validate bounds (convert nm to voxel coordinates for check)
        let (gx, gy, gz) = self.size();
        let grid_max_nm = (
            gx as i64 * self.voxel_size.x_nm,
            gy as i64 * self.voxel_size.y_nm,
            gz as i64 * self.voxel_size.z_nm,
        );

        if metadata.bbox.max.x > grid_max_nm.0
            || metadata.bbox.max.y > grid_max_nm.1
            || metadata.bbox.max.z > grid_max_nm.2
        {
            return Err(PlacementError::OutOfBounds {
                component: metadata.name.clone(),
                position: metadata.bbox.min,
                grid_size: self.size(),
            });
        }

        // 2. Check for collisions if requested
        if check_collision {
            // Check against existing components
            for existing in &self.component_metadata {
                // Simple AABB collision check
                if metadata.bbox.min.x < existing.bbox.max.x &&
                   metadata.bbox.max.x > existing.bbox.min.x &&
                   metadata.bbox.min.y < existing.bbox.max.y &&
                   metadata.bbox.max.y > existing.bbox.min.y &&
                   metadata.bbox.min.z < existing.bbox.max.z &&
                   metadata.bbox.max.z > existing.bbox.min.z {
                    
                    return Err(PlacementError::Collision {
                        component: metadata.name.clone(),
                        position: metadata.bbox.min,
                        colliding_component: existing.name.clone(),
                    });
                }
            }
        }

        // 3. Store component metadata (sparse - no voxel filling!)
        self.component_metadata.push(metadata);

        Ok(())
    }

    /// Stamp a cylinder (disc) of material at a specific Z-layer
    ///
    /// Used for drawing compound vias and bridges, where different Z-layers
    /// might have different materials (e.g., a thin Silicide interface layer
    /// followed by a Tungsten fill).
    ///
    /// # Arguments
    ///
    /// * `center_x`, `center_y` - Center coordinates in voxels
    /// * `z` - The Z-layer to stamp on
    /// * `radius_voxels` - Radius of the cylinder in voxels
    /// * `material` - The material to stamp
    /// * `handle` - The net handle
    pub fn stamp_cylinder(
        &self,
        center_x: usize,
        center_y: usize,
        z: usize,
        radius_voxels: usize,
        material: crate::voxel_grid::chunk::MaterialId,
        handle: crate::netlist::NetHandle,
    ) {
        if !self.in_bounds(center_x, center_y, z) {
            return;
        }

        let r_sq = (radius_voxels * radius_voxels) as isize;
        let cx = center_x as isize;
        let cy = center_y as isize;
        let r = radius_voxels as isize;

        for dx in -r..=r {
            for dy in -r..=r {
                if dx * dx + dy * dy <= r_sq {
                    let px = cx + dx;
                    let py = cy + dy;
                    
                    if px >= 0 && py >= 0 {
                        let ux = px as usize;
                        let uy = py as usize;
                        if self.in_bounds(ux, uy, z) {
                            self.set_occupied(ux, uy, z, material, handle);
                        }
                    }
                }
            }
        }
    }

    /// Stamp a TSV (Through-Silicon Via) with concentric cylindrical layers.
    ///
    /// This implements the "Final Boss" of 3D connectivity:
    /// 1. Outer insulator liner (prevents substrate shorts)
    /// 2. Optional bridge layer (for adhesion/ohmic contact)
    /// 3. Conductive fill core (Copper/Tungsten)
    ///
    /// # Arguments
    ///
    /// * `center_x_nm`, `center_y_nm` - Center coordinates in nanometers
    /// * `z_start_nm`, `z_end_nm` - Vertical span in nanometers
    /// * `params` - TSV parameters (diameter, materials)
    /// * `handle` - The net handle for the conductive parts
    pub fn stamp_tsv(
        &self,
        center_x_nm: i64,
        center_y_nm: i64,
        z_start_nm: i64,
        z_end_nm: i64,
        params: TSVParams,
        handle: crate::netlist::NetHandle,
    ) {
        // Convert nm to voxel coordinates
        let vx = (center_x_nm / self.voxel_size.x_nm) as usize;
        let vy = (center_y_nm / self.voxel_size.y_nm) as usize;
        let vz_start = (z_start_nm / self.voxel_size.z_nm) as usize;
        let vz_end = (z_end_nm / self.voxel_size.z_nm) as usize;

        let total_radius_nm = params.diameter_nm / 2;
        let liner_thickness_nm = params.stack.liner_thickness_nm;
        let bridge_thickness_nm = params.stack.bridge_thickness_nm;

        let total_radius_voxels = (total_radius_nm / self.voxel_size.x_nm) as usize;
        let bridge_radius_nm = total_radius_nm - liner_thickness_nm;
        let fill_radius_nm = bridge_radius_nm - bridge_thickness_nm;

        let bridge_radius_voxels = (bridge_radius_nm / self.voxel_size.x_nm) as usize;
        let fill_radius_voxels = (fill_radius_nm / self.voxel_size.x_nm) as usize;

        for z in vz_start..=vz_end {
            // 1. Stamp the outer liner (insulator)
             // Insulator has no net handle (handle 0)
             self.stamp_cylinder(
                 vx,
                 vy,
                 z,
                 total_radius_voxels,
                 params.stack.liner_material,
                 crate::netlist::NetHandle::none(),
             );

            // 2. Stamp the bridge (if present)
            if let Some(bridge_mat) = params.stack.bridge_material {
                self.stamp_cylinder(vx, vy, z, bridge_radius_voxels, bridge_mat, handle);
            }

            // 3. Stamp the conductive fill core
            self.stamp_cylinder(vx, vy, z, fill_radius_voxels, params.stack.fill_material, handle);
        }
    }
}
