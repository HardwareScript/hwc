//! Domain structures for hierarchical parallel routing.
//!
//! This module defines the core data structures for domain-based parallel routing,
//! where the design is partitioned into isolated "Glass Box" domains that can be
//! routed independently on separate threads.

use crate::geometry::{BoundingBox, Point3D};
use crate::netlist::{NetId, PinId};
use crate::voxel_grid::VoxelGrid;
use compact_str::CompactString;

/// A routing domain represents an isolated "Glass Box" for parallel routing.
///
/// Each domain contains:
/// - A 3D bounding box defining its physical boundaries
/// - A list of internal nets that route entirely within the domain
/// - A list of interface pins that connect to the outside world
/// - An isolated voxel grid for collision-free parallel routing
///
/// # Reference
/// - `ROADMAP/v0.1.4/Gap3.md` (Section "1. The Domain Structure")
///
/// # Example
/// A routing domain represents a module instance with its bounding box,
/// internal nets, interface pins, and local voxel grid for routing.
#[derive(Debug, Clone)]
pub struct RoutingDomain {
    /// Unique identifier for this domain (e.g., "MainDSP.ALU_Core")
    pub domain_id: CompactString,

    /// The physical 3D "Glass Box" boundaries
    pub bounding_box: BoundingBox,

    /// Nets that route entirely inside this module
    pub internal_nets: Vec<NetId>,

    /// Pins on the edge connecting to the outside world
    pub interface_pins: Vec<PinId>,

    /// Local voxel grid for collision-free parallel routing
    /// Uses VoxelGrid with flat array indexing (no HashMap collisions)
    pub local_grid: VoxelGrid,
}

/// A routed domain contains the results of parallel routing within a domain.
///
/// After a thread finishes routing a domain, it returns this structure containing:
/// - The domain identifier
/// - The bounding box offset for coordinate translation
/// - All routes that were successfully placed
/// - The occupied voxel grid chunk
///
/// # Reference
/// - `ROADMAP/v0.1.4/Gap3.md` (Section "1. The Domain Structure")
///
/// # Example
/// A routed domain contains the successfully placed routes and occupied voxels
/// for a module instance after parallel routing.
#[derive(Debug, Clone)]
pub struct RoutedDomain {
    /// Domain identifier
    pub id: CompactString,

    /// Bounding box minimum point for coordinate translation during assembly
    pub box_offset: Point3D,

    /// All internal routes successfully placed in this domain
    pub routes: Vec<Route>,

    /// Occupied voxels using VoxelGrid (flat array indexing)
    /// No Morton encoding collisions, deterministic routing
    pub grid_chunk: VoxelGrid,
}

/// A route within a domain.
///
/// Represents a successfully routed connection between two pins.
#[derive(Debug, Clone)]
pub struct Route {
    /// The net this route belongs to
    pub net_id: NetId,

    /// Waypoints along the route (in local coordinates relative to domain)
    pub waypoints: Vec<Point3D>,
}

impl RoutingDomain {
    /// Create a new routing domain.
    ///
    /// # Arguments
    /// * `domain_id` - Unique identifier for this domain
    /// * `bounding_box` - Physical 3D boundaries
    /// * `internal_nets` - Nets that route entirely within this domain
    /// * `interface_pins` - Pins connecting to the outside world
    ///
    /// # Returns
    /// A new `RoutingDomain` with an empty local grid
    pub fn new(
        domain_id: CompactString,
        bounding_box: BoundingBox,
        internal_nets: Vec<NetId>,
        interface_pins: Vec<PinId>,
    ) -> Self {
        // Calculate domain dimensions in nanometers
        let width = (bounding_box.max.x - bounding_box.min.x).max(0) as usize;
        let height = (bounding_box.max.y - bounding_box.min.y).max(0) as usize;
        let depth = (bounding_box.max.z - bounding_box.min.z).max(0) as usize;

        // Convert to voxel dimensions (assuming 100µm voxels = 100,000 nm)
        let voxel_size_nm = 100_000;
        let voxels_x = width.div_ceil(voxel_size_nm);
        let voxels_y = height.div_ceil(voxel_size_nm);
        let voxels_z = depth.div_ceil(1_000_000); // 1mm layers

        // Create VoxelSize for this domain
        let voxel_size = crate::space::VoxelSize {
            x_nm: voxel_size_nm as i64,
            y_nm: voxel_size_nm as i64,
            z_nm: 1_000_000, // 1mm layers
        };

        // Create VoxelGrid for this domain
        let local_grid = VoxelGrid::new(voxels_x, voxels_y, voxels_z, voxel_size, 0);

        Self {
            domain_id,
            bounding_box,
            internal_nets,
            interface_pins,
            local_grid,
        }
    }

    /// Get the dimensions of this domain in nanometers.
    pub fn dimensions(&self) -> (i64, i64, i64) {
        let width = self.bounding_box.max.x - self.bounding_box.min.x;
        let height = self.bounding_box.max.y - self.bounding_box.min.y;
        let depth = self.bounding_box.max.z - self.bounding_box.min.z;
        (width, height, depth)
    }

    /// Convert a global coordinate to local coordinate (relative to domain origin).
    pub fn global_to_local(&self, global: Point3D) -> Point3D {
        Point3D::new(
            global.x - self.bounding_box.min.x,
            global.y - self.bounding_box.min.y,
            global.z - self.bounding_box.min.z,
        )
    }

    /// Convert a local coordinate to global coordinate.
    pub fn local_to_global(&self, local: Point3D) -> Point3D {
        Point3D::new(
            local.x + self.bounding_box.min.x,
            local.y + self.bounding_box.min.y,
            local.z + self.bounding_box.min.z,
        )
    }
}

impl RoutedDomain {
    /// Create a new routed domain from a routing domain and its routes.
    pub fn new(domain: &RoutingDomain, routes: Vec<Route>) -> Self {
        Self {
            id: domain.domain_id.clone(),
            box_offset: domain.bounding_box.min,
            routes,
            // Clone the VoxelGrid (this is efficient due to null-page optimization)
            grid_chunk: VoxelGrid::new(
                domain.local_grid.size().0,
                domain.local_grid.size().1,
                domain.local_grid.size().2,
                domain.local_grid.voxel_size,
                0, // Default insulator (Air)
            ),
        }
    }
}
