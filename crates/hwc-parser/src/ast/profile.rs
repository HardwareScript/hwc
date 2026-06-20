//! Profile definition types

use serde::{Deserialize, Serialize};

use super::common::{Identifier, Measurement};
use super::expression::Expression;
use crate::lexer::Span;
use compact_str::CompactString;

/// Preferred routing direction for a metal layer (v0.1.7 ASIC extension).
///
/// Odd metal layers (M1, M3, M5) typically prefer horizontal routing,
/// while even layers (M2, M4, M6) prefer vertical. This maximizes
/// routing density and prevents wire deadlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingDirection {
    /// Horizontal routing preferred (East-West)
    Horizontal,
    /// Vertical routing preferred (North-South)
    Vertical,
    /// No direction preference (power/ground planes)
    Any,
}

/// Routing constraints from the profile's `routing:` block (v0.1.7).
///
/// Controls gridded routing behavior for ASIC designs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingConstraints {
    /// Per-layer routing direction preferences.
    /// Maps layer name (e.g., "m1", "m2") to preferred direction.
    pub layer_directions: rustc_hash::FxHashMap<String, RoutingDirection>,
    pub span: Span,
}

/// Profile definition: `profile Name:` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileDefinition {
    pub name: Identifier,
    pub description: Option<CompactString>,
    pub trace: Option<TraceConstraints>,
    pub via: Option<ViaConstraints>,
    pub layer: Option<LayerConstraints>,
    pub clearance: Option<ClearanceConstraints>,
    pub thermal: Option<ThermalConstraints>,
    pub manufacturing: Option<ManufacturingConstraints>,
    /// Physical layer stackup (v0.1.7 Z-Axis Abstraction)
    /// Single source of truth for named layers and their physical thicknesses.
    /// Replaces the old impedance-only StackupConstraints.
    pub stackup: Option<LayerStackup>,
    pub export: Option<ExportConstraints>, // v0.1.6: Export & visualization rules
    /// Routing constraints (v0.1.7): layer direction preferences for ASIC gridded routing.
    pub routing: Option<RoutingConstraints>,
    /// Bridge rules for material transitions (Phase 1 - BRIDGE-IMPLEMENTATION.md)
    /// Syntax: `bridge FromMaterial to ToMaterial: BridgeMaterial`
    pub bridges: Vec<BridgeRule>,
    /// Explicit via definitions (v0.1.7)
    pub vias: Vec<ViaDefinition>,
    pub technology: Option<String>,
    pub other: rustc_hash::FxHashMap<CompactString, String>, // v0.1.6: Custom constraint blocks
    pub span: Span,
}

impl ProfileDefinition {
    /// Returns the thickness expression for a named layer in the stackup.
    /// v0.1.7: Used by the unroller to resolve dynamic pad thicknesses.
    pub fn get_layer_thickness(&self, layer_name: &str) -> Option<&Expression> {
        self.stackup.as_ref().and_then(|s| {
            s.layers
                .iter()
                .find(|l| l.name.name == layer_name)
                .map(|l| &l.thickness)
        })
    }

    /// Returns true if this profile is an ASIC (Manhattan) profile.
    ///
    /// ASIC profiles use Manhattan (90°) routing constraints and require
    /// layer-by-layer via tower unrolling. Detection is based on:
    /// 1. Profile name containing "asic" (case-insensitive)
    /// 2. Presence of a `routing:` block with layer direction preferences
    /// 3. Grid snapping enabled in manufacturing constraints
    pub fn is_asic(&self) -> bool {
        self.name.name.to_lowercase().contains("asic")
            || self.routing.is_some()
            || self
                .manufacturing
                .as_ref()
                .is_some_and(|m| m.grid_snapping.unwrap_or(false))
    }
}

/// Bridge rule: maps a material transition to a specific bridge material.
///
/// Syntax in profile: `bridge Silicon to Copper: Titanium_Silicide`
/// Syntax in space:   `bridge: Titanium_Silicide` (explicit override)
///
/// The compiler resolves material names against the MaterialDatabase at build time.
///
/// **Phase 1 Update**: Support compound stacks (interface + fill materials)
/// Syntax: `bridge Silicon to Copper:`
///           `interface: Titanium_Silicide`
///           `thickness: 50nm`
///           `fill: Tungsten`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeRule {
    /// Source material name (e.g., "Silicon")
    pub from: CompactString,
    /// Destination material name (e.g., "Copper")
    pub to: CompactString,
    /// Bridge interface material name (e.g., "Titanium_Silicide")
    /// This is the thin layer that touches the source material
    pub interface_material: CompactString,
    /// Interface thickness (e.g., 50nm) - typically 1 voxel
    pub interface_thickness: Option<Measurement>,
    /// Via fill material (e.g., "Tungsten") - fills the rest of the via
    pub fill_material: Option<CompactString>,
    pub span: Span,
}

/// Explicit via definition within a profile (v0.1.7).
///
/// Syntax:
/// ```hw
/// via Microvia_1:
///     diameter: 0.3mm
///     annular_ring: 0.15mm
///     spanning: inner2 to inner1
///     material: Copper
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViaDefinition {
    pub name: Identifier,
    pub diameter: Measurement,
    pub annular_ring: Measurement,
    pub from_layer: Identifier,
    pub to_layer: Identifier,
    pub material: Option<Identifier>,
    pub span: Span,
}

