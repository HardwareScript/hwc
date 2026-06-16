//! Type Definitions for Geometry Router
//!
//! This module contains the core types used by the geometry router.

use crate::geometry::Point3D;
use crate::netlist::NetId;
use hwc_parser::Expression;
use rustc_hash::FxHashMap;

/// Net route request for automatic routing.
///
/// Contains the start and goal points for a net that needs to be routed.
#[derive(Debug, Clone)]
pub struct NetRoute {
    pub net_id: NetId,
    pub start: Point3D,
    pub goal: Point3D,
}

/// Routed net result.
///
/// Contains the successfully routed path for a net.
#[derive(Debug, Clone)]
pub struct RoutedNet {
    pub net_id: NetId,
    pub path: Vec<Point3D>,
    pub vias: Vec<Via>, // Track vias for drill file generation
}

/// Via type classification for HDI (High-Density Interconnect) routing.
///
/// Different via types have different manufacturing costs and routing implications:
/// - Through-hole: Cheapest, but blocks all layers
/// - Blind: More expensive, blocks fewer layers
/// - Buried: Most expensive, doesn't block outer layers
/// - Microvia: Expensive, smallest footprint (<150µm, max 2 layers)
///
/// **Reference:** GAP1 Section 5.3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViaType {
    /// Through-hole via spanning all layers (cheapest)
    ThroughHole,

    /// Blind via from outer layer to inner layer
    Blind,

    /// Buried via connecting only inner layers (most expensive)
    Buried,

    /// Microvia with laser drilling (<150µm diameter, max 2 layers)
    Microvia,
}

impl ViaType {
    /// Get the manufacturing cost multiplier for this via type.
    ///
    /// Used by the router to prefer cheaper via types when possible.
    /// Base cost is 1.0 for through-hole vias.
    pub fn cost_multiplier(&self) -> f64 {
        match self {
            ViaType::ThroughHole => 1.0,
            ViaType::Blind => 1.5,
            ViaType::Buried => 2.0,
            ViaType::Microvia => 2.5,
        }
    }

    /// Get the routing penalty for this via type.
    ///
    /// Higher penalty discourages the router from using this via type.
    /// Through-hole has base penalty of 10,000 points.
    pub fn routing_penalty(&self) -> i64 {
        match self {
            ViaType::ThroughHole => 10_000,
            ViaType::Blind => 12_000,
            ViaType::Buried => 15_000,
            ViaType::Microvia => 18_000,
        }
    }
}

/// Via (Vertical Interconnect Access) for PCB manufacturing.
///
/// Represents a physical drill hole with copper plating between two Z elevations.
/// Vias are expensive and degrade signals, so the router minimizes their use.
#[derive(Debug, Clone, PartialEq)]
pub struct Via {
    /// Position in 2D space (X, Y coordinates in nanometers)
    pub position: (i64, i64),

    /// Bottom Z elevation of the via span in nanometers
    pub from_z_nm: i64,

    /// Top Z elevation of the via span in nanometers
    pub to_z_nm: i64,

    /// Drill diameter in nanometers
    pub diameter_nm: i64,

    /// Net ID this via belongs to
    pub net_id: NetId,

    /// Via type classification (through-hole, blind, buried, microvia)
    pub via_type: ViaType,

    /// Annular ring size in nanometers (copper pad around drill hole)
    pub annular_ring_nm: i64,

    /// Generic via properties (e.g., thermal_relief)
    pub properties: FxHashMap<String, Expression>,
}

