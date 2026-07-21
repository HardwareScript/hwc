//! Core CIR type definitions: InterfaceId, Normal2D, Orientation, DerivedConstraint, RoutingDatabase.

use crate::geometry::Point3D;

// ─── Interface ID ───────────────────────────────────────────────────────────

/// Strongly-typed interface ID (newtype wrapper around u32).
///
/// Allocated by `EntityGraph` during component registration. Each physical
/// contact point on a component receives a unique `InterfaceId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InterfaceId(pub u32);

impl InterfaceId {
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

// ─── Normal Vector (Fixed-Point) ────────────────────────────────────────────

/// Fixed-point integer normal vector scaled by 10^9.
///
/// Used for deterministic geometry operations without floating-point math.
/// All normal vectors are unit-length in the scaled coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Normal2D {
    /// x component * 10^9
    pub x: i32,
    /// y component * 10^9
    pub y: i32,
}

impl Normal2D {
    /// Scale factor for fixed-point representation (10^9).
    pub const SCALE: i64 = 1_000_000_000;

    /// Create a new normal vector from scaled components.
    #[inline(always)]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Normal pointing North (0, +1).
    pub const NORTH: Self = Self::new(0, Self::SCALE as i32);

    /// Normal pointing South (0, -1).
    pub const SOUTH: Self = Self::new(0, -(Self::SCALE as i32));

    /// Normal pointing East (+1, 0).
    pub const EAST: Self = Self::new(Self::SCALE as i32, 0);

    /// Normal pointing West (-1, 0).
    pub const WEST: Self = Self::new(-(Self::SCALE as i32), 0);

    /// Zero normal (degenerate).
    pub const ZERO: Self = Self::new(0, 0);

    /// Convert to a direction vector (dx, dy) with magnitude 1.
    #[inline]
    pub fn to_unit_direction(&self) -> (i64, i64) {
        let sx = self.x as i64;
        let sy = self.y as i64;
        if sx == 0 && sy == 0 {
            return (0, 0);
        }
        if sx.abs() >= sy.abs() {
            if sx >= 0 {
                (1, 0)
            } else {
                (-1, 0)
            }
        } else if sy >= 0 {
            (0, 1)
        } else {
            (0, -1)
        }
    }
}

// ─── Orientation ─────────────────────────────────────────────────────────────

/// How an interface's normal direction is determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// Compute normals automatically from geometry edges
    Derived,
    /// Use an explicitly provided normal vector
    Explicit(Normal2D),
    /// No directional information (point contacts)
    None,
}

// ─── Derived Constraints ─────────────────────────────────────────────────────

/// Routing constraints derived from interface capabilities.
///
/// These are consumed by the router, DRC checker, EM solver, and IR drop analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DerivedConstraint {
    /// Minimum trace width in nanometers
    MinimumTraceWidth(i64),
    /// Maximum trace length in nanometers
    MaximumTraceLength(i64),
    /// Thermal via required at this interface
    ThermalViaRequired,
    /// No constraint derived
    None,
}

// ─── Routing Database Trait ──────────────────────────────────────────────────

/// Technology database for capability constraint derivation.
///
/// Provides access to material properties and technology parameters needed
/// to convert interface capabilities into concrete routing constraints.
pub trait RoutingDatabase {
    /// Maximum current density in microamps per nanometer of trace width.
    fn get_max_current_density_ua_per_nm(&self) -> i64;

    /// Local temperature at a position (in millikelvin).
    fn get_local_temperature_at(&self, pos: Point3D) -> i64;

    /// Current density at a position (in uA/nm²).
    fn get_current_density_at(&self, pos: Point3D) -> i64;

    /// Nearest parallel trace distance at a position (in nm).
    fn get_nearest_parallel_trace_distance(&self, pos: Point3D) -> i64;

    /// Whether a position is in a reference plane void.
    fn is_in_reference_void(&self, pos: Point3D) -> bool;
}
