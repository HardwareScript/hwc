//! Physical Routing Data Structures & Native Type System
//!
//! All physical dimensions are enforced in fixed-point picometers ($1\text{ pm} = 10^{-12}\text{ m}$).

use compact_str::CompactString;
use hwc_engine::geometry::{BoundingBox, Point3D};
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

// ============================================================================
// 1. PIN ACCESS ANALYSIS (PAA) TYPES
// ============================================================================

/// Discrete on-grid access point pre-verified for via landing and enclosure.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessPoint {
    /// Exact 3D center coordinate in picometers
    pub point: Point3D,
    /// Physical metal layer index (0 for M1, 1 for M2, etc.)
    pub layer_idx: u8,
    /// Routability score based on track availability and pin-edge enclosure
    pub score: u16,
    /// Whether this access point matches the preferred layer routing direction
    pub is_preferred: bool,
}

/// Map of all pre-computed access points grouped by logical pin identifier.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PinAccessMap {
    /// Map: (Component Instance ID, Pin Name) -> Top K Access Points
    pub access_points: FxHashMap<(u32, CompactString), Vec<AccessPoint>>,
}

impl PinAccessMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        component_id: u32,
        pin_name: CompactString,
        points: Vec<AccessPoint>,
    ) {
        self.access_points.insert((component_id, pin_name), points);
    }

    pub fn get(&self, component_id: u32, pin_name: &str) -> Option<&Vec<AccessPoint>> {
        self.access_points
            .get(&(component_id, CompactString::new(pin_name)))
    }
}

// ============================================================================
// 2. 3D GLOBAL ROUTING TYPES (14-BYTE SoA L3-CACHE TENSOR)
// ============================================================================

/// 3D Volumetric Capacity Tensor using a flat Structure-of-Arrays (SoA) layout.
/// Memory footprint: Exactly 14 bytes per G-Cell (fits 100% in CPU L3 cache).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumetricTensor3D {
    pub dim_x: usize,
    pub dim_y: usize,
    pub dim_z: usize,

    pub gcell_width_pm: i64,
    pub gcell_height_pm: i64,

    /// Horizontal track capacity (2 bytes)
    pub cap_x: Vec<u16>,
    /// Vertical track capacity (2 bytes)
    pub cap_y: Vec<u16>,
    /// Present horizontal track occupancy (2 bytes)
    pub occ_x: Vec<u16>,
    /// Present vertical track occupancy (2 bytes)
    pub occ_y: Vec<u16>,
    /// Accumulated historical horizontal congestion cost (2 bytes)
    pub hist_x: Vec<u16>,
    /// Accumulated historical vertical congestion cost (2 bytes)
    pub hist_y: Vec<u16>,
    /// Base wire cost per layer/material (2 bytes)
    pub base_cost: Vec<u16>,
}

/// 3D G-Cell spatial envelope emitted by Global Routing.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GCellVolume3D {
    pub gcell_x: u16,
    pub gcell_y: u16,
    pub layer_idx: u8,
}

/// 3D routing corridor assigned to a net by Global Routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingGuide {
    pub net_id: NetId,
    pub volumes: Vec<GCellVolume3D>,
}

// ============================================================================
// 3. PANEL TRACK ASSIGNMENT (TA) TYPES
// ============================================================================

/// A continuous physical routing track spanning across a panel of G-Cells.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignedTrackSegment {
    pub net_id: NetId,
    pub layer_idx: u8,
    pub track_index: u32,
    pub start_coord_pm: i64,
    pub end_coord_pm: i64,
    pub fixed_axis_coord_pm: i64,
}

// ============================================================================
// 4. DETAILED ROUTING & PHYSICAL OUTPUT TYPES
// ============================================================================

/// Discrete routed wire segment in picometers.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutedTraceSegment {
    pub net_id: NetId,
    pub layer_name: CompactString,
    pub start: Point3D,
    pub end: Point3D,
    pub width_pm: i64,
}

/// Discrete routed vertical via instance.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutedViaInstance {
    pub net_id: NetId,
    pub position: Point3D,
    pub from_layer_name: CompactString,
    pub to_layer_name: CompactString,
    pub diameter_pm: i64,
}

/// Manufacturing cut-mask polygon for sub-2nm lithography (SAQP/High-NA EUV).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutMaskPolygon {
    pub layer_name: CompactString,
    pub bbox: BoundingBox,
}

/// Complete finalized routing solution returned to the compiler database.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutedOutput {
    pub traces: Vec<RoutedTraceSegment>,
    pub vias: Vec<RoutedViaInstance>,
    pub cut_masks: Option<Vec<CutMaskPolygon>>,
}
