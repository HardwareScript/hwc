//! Main physics validator implementation

use crate::bit_chunk::BitChunk;
use crate::geometry::Point3D;
use crate::geometry_router::substrate_types::NetId;
use crate::geometry_router::EntityGraph;
use rayon::prelude::*;
use std::time::Instant;

use super::clearance::validate_clearance_dilation;
use super::thermal::validate_thermal_gradients;
use super::types::{NetProperties, PhysicsValidationReport, PhysicsViolation};
use super::voltage::validate_voltage_boundaries;

/// Physics validator using bit-parallel sweeps
pub struct PhysicsValidator {
    /// Flat property table indexed by NetID
    /// This is the "God-Tier" replacement for HashMaps.
    /// Access: O(1) with zero hashing overhead
    net_properties: Vec<NetProperties>,
}

impl PhysicsValidator {
    pub fn new() -> Self {
        Self {
            net_properties: Vec::new(),
        }
    }

    /// Ensure the property table is large enough for the given NetID
    fn ensure_capacity(&mut self, net: NetId) {
        let required_size = (net as usize) + 1;
        if self.net_properties.len() < required_size {
            self.net_properties
                .resize(required_size, NetProperties::default());
        }
    }

    /// Get properties for a net (returns default if net not found)
    #[inline(always)]
    fn get_properties(&self, net: NetId) -> NetProperties {
        self.net_properties
            .get(net as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Set clearance requirement for a net
    pub fn set_clearance_requirement(&mut self, net: NetId, clearance_nm: i64) {
        self.ensure_capacity(net);
        self.net_properties[net as usize].clearance_nm = clearance_nm;
    }

    /// Set voltage for a net
    pub fn set_net_voltage(&mut self, net: NetId, voltage_mv: i64) {
        self.ensure_capacity(net);
        self.net_properties[net as usize].voltage_mv = voltage_mv;
    }

    /// Set current density for a net
    pub fn set_current_density(&mut self, net: NetId, density_ma_mm2: f64) {
        self.ensure_capacity(net);
        self.net_properties[net as usize].current_density_ma_mm2 = density_ma_mm2;
    }

    /// Validate the entire voxel grid using parallel page sweeping
    ///
    /// This is the main entry point for System 4 physics validation.
    /// It distributes chunks across all CPU cores using Rayon.
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to validate
    ///
    /// # Returns
    /// Physics validation report with all violations
    ///
    /// # Performance
    /// Expected: 100M voxels/sec on 4-core system
    pub fn validate_parallel(
        &self,
        grid: &EntityGraph,
        silicon_material_id: Option<crate::geometry_router::substrate_types::MaterialId>,
    ) -> PhysicsValidationReport {
        let start = Instant::now();

        // v0.1.7: Use 1-based Silicon ID if provided, otherwise default to 5 (standard)
        let silicon_id = silicon_material_id.unwrap_or(5);

        // Collect all chunk indices that are occupied
        let (size_x, size_y, size_z) = grid.size();
        let chunks_x = size_x.div_ceil(4);
        let chunks_y = size_y.div_ceil(4);
        let chunks_z = size_z.div_ceil(4);

        let mut chunk_coords = Vec::new();
        for cz in 0..chunks_z {
            for cy in 0..chunks_y {
                for cx in 0..chunks_x {
                    if !grid.is_chunk_empty(cx, cy, cz) {
                        chunk_coords.push((cx, cy, cz));
                    }
                }
            }
        }

        let chunks_checked = chunk_coords.len();

        // Parallel sweep: Distribute chunks across all CPU cores
        let violations: Vec<PhysicsViolation> = chunk_coords
            .par_iter()
            .flat_map(|&(cx, cy, cz)| self.validate_chunk(grid, cx, cy, cz, silicon_id))
            .collect();

        let elapsed = start.elapsed();
        let validation_time_ms = elapsed.as_secs_f64() * 1000.0;

        // Count voxels checked (approximate: chunks_checked × 64)
        let voxels_checked = chunks_checked * 64;

        PhysicsValidationReport {
            violations,
            validation_time_ms,
            voxels_checked,
            chunks_checked,
        }
    }

    /// Validate only dirty chunks (incremental validation)
    ///
    /// This is the God-Tier optimization for HMR. Instead of re-validating the entire board,
    /// we only validate chunks that have been modified since the last validation.
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to validate
    ///
    /// # Returns
    /// Physics validation report with violations from dirty chunks only
    ///
    /// # Performance
    /// Expected: < 100 microseconds for a single wire change
    /// This is 100× faster than full board validation for small changes
    ///
    /// # Important
    /// After calling this method, you should call `grid.clear_dirty_flags()` to reset
    /// the dirty tracking for the next incremental validation cycle.
    pub fn validate_incremental(
        &self,
        grid: &EntityGraph,
        silicon_material_id: Option<crate::geometry_router::substrate_types::MaterialId>,
    ) -> PhysicsValidationReport {
        let start = Instant::now();

        // v0.1.7: Use 1-based Silicon ID if provided, otherwise default to 5 (standard)
        let silicon_id = silicon_material_id.unwrap_or(5);

        let dirty_chunk_indices = grid.get_dirty_chunks();
        let chunks_checked = dirty_chunk_indices.len();

        if chunks_checked == 0 {
            // No dirty chunks - nothing to validate
            return PhysicsValidationReport {
                violations: Vec::new(),
                validation_time_ms: 0.0,
                voxels_checked: 0,
                chunks_checked: 0,
            };
        }

        // Convert chunk indices to chunk coordinates
        let (size_x, size_y, _size_z) = grid.size();
        let chunks_x = size_x.div_ceil(4);
        let chunks_y = size_y.div_ceil(4);

        let chunk_coords: Vec<(usize, usize, usize)> = dirty_chunk_indices
            .iter()
            .map(|&index| {
                // Reverse the linear indexing: chunk_x + chunk_y * chunks_x + chunk_z * chunks_x * chunks_y
                let chunk_z = index / (chunks_x * chunks_y);
                let remainder = index % (chunks_x * chunks_y);
                let chunk_y = remainder / chunks_x;
                let chunk_x = remainder % chunks_x;
                (chunk_x, chunk_y, chunk_z)
            })
            .collect();

        // Parallel sweep: Distribute dirty chunks across all CPU cores
        let violations: Vec<PhysicsViolation> = chunk_coords
            .par_iter()
            .flat_map(|&(cx, cy, cz)| self.validate_chunk(grid, cx, cy, cz, silicon_id))
            .collect();

        let elapsed = start.elapsed();
        let validation_time_ms = elapsed.as_secs_f64() * 1000.0;

        // Count voxels checked (approximate: chunks_checked × 64)
        let voxels_checked = chunks_checked * 64;

        PhysicsValidationReport {
            violations,
            validation_time_ms,
            voxels_checked,
            chunks_checked,
        }
    }

    /// Validate a single chunk (4×4×4 = 64 voxels)
    ///
    /// This is called in parallel by validate_parallel().
    /// Uses bitwise operations for maximum speed.
    ///
    /// # Arguments
    /// * `grid` - The voxel grid
    /// * `chunk_x`, `chunk_y`, `chunk_z` - Chunk coordinates
    ///
    /// # Returns
    /// Vector of violations found in this chunk
    fn validate_chunk(
        &self,
        grid: &EntityGraph,
        chunk_x: usize,
        chunk_y: usize,
        chunk_z: usize,
        silicon_id: crate::geometry_router::substrate_types::MaterialId,
    ) -> Vec<PhysicsViolation> {
        let mut violations = Vec::new();

        // Convert chunk coordinates to voxel coordinates
        let base_x = chunk_x * 4;
        let base_y = chunk_y * 4;
        let base_z = chunk_z * 4;

        // Create a BitChunk representation of this region
        let mut bit_chunk = BitChunk::new();

        // Populate the BitChunk from the VoxelGrid
        for lz in 0..4 {
            for ly in 0..4 {
                for lx in 0..4 {
                    let x = base_x + lx;
                    let y = base_y + ly;
                    let z = base_z + lz;

                    if !grid.is_empty(x, y, z) {
                        let material = grid.get_material(x, y, z);
                        let net = grid.get_net(x, y, z);
                        let index = BitChunk::local_index(lx, ly, lz);
                        bit_chunk.set_occupied(index, material, net);
                    }
                }
            }
        }

        // Find all short circuits in this chunk using bit-parallel operations
        let short_circuits = bit_chunk.find_all_short_circuits();
        for (net_a, net_b, collision_mask) in short_circuits {
            // Find the first collision voxel
            let first_bit = collision_mask.trailing_zeros() as usize;
            let lx = first_bit % 4;
            let ly = (first_bit / 4) % 4;
            let lz = first_bit / 16;

            let location = Point3D::new(
                (base_x + lx) as i64 * grid.voxel_size.x_nm,
                (base_y + ly) as i64 * grid.voxel_size.y_nm,
                (base_z + lz) as i64 * grid.voxel_size.z_nm,
            );

            violations.push(PhysicsViolation::ShortCircuit {
                net_a,
                net_b,
                location,
            });
        }

        // v0.1.7: TSV Substrate Short Circuit Detection (P47)
        // Check for shorts between conductive nets and silicon substrate
        for &net_id in bit_chunk.net_planes.keys() {
            if net_id == 0 {
                continue;
            } // Skip substrate net

            let shorts = bit_chunk.find_substrate_shorts(net_id, silicon_id);
            if shorts != 0 {
                // Find first violation location
                let first_bit = shorts.trailing_zeros() as usize;
                let lx = first_bit % 4;
                let ly = (first_bit / 4) % 4;
                let lz = first_bit / 16;

                let location = Point3D::new(
                    (base_x + lx) as i64 * grid.voxel_size.x_nm,
                    (base_y + ly) as i64 * grid.voxel_size.y_nm,
                    (base_z + lz) as i64 * grid.voxel_size.z_nm,
                );

                violations.push(PhysicsViolation::SubstrateShortCircuit {
                    net: net_id,
                    substrate_material: silicon_id,
                    location,
                });
            }
        }

        // v0.1.7: TSV Keep-Out Zone (KOZ) Validation
        // Checks if components/traces are placed in forbidden stress-field regions
        for lz in 0..4 {
            for ly in 0..4 {
                for lx in 0..4 {
                    let index = BitChunk::local_index(lx, ly, lz);
                    if bit_chunk.is_occupied(index) {
                        let net_id = bit_chunk.get_net(index).unwrap_or(0);
                        if net_id == 0 {
                            continue;
                        } // Skip substrate net

                        let x_nm = (base_x + lx) as i64 * grid.voxel_size.x_nm;
                        let y_nm = (base_y + ly) as i64 * grid.voxel_size.y_nm;
                        let z_nm = (base_z + lz) as i64 * grid.voxel_size.z_nm;

                        for layer in grid.get_substrate_layers() {
                            if layer.koz_radius_nm > 0 && layer.is_in_koz(x_nm, y_nm, z_nm) {
                                // If it's a contact layer (TSV), we need to check if the net
                                // is the same as the TSV net. If so, it's allowed.
                                if layer.layer_type
                                    == crate::geometry_router::substrate_types::SubstrateLayerType::Contact
                                    && layer.net == net_id
                                {
                                    continue;
                                }

                                violations.push(PhysicsViolation::KozViolation {
                                    net: net_id,
                                    location: Point3D::new(x_nm, y_nm, z_nm),
                                    reason: "Inside TSV mechanical stress keep-out zone".into(),
                                });
                                break; // Only report one KOZ violation per voxel
                            }
                        }
                    }
                }
            }
        }

        // Create closure for property access
        let get_props = |net: NetId| self.get_properties(net);

        // Voltage boundary validation
        violations.extend(validate_voltage_boundaries(
            grid, &bit_chunk, base_x, base_y, base_z, get_props,
        ));

        // Clearance validation using dilation (THE KILLER FEATURE)
        violations.extend(validate_clearance_dilation(
            &bit_chunk, base_x, base_y, base_z, get_props,
        ));

        // Thermal gradient validation
        violations.extend(validate_thermal_gradients(
            grid, &bit_chunk, base_x, base_y, base_z, get_props,
        ));

        violations
    }
}

impl Default for PhysicsValidator {
    fn default() -> Self {
        Self::new()
    }
}