/// Manufacturing constraints (IPC-2221 formulas, copper thickness, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingConstraints {
    pub copper_thickness: Option<Measurement>,
    pub ipc2221_k_external: Option<f64>,
    pub ipc2221_k_internal: Option<f64>,
    pub min_feature_size: Option<Measurement>,
    pub solder_mask_expansion: Option<Measurement>,
    /// Solder mask thickness (default: 20µm). Applied on outer surfaces.
    /// Components mounted on top/bottom sit on the mask, not on copper.
    pub solder_mask_thickness: Option<Measurement>,
    // v0.1.7 ASIC Extensions
    /// Track pitch for gridded routing (ASIC only). Snaps traces to manufacturing grid.
    pub track_pitch: Option<Measurement>,
    /// Whether to snap traces to the routing grid (ASIC only).
    pub grid_snapping: Option<bool>,
    /// Dummy fill toggle (thieving pass for copper density uniformity).
    pub dummy_fill: Option<bool>,
    /// Target copper density for dummy fill (0.0–1.0, default: 0.45).
    pub dummy_fill_density: Option<f64>,
    /// Dummy fill element size.
    pub dummy_fill_size: Option<Measurement>,
    /// Dummy fill spacing between elements.
    pub dummy_fill_spacing: Option<Measurement>,
    pub span: Span,
}

/// Trace constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceConstraints {
    pub min_width: Measurement,
    pub min_spacing: Measurement,
    pub max_width: Option<Measurement>,
    pub max_length: Option<Measurement>,
    pub span: Span,
}

/// Via constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViaConstraints {
    pub min_diameter: Measurement,
    pub min_annular_ring: Measurement,

    /// Default diameter to use when placing vias (if not specified)
    pub default_diameter: Option<Measurement>,

    /// Minimum spacing between via centers (e.g., 600µm)
    /// Used for via array generation on power/ground nets
    pub min_spacing: Option<Measurement>,
    pub max_aspect_ratio: Option<f64>,
    pub default_via_fill: Option<Identifier>,
    /// Via shape: "square" or "cylinder"
    pub shape: Option<Identifier>,
    // v0.1.7 ASIC Extensions
    /// Per-layer enclosure (annular ring) constraints.
    /// Maps layer name to minimum enclosure distance.
    pub enclosures: Option<rustc_hash::FxHashMap<String, Measurement>>,
    /// Whether stacked vias are permitted (ASIC: false, PCB: true).
    pub allow_stacked_vias: Option<bool>,
    /// Minimum stagger offset between stacked vias (if allowed).
    pub min_stagger_offset: Option<Measurement>,

    pub span: Span,
}

/// Layer constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerConstraints {
    pub max_count: Option<usize>,
    pub min_thickness: Option<Measurement>,
    pub span: Span,
}

/// Clearance constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearanceConstraints {
    pub high_voltage: Option<Measurement>,
    pub safety_factor: Option<f64>,
    pub low_voltage_threshold: Option<Measurement>,
    pub medium_voltage_threshold: Option<Measurement>,
    pub high_voltage_threshold: Option<Measurement>,
    pub span: Span,
}

/// Thermal constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalConstraints {
    pub ambient_temp: Measurement,
    pub max_operating_temp: Measurement,
    pub max_temp_rise: Measurement,
    pub clustering_threshold: Option<Measurement>,
    pub span: Span,
}

/// Physical layer stackup (v0.1.7 Z-Axis Abstraction - Breaking Change)
///
/// This is the single source of truth for layer names and physical thicknesses.
/// The compiler uses this to resolve `Elevation::Semantic` (e.g. `layer: l1`)
/// into absolute nanometer Z coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerStackup {
    pub layers: Vec<StackupLayer>,
}

/// One layer in the physical stackup
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackupLayer {
    /// Layer name as used in `on layer: l1` or `spanning layer: l1 to l2`
    pub name: Identifier, // e.g. l1, d1, inner1, bottom

    /// Material name (looked up in the Material Database)
    pub material: CompactString, // e.g. "Copper", "FR4"

    /// Physical thickness (can be a measurement or expression)
    pub thickness: Expression,
}

/// Export & Visualization constraints (v0.1.6: Anti-Aliasing Switch)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportConstraints {
    /// Enable anti-aliasing/smoothing for voxel-to-vector conversion
    pub antialiasing: bool,

    /// Maximum deviation allowed during smoothing (e.g., 5nm)
    pub smoothing_tolerance: Option<Measurement>,

    /// Angles to preserve during smoothing (e.g., [45, 90])
    pub corner_lock: Option<Vec<u32>>,

    pub span: Span,
}

impl Default for ExportConstraints {
    fn default() -> Self {
        Self {
            antialiasing: false, // Conservative default: no smoothing
            smoothing_tolerance: None,
            corner_lock: Some(vec![45, 90]), // Preserve common design angles
            span: Span::new(0, 0),
        }
    }
}
