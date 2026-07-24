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

/// Whether a stackup layer permits routing (v0.1.8 Physical Synthesis Guardrails).
///
/// This is a table-driven constraint: each layer in the stackup declares its
/// routability mode. The pathfinder consults this table before placing trace
/// segments, ensuring no trace lands on a non-routable layer (e.g., `active`,
/// `substrate`, `oxide`).
///
/// # Modes
/// - `True`: Full routing permitted (metal layers like metal1, metal2)
/// - `False`: No routing permitted (substrate, active, oxide layers)
/// - `LocalOnly`: Local interconnects permitted with a max length limit;
///   may exit component boundaries briefly for gate-to-gate ties (e.g., poly)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutableMode {
    /// Full routing permitted on this layer
    True,
    /// No routing permitted on this layer
    False,
    /// Local interconnects only — bounded by `max_local_route_length`
    LocalOnly,
}

/// Routing constraints from the profile's `routing:` block (v0.1.7).
///
/// Controls gridded routing behavior for ASIC designs.
/// v0.1.8: All routing heuristic weights come from the PDK profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingConstraints {
    /// Per-layer routing direction preferences.
    /// Maps layer name (e.g., "m1", "m2") to preferred direction.
    pub layer_directions: rustc_hash::FxHashMap<String, RoutingDirection>,
    /// Maximum route length for `routable: local_only` layers (v0.1.8).
    /// When a trace on a local_only layer exceeds this length outside a
    /// component bounding box, the pathfinder rejects the segment.
    /// Default: 10µm (10_000 nm).
    pub max_local_route_length: Option<Measurement>,
    /// Minimum segment length when collapsing a pathfinder path to segments.
    /// Required — no compiler default. Short collinear stubs below this length
    /// are dropped; real turns at or above it are preserved.
    pub min_segment_length: Option<Measurement>,
    /// Topological router: cost per grid step (base movement cost).
    pub base_cost: Option<i64>,
    /// Topological router: penalty for via transitions (layer changes).
    /// Higher values discourage vias. Default: 50.
    pub via_penalty: Option<i64>,
    /// Topological router: penalty for moving against preferred layer direction.
    /// Higher values enforce stricter direction adherence. Default: 10.
    pub direction_penalty: Option<i64>,
    /// Topological router: penalty when clearance is tight.
    /// Applied when min_clearance < 2 * min_trace_width. Default: 2.
    pub tight_clearance_penalty: Option<i64>,
    /// Topological router: penalty for crosstalk risk (long parallel runs).
    /// Applied when max_parallel_length < 10 * min_trace_width. Default: 3.
    pub crosstalk_penalty: Option<i64>,
    /// Topological router: penalty for impedance-controlled nets.
    /// Default: 1.
    pub impedance_penalty: Option<i64>,
    /// Topological router: extreme penalty for crossing reference-plane voids.
    /// Applied to high-speed nets. Default: 5_000_000.
    pub reference_void_penalty: Option<i64>,
    /// Net routing priorities from PDK profile.
    /// Maps net name to priority (higher = routed first).
    /// v0.1.8 ZERO-MAGIC: Priority must be declared here, not guessed from names.
    pub net_priorities: rustc_hash::FxHashMap<String, u8>,
    /// Default perpendicular escape stub length (v0.1.9 Declarative Escape Policies).
    /// Distance the trace must travel perpendicular to the pad edge before turning.
    /// - 0nm: Turn immediately (flush with pad edge)
    /// - >0nm: Enforces perpendicular escape segment
    /// Can be overridden by net_type intent or individual route declarations.
    pub escape_stub: Option<Measurement>,
    pub span: Span,
}

/// A user-declared routing intent (CIR Phase 2.2).
///
/// Syntax in profile:
/// ```hw
/// intent Clock:
///     routing_style: straight
///     cost_weights:
///         base: 10
///         via_penalty: 500
/// ```
///
/// This replaces the old hardcoded `RoutingIntent::clock()` with a
/// table-driven approach: users declare intents in their PDK profile,
/// and the compiler looks them up by name at routing time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileIntent {
    /// Intent name (e.g., "Clock", "Power", "Signal").
    pub name: Identifier,
    /// Routing style preference for this intent.
    /// Known styles: "straight", "manhattan", "auto".
    pub routing_style: Option<Identifier>,
    /// Cost weight overrides for this intent.
    /// If not specified, the global routing cost weights are used.
    pub cost_weights: Option<CostWeights>,
    /// Escape stub override for this intent (v0.1.9).
    /// Overrides the global routing.escape_stub for nets with this intent.
    pub escape_stub: Option<Measurement>,
    pub span: Span,
}

/// Cost weight overrides for a routing intent.
///
/// Syntax in profile (inside `intent` block):
/// ```hw
/// cost_weights:
///     base: 10
///     via_penalty: 500
///     direction_penalty: 20
///     tight_clearance_penalty: 5
///     crosstalk_penalty: 10
///     impedance_penalty: 3
///     reference_void_penalty: 10000000
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostWeights {
    pub base: Option<i64>,
    pub via_penalty: Option<i64>,
    pub direction_penalty: Option<i64>,
    pub tight_clearance_penalty: Option<i64>,
    pub crosstalk_penalty: Option<i64>,
    pub impedance_penalty: Option<i64>,
    pub reference_void_penalty: Option<i64>,
    pub span: Span,
}

/// Profile definition: `profile Name:` (v0.1.6)
/// v0.2.0: Supports optional `export` keyword for visibility control
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileDefinition {
    pub name: Identifier,
    pub is_exported: bool, // v0.2.0: Access control
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
    /// User-declared routing intents (CIR Phase 2.2).
    /// Replaces hardcoded `RoutingIntent::clock()` etc. with a table-driven approach.
    pub intents: Vec<ProfileIntent>,
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
    /// Interface thickness (e.g., 50nm)
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
    /// Number of segments used to approximate circular geometry (vias, pads,
    /// tubes, TSVs). Declared in the PDK profile — no compiler default. This
    /// is the single source of truth consumed by both geometry generation and
    /// mesh export so the two never disagree on circle fidelity.
    pub circle_segments: Option<usize>,
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
    pub edge_clearance: Option<Measurement>,
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
    /// List of materials permitted for conductive traces/pours.
    /// v0.1.8: Replaces hardcoded "copper" default.
    pub allowed_conductors: Vec<CompactString>,
    /// List of materials permitted for dielectric isolation.
    /// v0.1.8: Replaces hardcoded "fr4", "air" defaults.
    pub allowed_dielectrics: Vec<CompactString>,
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

    /// Whether this layer permits routing (v0.1.8 Physical Synthesis Guardrails).
    /// Table-driven constraint — the pathfinder consults this before placing
    /// trace segments. `None` means the field was not declared (legacy files
    /// default to `true` for backward compatibility).
    pub routable: Option<RoutableMode>,
}

/// Export & Visualization constraints (v0.1.6: Anti-Aliasing Switch)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportConstraints {
    /// Enable anti-aliasing/smoothing for discrete-to-vector conversion
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
