//! Unified Endpoint Resolution types for routing.
//!
//! Routes are stored as stable EntityId references with escape port metadata.
//! Coordinates are NEVER stored — they are resolved from the EntityGraph
//! (the single source of truth) at routing time via `resolve_route_boundary_points`.

use compact_str::CompactString;
use hwc_engine::geometry::EntityId;
use hwc_engine::netlist::NetId;

/// Cardinal direction for port escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardinalDirection {
    North,
    South,
    East,
    West,
}

/// Edge offset for positioning along a pad edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeOffset {
    /// Center of the edge (default)
    Center,
    /// Normalized ratio 0.0..1.0 along the edge
    Percentage(f64),
    /// Absolute offset from edge center in nanometers
    MeasurementNm(i64),
}

/// Escape port specification for one endpoint of a route.
#[derive(Debug, Clone)]
pub struct EscapeSpec {
    pub port: CardinalDirection,
    pub offset: EdgeOffset,
}

impl Default for EscapeSpec {
    fn default() -> Self {
        Self {
            port: CardinalDirection::East,
            offset: EdgeOffset::Center,
        }
    }
}

/// A routing request with resolved entity endpoints.
///
/// Instead of passing absolute coordinate numbers between compilation stages,
/// routes are stored as stable EntityId references. The router queries the
/// EntityGraph directly for physical coordinates at routing time, guaranteeing
/// that the single source of truth (EntityGraph) is always used.
#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    /// The logical net ID for this route
    pub net_id: NetId,

    /// Stable identifier for the source entity (pin, pad, or space)
    pub from: EntityId,

    /// Stable identifier for the destination entity (pin, pad, or space)
    pub to: EntityId,

    /// Trace width in nanometers
    pub width_nm: i64,

    /// Net name for debugging and error messages
    pub net_name: CompactString,

    /// v0.2.0: Routing layer name (REQUIRED for database lookups)
    /// This is the single source of truth for which layer to route on.
    /// Database queries use this to find exact connection Z coordinates.
    pub layer_name: CompactString,

    /// Optional layer override (target Z coordinate)
    /// DEPRECATED in v0.2.0: Use layer_name + database instead
    pub target_layer_z: Option<i64>,

    /// Operating current capability in milliamps (from current_limit_ac.peak)
    pub current_capability_ma: f64,

    /// Actual operating current in milliamps (from net declaration)
    pub actual_current_ma: f64,

    /// Escape port for the source endpoint (resolved by chain-link logic)
    pub exit_escape: EscapeSpec,

    /// Escape port for the destination endpoint (resolved by chain-link logic)
    pub enter_escape: EscapeSpec,
}

impl ResolvedRoute {
    /// Create a new resolved route with entity endpoints.
    ///
    /// v0.2.0: layer_name is now REQUIRED for database-driven routing.
    pub fn new(
        net_id: NetId,
        from: EntityId,
        to: EntityId,
        width_nm: i64,
        net_name: CompactString,
        layer_name: CompactString,
    ) -> Self {
        Self {
            net_id,
            from,
            to,
            width_nm,
            net_name,
            layer_name,
            target_layer_z: None,
            current_capability_ma: 0.0,
            actual_current_ma: 0.0,
            exit_escape: EscapeSpec::default(),
            enter_escape: EscapeSpec::default(),
        }
    }

    /// Set the optional layer override
    pub fn with_layer_override(mut self, z: i64) -> Self {
        self.target_layer_z = Some(z);
        self
    }

    /// Set current ratings
    pub fn with_currents(mut self, capability_ma: f64, actual_ma: f64) -> Self {
        self.current_capability_ma = capability_ma;
        self.actual_current_ma = actual_ma;
        self
    }

    /// Set escape port specifications
    pub fn with_escapes(mut self, exit: EscapeSpec, enter: EscapeSpec) -> Self {
        self.exit_escape = exit;
        self.enter_escape = enter;
        self
    }
}
