//! Type definitions for the G-Cell sweep engine.

use compact_str::CompactString;

/// A lightweight DRC violation for the sweep engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SweepViolation {
    pub net_a: u32,
    pub net_b: u32,
    pub location: (i64, i64),
    pub violation_type: ViolationType,
}

/// Types of DRC violations detected by the sweep engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViolationType {
    /// Clearance between two different nets is insufficient.
    ClearanceViolation { required: i64, actual: i64 },
    /// Two different nets are shorted (zero clearance).
    ShortCircuit,
    /// Same-net overlap not at a valid VirtualJunction or component port.
    SameNetOverlap,
    /// v0.1.8: Coplanar forbidden junction — conductor touching semiconductor
    /// without an intermediate ohmic contact bridge.
    ForbiddenJunction {
        mat_a: CompactString,
        mat_b: CompactString,
    },
}

/// v0.1.8: Classification of a material junction between two touching geometries.
///
/// This is a table-driven classification: the DRC engine queries the material
/// registry for conductivity categories and the bridge table for registered
/// transitions. No hard-coding — all rules come from the profile + material DB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JunctionClassification {
    /// The junction is allowed (same category, or insulator involved).
    Allowed,
    /// A bridge is required and has been declared in the profile.
    /// Contains the bridge material name for diagnostic suggestions.
    BridgeRequired { bridge: CompactString },
    /// The junction is forbidden — conductor touching semiconductor with
    /// no declared bridge. This is a hard error.
    Forbidden,
}

/// Bridge table lookup key: "FromMaterial:ToMaterial" → bridge material name.
pub type BridgeTable = rustc_hash::FxHashMap<CompactString, CompactString>;
