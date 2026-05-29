//! Route Lockfile System
//!
//! **Architecture Reference:** GAP2 Section 2 (Pillar B)
//!
//! Persistent route caching to prevent the "butterfly effect" where moving one
//! component causes all routes to change. The lockfile stores successful routes
//! and only reroutes nets that are actually affected by changes.
//!
//! # Benefits
//! - Minimal Git diffs (only changed routes appear)
//! - Faster compilation (frozen routes skip A* entirely)
//! - Predictable behavior (moving one component doesn't cascade)
//! - Team collaboration (merge conflicts are rare and localized)

use crate::geometry::Point3D;
use crate::netlist::NetId;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Route lockfile containing all successfully routed nets.
///
/// This file is generated on every successful build and stores the exact
/// waypoints of each route. On subsequent builds, unchanged routes are
/// preserved exactly, and only affected routes are rerouted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteLockfile {
    /// Lockfile format version
    pub version: CompactString,

    /// Board name for validation
    pub board: CompactString,

    /// Grid metadata for validation
    pub grid: GridMetadata,

    /// All locked routes
    pub routes: Vec<LockedRoute>,
}

/// Grid metadata for validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridMetadata {
    /// Grid dimensions [width, height, depth] in voxels
    pub dimensions: [usize; 3],

    /// Voxel resolution [x, y, z] in millimeters
    pub resolution: [f64; 3],
}

/// A locked route with exact waypoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedRoute {
    /// Net ID (as raw u32 for serialization)
    pub net_id: u32,

    /// Net name for human readability
    pub net_name: CompactString,

    /// Source pin reference (e.g., "R1.Pin2")
    pub source: CompactString,

    /// Destination pin reference (e.g., "Amp_Tx.RF_IN")
    pub destination: CompactString,

    /// Exact waypoints [x, y, z] in nanometers
    pub waypoints: Vec<[i64; 3]>,

    /// Total route length in millimeters
    pub length_mm: f64,

    /// Number of layer transitions (vias)
    pub layer_transitions: u32,

    /// Hash of source/dest positions for validation
    pub hash: CompactString,
}

impl RouteLockfile {
    /// Create a new empty lockfile.
    pub fn new(board: CompactString, grid: GridMetadata) -> Self {
        Self {
            version: "0.1.4".into(),
            board,
            grid,
            routes: Vec::new(),
        }
    }

    /// Load lockfile from disk.
    ///
    /// # Arguments
    /// * `path` - Path to .hw.routes.lock file
    ///
    /// # Returns
    /// Lockfile if it exists and is valid, None otherwise
    pub fn load<P: AsRef<Path>>(path: P) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Save lockfile to disk.
    ///
    /// # Arguments
    /// * `path` - Path to .hw.routes.lock file
    ///
    /// # Returns
    /// Ok if successful, Err otherwise
    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
    }

    /// Add a route to the lockfile.
    pub fn add_route(&mut self, route: LockedRoute) {
        self.routes.push(route);
    }

    /// Sort routes by net name for deterministic diffs.
    pub fn sort_routes(&mut self) {
        self.routes.sort_by(|a, b| a.net_name.cmp(&b.net_name));
    }

    /// Get route by net ID.
    pub fn get_route(&self, net_id: NetId) -> Option<&LockedRoute> {
        self.routes.iter().find(|r| r.net_id == net_id.raw())
    }

    /// Remove route by net ID.
    pub fn remove_route(&mut self, net_id: NetId) {
        self.routes.retain(|r| r.net_id != net_id.raw());
    }

    /// Validate grid metadata matches current board.
    pub fn validate_grid(&self, current_grid: &GridMetadata) -> bool {
        self.grid.dimensions == current_grid.dimensions
            && self.grid.resolution == current_grid.resolution
    }
}

