//! Core types for physics validation

use crate::geometry::Point3D;
use crate::voxel_grid::NetId;
use crate::voxel_grid::MaterialId;
use compact_str::CompactString;
use std::fmt;

/// Physical violation detected during validation
#[derive(Debug, Clone)]
pub enum PhysicsViolation {
    /// Short circuit detected between two nets
    ShortCircuit {
        net_a: NetId,
        net_b: NetId,
        location: Point3D,
    },

    /// Clearance violation (traces too close)
    ClearanceViolation {
        net_a: NetId,
        net_b: NetId,
        actual_clearance_nm: i64,
        required_clearance_nm: i64,
        location: Point3D,
    },

    /// Voltage boundary violation (voltage exceeds material rating)
    VoltageBoundary {
        net: NetId,
        voltage_mv: i64,
        max_mv: i64,
        location: Point3D,
    },

    /// Thermal hotspot detected
    ThermalHotspot {
        nets: Vec<NetId>,
        location: Point3D,
        combined_power_mw: f64,
        temperature_rise_c: f64,
    },

    /// Substrate short circuit (conductor touching substrate without liner)
    ///
    /// Specifically for TSVs, this occurs if the conductive core touches
    /// the silicon substrate without an insulator liner.
    SubstrateShortCircuit {
        net: NetId,
        substrate_material: MaterialId,
        location: Point3D,
    },

    /// Keep-out zone violation (geometry placed inside a forbidden KOZ)
    KozViolation {
        net: NetId,
        location: Point3D,
        reason: CompactString,
    },
}

impl fmt::Display for PhysicsViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhysicsViolation::ShortCircuit {
                net_a,
                net_b,
                location,
            } => {
                write!(f, "Short circuit between net {} and {} at {}", net_a, net_b, location)
            }
            PhysicsViolation::ClearanceViolation {
                net_a,
                net_b,
                actual_clearance_nm,
                required_clearance_nm,
                location,
            } => {
                write!(
                    f,
                    "Clearance violation between net {} and {}: actual {}nm, required {}nm at {}",
                    net_a, net_b, actual_clearance_nm, required_clearance_nm, location
                )
            }
            PhysicsViolation::VoltageBoundary {
                net,
                voltage_mv,
                max_mv,
                location,
            } => {
                write!(
                    f,
                    "Voltage violation for net {}: {}mV exceeds max {}mV at {}",
                    net, voltage_mv, max_mv, location
                )
            }
            PhysicsViolation::ThermalHotspot {
                nets,
                location,
                temperature_rise_c,
                ..
            } => {
                write!(
                    f,
                    "Thermal hotspot at {}: {:.1}°C rise (nets: {:?})",
                    location, temperature_rise_c, nets
                )
            }
            PhysicsViolation::SubstrateShortCircuit {
                net,
                substrate_material,
                location,
            } => {
                write!(
                    f,
                    "Substrate short circuit: net {} touching substrate material {} at {}",
                    net, substrate_material, location
                )
            }
            PhysicsViolation::KozViolation {
                net,
                location,
                reason,
            } => {
                write!(
                    f,
                    "Keep-out zone violation for net {} at {}: {}",
                    net, location, reason
                )
            }
        }
    }
}

/// Physics validation report
#[derive(Debug, Clone)]
pub struct PhysicsValidationReport {
    pub violations: Vec<PhysicsViolation>,
    pub validation_time_ms: f64,
    pub voxels_checked: usize,
    pub chunks_checked: usize,
}

impl PhysicsValidationReport {
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
            validation_time_ms: 0.0,
            voxels_checked: 0,
            chunks_checked: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn total_violations(&self) -> usize {
        self.violations.len()
    }

    /// Calculate throughput in voxels per second
    pub fn throughput_voxels_per_sec(&self) -> f64 {
        if self.validation_time_ms > 0.0 {
            (self.voxels_checked as f64 / self.validation_time_ms) * 1000.0
        } else {
            0.0
        }
    }
}

impl Default for PhysicsValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Packed net properties for O(1) array-indexed access
///
/// This struct replaces HashMap lookups with direct array indexing.
/// NetID is used as the array index, eliminating all hashing overhead.
///
/// # Memory Layout
///
/// - 8 bytes: voltage (i64)
/// - 8 bytes: clearance (i64)
/// - 8 bytes: current_density (f64)
/// - 4 bytes: layer_mask (u32)
///
/// Total: 28 bytes per net (cache-friendly)
#[derive(Debug, Clone, Copy, Default)]
pub struct NetProperties {
    /// Voltage in millivolts
    pub voltage_mv: i64,

    /// Clearance requirement in nanometers
    pub clearance_nm: i64,

    /// Current density in mA/mm²
    pub current_density_ma_mm2: f64,

    /// Layer mask for future multi-layer routing (currently unused)
    pub layer_mask: u32,
}