impl Via {
    /// Create a new via with automatic type classification from physical Z extents.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        position: (i64, i64),
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        net_id: NetId,
        board_min_z_nm: i64,
        board_max_z_nm: i64,
        voxel_z_nm: i64,
        annular_ring_nm: i64,
    ) -> Self {
        let via_type = Self::classify_via_type(
            from_z_nm,
            to_z_nm,
            diameter_nm,
            board_min_z_nm,
            board_max_z_nm,
            voxel_z_nm,
        );

        Self {
            position,
            from_z_nm,
            to_z_nm,
            diameter_nm,
            net_id,
            via_type,
            annular_ring_nm,
            properties: FxHashMap::default(),
        }
    }

    /// Create a new via with explicit type.
    pub fn new_with_type(
        position: (i64, i64),
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        net_id: NetId,
        via_type: ViaType,
        annular_ring_nm: i64,
    ) -> Self {
        Self {
            position,
            from_z_nm,
            to_z_nm,
            diameter_nm,
            net_id,
            via_type,
            annular_ring_nm,
            properties: FxHashMap::default(),
        }
    }

    /// Classify via type from physical Z span (no layer indices).
    fn classify_via_type(
        from_z_nm: i64,
        to_z_nm: i64,
        diameter_nm: i64,
        board_min_z_nm: i64,
        board_max_z_nm: i64,
        voxel_z_nm: i64,
    ) -> ViaType {
        let voxel_z_nm = voxel_z_nm.max(1);
        let min_z = from_z_nm.min(to_z_nm);
        let max_z = from_z_nm.max(to_z_nm);
        let z_span = max_z - min_z;

        if board_max_z_nm <= board_min_z_nm {
            return ViaType::ThroughHole;
        }

        // Microvia: <150µm diameter and spans at most two voxel slabs
        if diameter_nm < 150_000 && z_span <= 2 * voxel_z_nm {
            return ViaType::Microvia;
        }

        let touches_bottom = min_z <= board_min_z_nm + voxel_z_nm / 2;
        let touches_top = max_z >= board_max_z_nm - voxel_z_nm / 2;

        if touches_bottom && touches_top {
            return ViaType::ThroughHole;
        }

        if !touches_bottom && !touches_top {
            return ViaType::Buried;
        }

        ViaType::Blind
    }

    /// Check if this is a through-hole via (spans the full board Z extent).
    pub fn is_through_hole(
        &self,
        board_min_z_nm: i64,
        board_max_z_nm: i64,
        voxel_z_nm: i64,
    ) -> bool {
        let voxel_z_nm = voxel_z_nm.max(1);
        let min_z = self.from_z_nm.min(self.to_z_nm);
        let max_z = self.from_z_nm.max(self.to_z_nm);
        min_z <= board_min_z_nm + voxel_z_nm / 2 && max_z >= board_max_z_nm - voxel_z_nm / 2
    }

    /// Check if this is a blind via (touches one outer Z face but not both).
    pub fn is_blind(&self, board_min_z_nm: i64, board_max_z_nm: i64, voxel_z_nm: i64) -> bool {
        let voxel_z_nm = voxel_z_nm.max(1);
        let min_z = self.from_z_nm.min(self.to_z_nm);
        let max_z = self.from_z_nm.max(self.to_z_nm);
        let touches_bottom = min_z <= board_min_z_nm + voxel_z_nm / 2;
        let touches_top = max_z >= board_max_z_nm - voxel_z_nm / 2;
        (touches_bottom || touches_top) && !(touches_bottom && touches_top)
    }

    /// Check if this is a buried via (does not touch either board Z face).
    pub fn is_buried(&self, board_min_z_nm: i64, board_max_z_nm: i64, voxel_z_nm: i64) -> bool {
        let voxel_z_nm = voxel_z_nm.max(1);
        let min_z = self.from_z_nm.min(self.to_z_nm);
        let max_z = self.from_z_nm.max(self.to_z_nm);
        min_z > board_min_z_nm + voxel_z_nm / 2 && max_z < board_max_z_nm - voxel_z_nm / 2
    }

    /// Sample Z plane positions (bottom of each voxel slab) between the via endpoints.
    pub fn z_planes(&self, voxel_z_nm: i64) -> Vec<i64> {
        let voxel_z_nm = voxel_z_nm.max(1);
        let min_z = self.from_z_nm.min(self.to_z_nm);
        let max_z = self.from_z_nm.max(self.to_z_nm);
        let first = (min_z / voxel_z_nm) * voxel_z_nm;
        let mut z = if first < min_z {
            first + voxel_z_nm
        } else {
            first
        };
        let mut planes = Vec::new();
        while z <= max_z {
            planes.push(z);
            z += voxel_z_nm;
        }
        planes
    }

    /// Get the total footprint radius including annular ring and clearance.
    ///
    /// # Arguments
    /// * `annular_ring_nm` - Copper pad around the drill hole
    /// * `clearance_nm` - Additional clearance for manufacturing
    pub fn footprint_radius_nm(&self, annular_ring_nm: i64, clearance_nm: i64) -> i64 {
        (self.diameter_nm + 2 * annular_ring_nm + clearance_nm) / 2
    }
}

/// Unified result container for both routing modes (Pass-Through & Hierarchical).
///
/// Aggregates paths and vias from all routed nets into a single result
/// that can be consumed by the compiler pipeline.
#[derive(Debug, Clone, Default)]
pub struct RouteResult {
    /// Routed paths per net. Each net maps to its ordered list of waypoints.
    pub paths: FxHashMap<NetId, Vec<Point3D>>,
    /// All vias placed during routing (for drill file generation).
    pub vias: Vec<Via>,
}

impl RouteResult {
    /// Create a new empty route result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another RouteResult into this one, appending paths and vias.
    ///
    /// If a net exists in both results, the paths are concatenated.
    pub fn merge(&mut self, other: RouteResult) {
        for (net_id, mut path) in other.paths {
            self.paths.entry(net_id).or_default().append(&mut path);
        }
        self.vias.extend(other.vias);
    }
}