impl LockedRoute {
    /// Create a new locked route.
    ///
    /// # Arguments
    /// * `net_id` - Net ID
    /// * `net_name` - Net name for human readability
    /// * `source` - Source pin reference
    /// * `destination` - Destination pin reference
    /// * `waypoints` - Route waypoints in nanometers
    pub fn new(
        net_id: NetId,
        net_name: CompactString,
        source: CompactString,
        destination: CompactString,
        waypoints: Vec<Point3D>,
    ) -> Self {
        // Convert waypoints to serializable format
        let waypoints_array: Vec<[i64; 3]> = waypoints.iter().map(|p| [p.x, p.y, p.z]).collect();

        // Calculate length in millimeters
        let length_nm: i64 = waypoints
            .windows(2)
            .map(|w| {
                let dx = w[1].x - w[0].x;
                let dy = w[1].y - w[0].y;
                let dz = w[1].z - w[0].z;
                ((dx * dx + dy * dy + dz * dz) as f64).sqrt() as i64
            })
            .sum();
        let length_mm = length_nm as f64 / 1_000_000.0;

        // Count layer transitions
        let layer_transitions = waypoints.windows(2).filter(|w| w[0].z != w[1].z).count() as u32;

        // Calculate hash of endpoints for validation
        let hash = Self::calculate_hash(&waypoints);

        Self {
            net_id: net_id.raw(),
            net_name,
            source,
            destination,
            waypoints: waypoints_array,
            length_mm,
            layer_transitions,
            hash,
        }
    }

    /// Calculate hash of route endpoints for validation.
    fn calculate_hash(waypoints: &[Point3D]) -> CompactString {
        if waypoints.is_empty() {
            return String::from("empty").into();
        }

        let start = waypoints.first().unwrap();
        let end = waypoints.last().unwrap();

        // Simple hash: combine coordinates
        format!(
            "{:x}",
            (start.x ^ start.y ^ start.z ^ end.x ^ end.y ^ end.z) as u64
        )
        .into()
    }

    /// Convert waypoints back to Point3D.
    pub fn to_points(&self) -> Vec<Point3D> {
        self.waypoints
            .iter()
            .map(|[x, y, z]| Point3D::new(*x, *y, *z))
            .collect()
    }

    /// Validate that endpoints match expected positions.
    ///
    /// # Arguments
    /// * `start` - Expected start position
    /// * `end` - Expected end position
    ///
    /// # Returns
    /// true if endpoints match, false otherwise
    pub fn validate_endpoints(&self, start: Point3D, end: Point3D) -> bool {
        if self.waypoints.is_empty() {
            return false;
        }

        let route_start = self.waypoints.first().unwrap();
        let route_end = self.waypoints.last().unwrap();

        route_start[0] == start.x
            && route_start[1] == start.y
            && route_start[2] == start.z
            && route_end[0] == end.x
            && route_end[1] == end.y
            && route_end[2] == end.z
    }
}

/// Route lockfile manager for selective rerouting.
pub struct LockfileManager {
    /// Current lockfile
    lockfile: Option<RouteLockfile>,

    /// Routes that need rerouting
    invalidated_routes: FxHashMap<NetId, String>,
}

impl LockfileManager {
    /// Create a new lockfile manager.
    pub fn new(lockfile: Option<RouteLockfile>) -> Self {
        Self {
            lockfile,
            invalidated_routes: FxHashMap::default(),
        }
    }

    /// Check if a route is locked and valid.
    ///
    /// # Arguments
    /// * `net_id` - Net ID to check
    /// * `start` - Expected start position
    /// * `end` - Expected end position
    ///
    /// # Returns
    /// Some(waypoints) if route is locked and valid, None otherwise
    pub fn get_locked_route(
        &self,
        net_id: NetId,
        start: Point3D,
        end: Point3D,
    ) -> Option<Vec<Point3D>> {
        let lockfile = self.lockfile.as_ref()?;
        let route = lockfile.get_route(net_id)?;

        // Validate endpoints haven't moved
        if !route.validate_endpoints(start, end) {
            return None;
        }

        // Check if route was invalidated
        if self.invalidated_routes.contains_key(&net_id) {
            return None;
        }

        Some(route.to_points())
    }

    /// Invalidate a route (mark for rerouting).
    ///
    /// # Arguments
    /// * `net_id` - Net ID to invalidate
    /// * `reason` - Reason for invalidation (for logging)
    pub fn invalidate_route(&mut self, net_id: NetId, reason: CompactString) {
        self.invalidated_routes.insert(net_id, reason.to_string());
    }

    /// Get all invalidated routes.
    pub fn get_invalidated_routes(&self) -> &FxHashMap<NetId, String> {
        &self.invalidated_routes
    }

    /// Check if any routes were invalidated.
    pub fn has_invalidated_routes(&self) -> bool {
        !self.invalidated_routes.is_empty()
    }
}
