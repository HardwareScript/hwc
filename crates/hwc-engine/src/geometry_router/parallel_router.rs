//! Parallel Router: Multi-threaded domain-based routing with Rayon.
//!
//! This module implements Phase 2 of the Hierarchical Parallel Routing pipeline
//! from GAP3. It uses domain isolation to enable lock-free parallel routing.
//!
//! **Reference:** `ROADMAP/v0.1.4/Gap3.md` (Section "Phase 2: Local Parallel Routing")
//!
//! **Architecture:**
//! - Each RoutingDomain has an isolated VoxelGrid (local_grid)
//! - Threads cannot touch each other's grids (zero locks, zero race conditions)
//! - Rayon's .into_par_iter() spawns threads automatically
//! - Output is deterministic because inputs are isolated
//! - VoxelGrid uses flat array indexing (no hash collisions)

use crate::constraint_manager::{ConstraintRulebook, Route, RoutedDomain, RoutingDomain};
use crate::geometry::Point3D;
use crate::geometry_router::pathfinding::route_net_deterministic;
use crate::netlist::NetlistArena;
use crate::voxel_grid::VoxelGrid;
use rayon::prelude::*;
use rustc_hash::FxHashSet;

/// Parallel router for domain-based multi-threaded routing.
///
/// Uses Rayon to route multiple domains simultaneously without locks.
/// Each domain has an isolated voxel grid, preventing race conditions.
pub struct ParallelRouter {
    /// Constraint rulebook for routing
    constraints: ConstraintRulebook,

    /// Voxel size in nanometers
    voxel_size_nm: i64,
}

impl ParallelRouter {
    /// Create a new parallel router.
    ///
    /// # Arguments
    /// * `constraints` - Constraint rulebook from constraint manager
    ///
    /// # Examples
    /// ```
    /// use hwc_engine::geometry_router::ParallelRouter;
    /// use hwc_engine::constraint_manager::ConstraintRulebook;
    ///
    /// let constraints = ConstraintRulebook::new(500_000);
    /// let router = ParallelRouter::new(constraints);
    /// ```
    pub fn new(constraints: ConstraintRulebook) -> Self {
        let voxel_size_nm = constraints.voxel_size_nm;

        Self {
            constraints,
            voxel_size_nm,
        }
    }

    /// Route multiple domains in parallel using Rayon.
    ///
    /// This is the core of Phase 2 parallel routing. Each domain is routed
    /// independently on a separate thread with its own isolated voxel grid.
    ///
    /// **Zero Locks:** Because each domain has its own VoxelGrid (local_grid),
    /// threads never touch shared memory. No Mutex, no RwLock, no atomics.
    ///
    /// **Deterministic:** Each domain produces identical routes every time
    /// because the input (domain boundaries, nets, constraints) is identical.
    /// VoxelGrid uses flat array indexing (no hash collisions).
    ///
    /// **Reference:** GAP3 Section "Phase 2: Local Parallel Routing"
    ///
    /// # Arguments
    /// * `domains` - Vector of routing domains to process
    /// * `netlist` - Netlist arena for pin position lookups
    ///
    /// # Returns
    /// Vector of routed domains with completed routes and occupied grids
    ///
    /// # Example
    /// Routes multiple domains in parallel, each with isolated voxel grids.
    pub fn route_domains(
        &self,
        domains: Vec<RoutingDomain>,
        netlist: &NetlistArena,
    ) -> Vec<RoutedDomain> {
        // Rayon magic: .into_par_iter() spawns threads automatically
        // Each thread gets its own RoutingDomain with isolated local_grid
        domains
            .into_par_iter()
            .map(|domain| {
                // Thread isolates here. It runs A* strictly within local_grid.
                // Output is deterministic because the input grid is isolated.
                let local_routes = Self::route_internal_nets(
                    &domain,
                    netlist,
                    &self.constraints,
                    self.voxel_size_nm,
                );

                // Create routed domain with a new VoxelGrid
                // Copy occupied voxels from domain's local_grid
                let grid_chunk = VoxelGrid::new(
                    domain.local_grid.size().0,
                    domain.local_grid.size().1,
                    domain.local_grid.size().2,
                    domain.local_grid.voxel_size,
                    0, // Default insulator (Air)
                );

                // Copy all occupied voxels from domain's local_grid
                for (x, y, z, material, handle) in domain.local_grid.iter_occupied() {
                    grid_chunk.set_occupied(x, y, z, material, handle);
                }

                RoutedDomain {
                    id: domain.domain_id.clone(),
                    box_offset: domain.bounding_box.min,
                    routes: local_routes,
                    grid_chunk,
                }
            })
            .collect()
    }