/// Routing error types with miette integration for multi-error reporting.
#[derive(Debug, Clone, thiserror::Error, miette::Diagnostic)]
pub enum RoutingError {
    /// No path found between start and goal
    #[error("No path found for net {} from {} to {}", .net_id.raw(), .start, .goal)]
    #[diagnostic(
        code(R21),
        url("https://docs.hw-script.org/errors/R21"),
        help("Physical Explanation: Routing failed because no valid path exists between the start and goal points. This can happen due to:\n- Insufficient clearance between obstacles\n- All routing layers blocked by other nets\n- Design rule constraints too restrictive\n\nSolution:\n1. Increase board size or add more routing layers\n2. Relax clearance constraints if safe\n3. Move components to create routing channels\n4. Use manual waypoints to guide the router\n\nDebugging: Use 'hwc route --debug' to visualize blocked regions.")
    )]
    NoPathFound {
        net_id: NetId,
        start: Point3D,
        goal: Point3D,
    },

    /// Clearance violation detected
    #[error("Clearance violation for net {} with net {} at {}", .net_id.raw(), .conflicting_net.raw(), .location)]
    #[diagnostic(
        code(R22),
        url("https://docs.hw-script.org/errors/R22"),
        help("Physical Explanation: Two nets are too close together, violating minimum clearance requirements. Insufficient clearance can cause:\n- Dielectric breakdown (arcing) between conductors\n- Manufacturing defects (shorts)\n- Signal integrity issues (crosstalk)\n\nBreakdown Voltage: V_bd = E_bd × d\nFor FR4: E_bd ≈ 20 kV/mm = 20 V/μm\n\nSolution:\n1. Increase spacing between nets\n2. Route on different layers\n3. Reduce voltage difference\n4. Use thicker dielectric material\n\nIPC-2221 recommends 2× minimum clearance for reliability.")
    )]
    ClearanceViolation {
        net_id: NetId,
        conflicting_net: NetId,
        location: Point3D,
    },

    /// Via placement blocked by occupied voxels
    #[error("Via placement blocked for net {} at {} - insufficient clearance on intermediate layers", .net_id.raw(), .position)]
    #[diagnostic(
        code(R23),
        url("https://docs.hw-script.org/errors/R23"),
        help("Physical Explanation: Via placement requires clearance on all layers it passes through. Blocked vias indicate:\n- Insufficient anti-pad clearance in copper pours\n- Other traces too close to via barrel\n- Component keepout zones blocking via\n\nVia Clearance: Must maintain minimum clearance on ALL layers, not just start/end layers.\n\nSolution:\n1. Move via to less congested area\n2. Increase anti-pad clearance in copper pours\n3. Use smaller via diameter (microvia)\n4. Route on different layers to avoid congestion")
    )]
    ViaPlacementBlocked { net_id: NetId, position: Point3D },

    /// Constraint-aware routing failed
    #[error("Constraint-aware routing failed for net {}: {}", .net_id.raw(), .message)]
    #[diagnostic(
        code(R24),
        url("https://docs.hw-script.org/errors/R24"),
        help("Physical Explanation: Routing failed to meet specific constraints such as:\n- Impedance control (trace width/spacing requirements)\n- Differential pair matching (length/spacing)\n- Maximum trace length (timing constraints)\n- Layer restrictions (high-speed signals)\n\nSolution:\n1. Review constraint requirements in profile definition (e.g., profiles.hw)\n2. Adjust component placement to reduce routing distance\n3. Use manual waypoints for critical nets\n4. Relax constraints if physically acceptable")
    )]
    ConstraintFailed { net_id: NetId, message: String },

    /// Maximum rip-up iterations exceeded
    #[error("Maximum rip-up iterations exceeded for net {}", .0.raw())]
    #[diagnostic(
        code(R31),
        url("https://docs.hw-script.org/errors/R31"),
        help("Physical Explanation: Router attempted to rip-up and reroute this net multiple times but failed to find a valid solution. This indicates:\n- Severe routing congestion\n- Conflicting constraints\n- Insufficient routing resources\n\nRip-up and reroute is a last-resort strategy when initial routing fails.\n\nSolution:\n1. Increase board size or add routing layers\n2. Reduce number of nets or component density\n3. Manual routing for critical nets\n4. Adjust routing priority (route critical nets first)")
    )]
    MaxIterationsExceeded(NetId),

    /// Invalid net ID
    #[error("Invalid net ID: {}", .0.raw())]
    #[diagnostic(
        code(R11),
        url("https://docs.hw-script.org/errors/R11"),
        help("Internal Error: Router received an invalid net ID. This is likely a compiler bug.\n\nPlease report this issue with your .hw file.")
    )]
    InvalidNet(NetId),
}
