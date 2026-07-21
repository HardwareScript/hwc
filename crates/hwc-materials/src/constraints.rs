//! Fabrication constraints for PCB and ASIC design
//!
//! This module defines constraint sets that specify manufacturing limits
//! for traces, vias, clearances, and layers.
//!
//! # v0.1.4 Architecture
//!
//! Constraints are now created from `define profile` blocks in .hw files:
//! 1. Parser creates `ProfileDefinition` AST nodes
//! 2. Symbol Table registers profiles during Pass 1
//! 3. Compiler converts `ProfileDefinition` → `ConstraintSet` via `profile_to_constraints()`
//! 4. Engine uses `ConstraintSet` for routing and validation
//!
//! See: `hwc-compiler/src/conversions.rs::profile_to_constraints()`

use compact_str::CompactString;
use miette::Diagnostic;
use thiserror::Error;

use crate::routing_intent::RoutingIntent;

#[derive(Error, Diagnostic, Debug)]
pub enum ConstraintError {
    #[error("Constraint profile not found: {0}")]
    #[diagnostic(
        code(M22),
        url("https://docs.hw-script.org/errors/M22"),
        help("Define a profile in your .hw file using 'define profile' syntax")
    )]
    NotFound(String),

    #[error("Failed to parse profile definition: {0}")]
    #[diagnostic(
        code(M31),
        url("https://docs.hw-script.org/errors/M31"),
        help("Verify profile syntax matches LANGUAGE-SPEC.md")
    )]
    ParseError(String),

    #[error("Failed to read file: {0}")]
    #[diagnostic(
        code(M32),
        url("https://docs.hw-script.org/errors/M32"),
        help("Verify file path exists and has read permissions")
    )]
    IoError(#[from] std::io::Error),

    #[error("Invalid constraint value: {0}")]
    #[diagnostic(
        code(M12),
        url("https://docs.hw-script.org/errors/M12"),
        help("Constraint values must be positive and within manufacturing limits")
    )]
    InvalidValue(String),
}

/// Bridge rule for multi-material continuity (v0.1.7)
#[derive(Debug, Clone)]
pub struct BridgeRule {
    pub from_material: CompactString,
    pub to_material: CompactString,
    pub interface_material: CompactString,
    pub fill_material: CompactString,
}

/// Whether a stackup layer permits routing (v0.1.8 Physical Synthesis Guardrails).
///
/// This is a table-driven constraint: each layer in the stackup declares its
/// routability mode. The pathfinder consults this table before placing trace
/// segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoutableMode {
    /// Full routing permitted (metal layers)
    True,
    /// No routing permitted (substrate, active, oxide)
    False,
    /// Local interconnects only with max length limit
    LocalOnly,
}

/// Complete constraint set for a fabrication process
#[derive(Debug, Clone)]
pub struct ConstraintSet {
    pub name: CompactString,
    pub description: CompactString,
    pub trace: TraceConstraints,
    pub via: ViaConstraints,
    pub clearance: ClearanceConstraints,
    pub layer: LayerConstraints,
    pub thermal: Option<ThermalConstraints>,
    pub stackup: Option<StackupConstraints>,
    pub bridges: Vec<BridgeRule>, // v0.1.7: Multi-material bridges
    /// Solder mask expansion in nanometers (v0.1.7)
    pub solder_mask_expansion_nm: Option<i64>,
    /// Number of segments used to approximate circular geometry (vias, pads,
    /// tubes, TSVs). Sourced from `manufacturing.circle_segments` in the PDK
    /// profile. No compiler default — must be declared.
    pub circle_segments: u32,
    /// Technology node string (e.g. "PCB", "ASIC") for manufacturing checks.
    pub technology: Option<String>,
    /// Per-layer routability map (v0.1.8 Physical Synthesis Guardrails).
    /// Maps layer name (e.g., "active", "metal1") to its routable mode.
    /// The pathfinder looks up this table before placing trace segments.
    pub layer_routability: rustc_hash::FxHashMap<CompactString, RoutableMode>,
    /// Maximum route length for `local_only` layers in nanometers (v0.1.8).
    /// Default: 10_000 nm (10µm).
    pub max_local_route_length_nm: Option<i64>,
    /// User-declared routing intents (CIR Phase 2.2).
    /// Replaces hardcoded `RoutingIntent::clock()` etc. with table-driven lookup.
    pub intents: Vec<RoutingIntent>,
}

/// Stackup constraints for impedance-controlled routing
#[derive(Debug, Clone)]
pub struct StackupConstraints {
    /// Dielectric height above ground plane in nanometers
    pub dielectric_height_nm: i64,

    /// Dielectric material name (e.g., "FR4", "Rogers4003")
    pub dielectric_material: CompactString,

    /// Copper thickness in nanometers
    pub copper_thickness_nm: i64,

    /// Relative permittivity (εr) of the dielectric material
    /// Common values: FR4 = 4.5, Rogers4003 = 3.38, Air = 1.0
    pub relative_permittivity: f64,

    /// Default target impedance for high-speed signals (optional)
    pub default_impedance_ohm: Option<f64>,
}

