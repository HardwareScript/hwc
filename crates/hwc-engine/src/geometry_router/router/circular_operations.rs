//! Circular area operations for via footprints and anti-pads

use super::core::GeometryRouter;
use crate::geometry::Point3D;

impl GeometryRouter {
    /// Check if a circular area is clear of occupied voxels at a Z elevation.
    pub(super) fn is_circular_area_clear(
        &self,
        center: (i64, i64),
        radius_nm: i64,
        z_nm: i64,
    ) -> bool {
        let voxel_radius = (radius_nm / self.voxel_size_nm) + 1;

        for dx in -voxel_radius..=voxel_radius {
            for dy in -voxel_radius..=voxel_radius {
                let x = center.0 + (dx * self.voxel_size_nm);
                let y = center.1 + (dy * self.voxel_size_nm);

                let dist_sq = (dx * dx + dy * dy) as f64;
                let radius_voxels = radius_nm as f64 / self.voxel_size_nm as f64;
                if dist_sq <= radius_voxels * radius_voxels {
                    let point = Point3D::new(x, y, z_nm);

                    if self.occupied_voxels.contains_key(&point) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Mark a circular area as occupied by a net at a Z elevation.
    pub(super) fn mark_circular_area_occupied(
        &mut self,
        center: (i64, i64),
        radius_nm: i64,
        z_nm: i64,
        net_id: crate::netlist::NetId,
    ) {
        let voxel_radius = (radius_nm / self.voxel_size_nm) + 1;

        for dx in -voxel_radius..=voxel_radius {
            for dy in -voxel_radius..=voxel_radius {
                let x = center.0 + (dx * self.voxel_size_nm);
                let y = center.1 + (dy * self.voxel_size_nm);

                let dist_sq = (dx * dx + dy * dy) as f64;
                let radius_voxels = radius_nm as f64 / self.voxel_size_nm as f64;
                if dist_sq <= radius_voxels * radius_voxels {
                    let point = Point3D::new(x, y, z_nm);
                    self.occupied_voxels.insert(point, net_id);
                }
            }
        }
    }

    /// Remove a circular area from occupied voxels at a Z elevation.
    pub(super) fn remove_circular_area(&mut self, center: (i64, i64), radius_nm: i64, z_nm: i64) {
        let voxel_radius = (radius_nm / self.voxel_size_nm) + 1;

        for dx in -voxel_radius..=voxel_radius {
            for dy in -voxel_radius..=voxel_radius {
                let x = center.0 + (dx * self.voxel_size_nm);
                let y = center.1 + (dy * self.voxel_size_nm);

                let dist_sq = (dx * dx + dy * dy) as f64;
                let radius_voxels = radius_nm as f64 / self.voxel_size_nm as f64;
                if dist_sq <= radius_voxels * radius_voxels {
                    let point = Point3D::new(x, y, z_nm);
                    self.occupied_voxels.remove(&point);
                }
            }
        }
    }
}