    /// Route all internal nets within a single domain.
    ///
    /// This function runs on a worker thread. It has exclusive access to
    /// the domain's local_grid, so no synchronization is needed.
    ///
    /// **Coordinate Translation:**
    /// - Converts global pin positions to local coordinates (relative to bounding box)
    /// - Routes in local space
    /// - Stores routes in local coordinates for later assembly
    ///
    /// **Reference:** GAP3 Section "Phase 2: Local Parallel Routing"
    ///
    /// # Arguments
    /// * `domain` - Reference to routing domain (immutable, we don't modify local_grid here)
    /// * `netlist` - Netlist arena for pin lookups
    /// * `constraints` - Constraint rulebook
    /// * `voxel_size_nm` - Voxel size in nanometers
    ///
    /// # Returns
    /// Vector of successfully routed nets in local coordinates
    fn route_internal_nets(
        domain: &RoutingDomain,
        netlist: &NetlistArena,
        constraints: &ConstraintRulebook,
        voxel_size_nm: i64,
    ) -> Vec<Route> {
        let mut routes = Vec::new();

        // Create bounds for this domain (in local coordinates)
        let (width, height, depth) = domain.dimensions();
        let local_bounds = super::neighbor_generation::GridBounds::new(width, height, depth);

        // Track occupied voxels in local space
        let mut occupied_voxels = FxHashSet::default();

        // Route all internal nets
        for &net_id in &domain.internal_nets {
            // Get net data
            let net_data = match netlist.get_net(net_id) {
                Some(data) => data,
                None => continue,
            };

            // Get pins for this net
            let pins = &net_data.pins;
            if pins.len() < 2 {
                continue; // Need at least 2 pins to route
            }

            // Route from first pin to all other pins
            let start_pin = pins[0];
            let start_pos_global = match netlist.get_pin_position(start_pin) {
                Some((x, y, z)) => Point3D::new(x, y, z),
                None => continue,
            };

            // Convert to local coordinates
            let start_local = domain.global_to_local(start_pos_global);

            // Route to each remaining pin
            for &end_pin in &pins[1..] {
                let end_pos_global = match netlist.get_pin_position(end_pin) {
                    Some((x, y, z)) => Point3D::new(x, y, z),
                    None => continue,
                };

                // Convert to local coordinates
                let end_local = domain.global_to_local(end_pos_global);

                // Get constraints for this net
                let net_constraints = constraints
                    .get_net_constraints(net_id)
                    .cloned()
                    .unwrap_or_default();

                // Determine layer direction
                let layer = (start_local.z / voxel_size_nm) as usize;
                let layer_direction = constraints.get_layer_direction(layer);

                // Route in local coordinate space
                let routing_params = crate::geometry_router::pathfinding::RoutingParams {
                    net_id,
                    constraints: &net_constraints,
                    bounds: local_bounds,
                    layer_direction,
                    voxel_size: crate::space::VoxelSize {
                        x_nm: voxel_size_nm,
                        y_nm: voxel_size_nm,
                        z_nm: voxel_size_nm,
                    },
                    clearance_zones: &[], // No clearance zones within domain (simplified for v0.1.4.2)
                    occupied_voxels: &occupied_voxels,
                    voxel_grid: Some(&domain.local_grid), // Pass VoxelGrid for collision detection
                    corridor: None,         // No corridor constraint for local routing
                    fixed_z_nm: None,       // v0.1.7: No fixed Z for local routing yet
                    exempt_components: &[], // v0.1.7: No exemptions for local routing yet
                    substrate_layers: None, // v0.1.7: No substrate context in parallel routing
                    is_high_speed_net: false, // v0.1.7: Default to non-high-speed
                };

                let path = route_net_deterministic(start_local, end_local, &routing_params);

                if let Some(waypoints) = path {
                    // Mark voxels as occupied for next route
                    for point in &waypoints {
                        occupied_voxels.insert(*point);
                    }

                    routes.push(Route { net_id, waypoints });
                }
            }
        }

        routes
    }
}