/// Trace width and spacing constraints
#[derive(Debug, Clone)]
pub struct TraceConstraints {
    /// Minimum trace width in nanometers
    pub min_width_nm: i64,

    /// Maximum trace width in nanometers (0 = unlimited)
    pub max_width_nm: i64,

    /// Minimum spacing between traces in nanometers
    pub min_spacing_nm: i64,

    /// Default trace width in nanometers
    pub default_width_nm: i64,
}

/// Via diameter and annular ring constraints
#[derive(Debug, Clone)]
pub struct ViaConstraints {
    /// Minimum via diameter in nanometers
    pub min_diameter_nm: i64,

    /// Maximum via diameter in nanometers (0 = unlimited)
    pub max_diameter_nm: i64,

    /// Minimum annular ring width in nanometers
    pub min_annular_ring_nm: i64,

    /// Minimum spacing between drill holes in nanometers (v0.1.7)
    pub min_spacing_nm: i64,

    /// Default via diameter in nanometers
    pub default_diameter_nm: i64,

    /// Via shape: "square" or "cylinder" (optional, defaults to cylinder)
    pub shape: Option<CompactString>,
}

/// Clearance constraints based on voltage
#[derive(Debug, Clone)]
pub struct ClearanceConstraints {
    /// Minimum clearance for low voltage (<50V) in nanometers
    pub low_voltage_nm: i64,

    /// Minimum clearance for medium voltage (50-150V) in nanometers
    pub medium_voltage_nm: i64,

    /// Minimum clearance for high voltage (>150V) in nanometers
    pub high_voltage_nm: i64,

    /// Safety factor multiplier (typically 2.0)
    pub safety_factor: f64,
}

/// Layer thickness and material constraints
#[derive(Debug, Clone)]
pub struct LayerConstraints {
    /// Minimum layer thickness in nanometers
    pub min_thickness_nm: i64,

    /// Maximum layer thickness in nanometers (0 = unlimited)
    pub max_thickness_nm: i64,

    /// Allowed conductor materials
    pub allowed_conductors: Vec<CompactString>,

    /// Allowed dielectric materials
    pub allowed_dielectrics: Vec<CompactString>,
}

/// Thermal constraints for physics validation
#[derive(Debug, Clone)]
pub struct ThermalConstraints {
    /// Ambient temperature in °C
    pub ambient_temp_c: f64,

    /// Maximum operating temperature in °C
    pub max_operating_temp_c: f64,

    /// Maximum allowed temperature rise in °C
    pub max_temp_rise_c: f64,

    /// Thermal clustering threshold in nanometers (optional)
    pub clustering_threshold_nm: Option<i64>,
}

impl ConstraintSet {
    /// Create an empty constraint set — ALWAYS PANICS.
    ///
    /// Fabrication constraints MUST come from the PDK profile. There are no
    /// silent defaults. Use `hwc_compiler::profile_to_constraints()` instead.
    pub fn empty() -> Self {
        panic!(
            "ConstraintSet::empty() is not allowed. Fabrication constraints must be \
             declared in the PDK profile. No silent defaults."
        )
    }

    /// Create constraint set from profile definition (v0.1.4)
    ///
    /// This is the primary way to create constraints in v0.1.4.
    /// Use `hwc_compiler::profile_to_constraints()` instead of calling this directly.
    ///
    /// # Example
    /// Use the compiler's profile_to_constraints function with the Symbol Table.
    pub fn from_profile_name(name: &str) -> Result<Self, ConstraintError> {
        Err(ConstraintError::NotFound(format!(
            "Profile '{}' not found. Use hwc_compiler::profile_to_constraints() with Symbol Table.",
            name
        )))
    }

    /// Get minimum trace width for a given layer
    pub fn get_min_trace_width(&self, _layer: usize) -> i64 {
        self.trace.min_width_nm
    }

    /// Get minimum clearance based on voltage difference
    pub fn get_min_clearance(&self, voltage_mv: i64) -> i64 {
        let voltage_v = voltage_mv / 1000;

        let base_clearance = if voltage_v < 50 {
            self.clearance.low_voltage_nm
        } else if voltage_v < 150 {
            self.clearance.medium_voltage_nm
        } else {
            self.clearance.high_voltage_nm
        };

        (base_clearance as f64 * self.clearance.safety_factor) as i64
    }

    /// Get minimum via diameter
    pub fn get_min_via_diameter(&self) -> i64 {
        self.via.min_diameter_nm
    }

    /// Validate trace width for a given layer
    pub fn validate_trace_width(
        &self,
        width_nm: i64,
        _layer: usize,
    ) -> Result<(), ConstraintError> {
        if width_nm < self.trace.min_width_nm {
            return Err(ConstraintError::InvalidValue(format!(
                "Trace width {}nm is below minimum {}nm",
                width_nm, self.trace.min_width_nm
            )));
        }

        if self.trace.max_width_nm > 0 && width_nm > self.trace.max_width_nm {
            return Err(ConstraintError::InvalidValue(format!(
                "Trace width {}nm exceeds maximum {}nm",
                width_nm, self.trace.max_width_nm
            )));
        }

        Ok(())
    }
}
