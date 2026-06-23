//! Space definition types

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::common::{Coordinate, Dimensions, Identifier, Measurement, OriginPoint, PinReference};
use super::component::ComponentPlacement;
use super::expression::Expression;
use super::pattern::PatternInstantiation;
use crate::lexer::Span;
use compact_str::CompactString;

/// Z-axis elevation (v0.1.7 Z-Axis Abstraction)
///
/// Replaces raw integer layer indices.
/// - Physical: Assembly paradigm (e.g. `z: 150um` or `z: start + 10um`)
/// - Semantic: High-Level paradigm (e.g. `layer: l1`), resolved via Profile stackup
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Elevation {
    /// Assembly paradigm: expression after `z:` (must evaluate to a measurement)
    /// v0.1.7 Phase 2.2: Supports range-based Z for exact material boundaries.
    Physical {
        start: Expression,
        end: Option<Expression>,
    },
    /// High-Level paradigm: semantic layer name from profile stackup
    Semantic(Identifier),
    /// Relative paradigm: `on layer: self` or `on z: relative`
    /// Resolves to the base elevation of the parent component.
    Relative,
}

impl Elevation {
    /// Returns true if this elevation uses physical units (Assembly paradigm).
    pub fn is_physical(&self) -> bool {
        matches!(self, Elevation::Physical { .. })
    }

    /// Returns true if this elevation uses a semantic layer name (High-Level paradigm).
    pub fn is_semantic(&self) -> bool {
        matches!(self, Elevation::Semantic(_))
    }

    /// Returns true if this elevation is relative to its parent (Relative paradigm).
    pub fn is_relative(&self) -> bool {
        matches!(self, Elevation::Relative)
    }

    /// Returns the start expression if this is a Physical elevation.
    pub fn as_physical_start(&self) -> Option<&Expression> {
        match self {
            Elevation::Physical { start, .. } => Some(start),
            Elevation::Semantic(_) | Elevation::Relative => None,
        }
    }

    /// Returns the end expression if this is a Physical elevation with a range.
    pub fn as_physical_end(&self) -> Option<&Expression> {
        match self {
            Elevation::Physical { end, .. } => end.as_ref(),
            Elevation::Semantic(_) | Elevation::Relative => None,
        }
    }

    /// Returns the layer identifier if this is a Semantic elevation.
    pub fn as_semantic_layer(&self) -> Option<&Identifier> {
        match self {
            Elevation::Physical { .. } | Elevation::Relative => None,
            Elevation::Semantic(id) => Some(id),
        }
    }
}

/// Global routing configuration for a space (v0.1.7)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub mode: RoutingMode,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingMode {
    /// Allow both automatic and manual routing (default)
    Mixed,
    /// Only allow manual routing (strict policy)
    ManualOnly,
}

/// Space definition: `space Name:` (v0.1.6)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceDefinition {
    pub name: Identifier,
    pub implements_module: Option<CompactString>, // NEW Phase 3: Optional module to validate against
    pub dimensions: Option<Dimensions>,
    /// New syntax (v0.1.8+): `resolution: 1nm`
    pub resolution: Option<Measurement>,
    pub origin: Option<OriginPoint>,
    pub profile: Option<Identifier>, // NEW v0.1.4: Reference to profile name
    pub mechanical: Option<Identifier>, // NEW v0.1.4: Reference to mechanical name
    pub substrate: Option<SubstratePlacement>,
    pub render: Option<super::component::RenderBlock>, // NEW v0.1.6: View/Visualization configuration
    pub routing_config: Option<RoutingConfig>,         // NEW v0.1.7: Global policy control

    /// **v0.1.7 CRITICAL FIX**: Unified statement stream that preserves textual order

    /// **v0.1.7 CRITICAL FIX**: Unified statement stream that preserves textual order
    ///
    /// This replaces the separate `components`, `pours`, `for_loops` vectors.
    /// Physical Reality: Atoms appear in the order you write them, not grouped by type.
    ///
    /// Before (v0.1.6): Compiler processed all `add` statements, then all `for` loops
    /// After (v0.1.7): Compiler processes statements in exact textual order
    ///
    /// This is CRITICAL for the `last` keyword to work correctly in complex designs.
    pub statements: Vec<SpaceTopLevelStatement>,

    // REMOVED (pre-release cleanup): Deprecated separate vectors (components/pours/polygons/contacts/for_loops) removed.
    // Why deprecated originally (v0.1.7): To support unified textual order for `last` keyword and physical reality of source ordering.
    // Why fully removed now: Pre-release (0.x), no backward compat burden allowed. Dual storage violated DRY/single-source-of-truth,
    // caused desync bugs during migration, increased struct size and serde complexity. Pattern to avoid repeating:
    // Never maintain parallel representations of the same data "temporarily for migration" — either migrate all consumers
    // atomically or keep only the new canonical form and update call sites immediately. See also removal of legacy AST, LayoutMapping, etc.
    // Consumers must now filter from `statements` or use helper iterators if added later.
    pub layouts: Vec<ModuleLayoutBlock>, // NEW: Module layout mappings
    pub routes: Vec<Route>,
    pub exposes: Vec<Expose>,
    pub nets: Vec<NetDeclaration>, // NEW v0.1.6: Net classifications for physics validation
    pub span: Span,
}

/// Top-level statement in a space block (v0.1.7)
///
/// This enum preserves the textual order of statements in the source file.
/// CRITICAL for `last` keyword to work correctly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpaceTopLevelStatement {
    /// Substrate placement: `add substrate(Material) ...`
    Substrate(SubstratePlacement),
    /// Component placement: `add ComponentType ...`
    Component(Box<ComponentPlacement>),
    /// Pour placement: `add pour(Material) ...`
    Pour(PourPlacement),
    /// Plane placement: `add plane(Material) ...` (conductive sheet)
    Plane(PlanePlacement),
    /// Polygon placement: `add polygon(Material) ...`
    Polygon(PolygonPlacement),
    /// Contact/via placement: `add contact(Material) ...`
    Contact(ContactPlacement),
    /// For loop: `for i in 0..7:`
    ForLoop(SpaceForLoop),
    /// Route: `route From.pin to To.pin`
    Route(Route),
    /// Expose: `expose Pin as Alias`
    Expose(Expose),
    /// v0.1.8: Prescriptive net-scoped route policy: `route net: NetName:`
    RouteNetPolicy(RouteNetPolicy),
}

/// v0.1.8: Prescriptive net-scoped route policy
///
/// Binds a pattern or strategy to an entire net globally, so the auto router
/// applies it to all Steiner tree segments for that net.
///
/// Example:
/// ```hardware
/// route net: ALL_PADS:
///     pattern: Zigzag(gap: 0.5mm)
///
/// route net: DDR5_BUS on layer: top:
///     pattern: Trombone(gap: 0.3mm, amp: 2.5mm)
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteNetPolicy {
    pub net_id: Identifier,
    pub target_layer: Option<Identifier>,
    pub pattern: Option<PatternInstantiation>,
    pub strategy: Option<Identifier>,
    pub span: Span,
}

/// For loop in space block (Sprint 3.4: Parametric Unrolling)
///
/// Allows parametric generation of components, pours, and routes.
/// Example:
/// ```hardware
/// for i in 0..8:
///     add Adder named Adder[i] at [x: i * 10mm, y: 0mm, z: 1]
///     route Adder[i].sum to Adder[i+1].carry
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceForLoop {
    pub variable: CompactString,
    pub start: usize,
    pub end: usize,
    pub body: Vec<SpaceStatement>,
    pub span: Span,
}

/// Statement inside a space for loop
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpaceStatement {
    /// Component placement
    Component(Box<ComponentPlacement>),
    /// Pour placement
    Pour(PourPlacement),
    /// Plane placement
    Plane(PlanePlacement),
    /// Contact placement
    Contact(ContactPlacement),
    /// Route
    Route(Route),
    /// Nested for loop
    ForLoop(Box<SpaceForLoop>),
}

/// Net classification for physics validation (v0.1.6)
///
/// Allows users to declare which nets are power/ground/signal.
/// Used by physics validator to check bulk biasing and other constraints.
///
/// Example:
/// ```hardware
/// space MyChip:
///     nets:
///         GND: { classification: ground, potential: 0V }
///         VDD: { classification: power, potential: 1.8V }
///         VOUT: { classification: signal }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetDeclaration {
    pub name: CompactString,
    pub classification: NetClassification,
    pub potential_mv: Option<i64>, // Optional voltage in millivolts
    /// v0.1.7: Optional signal frequency in Hz (e.g., 5_000_000_000.0 for 5 GHz).
    /// Used to classify high-speed nets that must avoid reference-plane voids.
    pub frequency_hz: Option<f64>,
    pub span: Span,
}

/// Net classification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetClassification {
    /// Power supply net (VDD, VCC, VDDA, etc.)
    Power,

    /// Ground net (GND, VSS, DGND, AGND, etc.)
    Ground,

    /// Signal net (data, clock, control, etc.)
    Signal,

    /// High voltage net (>150V) requiring special isolation
    HighVoltage,

    /// Bidirectional or unclassified
    Unclassified,
}

/// Substrate placement: `add Substrate(FR4) spanning [1,1,1] to [4,500,500]`
///
/// v0.1.7 Phase 2.2: Supports `cutouts:` property for manual solder mask/dielectric openings.
/// ```hardware
/// add substrate(Silicon_N) spanning [0,0,0] to [10mm,10mm,500um]:
///     cutouts:
///         - [x:2mm, y:2mm] to [x:3mm, y:3mm]
///         - [x:7mm, y:7mm] to [x:8mm, y:8mm]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubstratePlacement {
    pub material: CompactString,
    pub from: Coordinate,
    pub to: Coordinate,
    /// Cutouts (holes) in the substrate.
    /// Each cutout is a bounding box defined by a from/to coordinate pair.
    /// v0.1.7 Phase 2.2: Explicit substrate cutouts for solder mask/dielectric openings.
    pub cutouts: Vec<CoordinatePair>,
    pub span: Span,
}

/// A coordinate pair defining a bounding box region (for cutouts, keepouts, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoordinatePair {
    pub from: Coordinate,
    pub to: Coordinate,
}

/// Route: `route From.Pin to To.Pin:` with `path:` block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub from: PinReference,
    pub to: PinReference,
    pub width: Option<Expression>,
    pub layer: Option<Identifier>,     // v0.1.8: Target routing layer (e.g. metal1)
    pub strategy: Option<Identifier>, // e.g. DDR5_Match (references a strategy definition)
    pub pattern: Option<PatternInstantiation>, // v0.1.8: Direct pattern reference e.g. Trombone(gap: 0.3mm, amp: 2.5mm)
    pub strategy_params: Vec<(Identifier, Expression)>, // e.g. target_length: 50mm
    pub path: Option<Vec<Coordinate>>,
    pub signal_group: Option<CompactString>, // Optional signal group for impedance control
    pub bridge: Option<CompactString>,       // Phase 1: Explicit bridge override
    pub exit_escape: Option<RouteEscape>,    // v0.1.7: Exit port specification
    pub enter_escape: Option<RouteEscape>,   // v0.1.7: Enter port specification
    pub current_limit_ac: Option<CurrentLimitAc>, // v0.1.8: AC current limit [rms, peak]
    pub span: Span,
}

/// Route escape specification for port-based routing (v0.1.7)
///
/// Examples:
/// - `exit: East` -> Center of East edge
/// - `exit: East at top` -> Top of East edge
/// - `exit: East at 80%` -> 80% up the East edge
/// - `exit: East at +150um` -> 150um offset from center
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteEscape {
    pub port: CardinalDirection,
    pub offset: Option<EdgeOffsetSpec>,
    pub span: Span,
}

/// Cardinal direction for port escapes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardinalDirection {
    North,
    South,
    East,
    West,
}

/// Edge offset specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgeOffsetSpec {
    /// Named position: "top", "bottom", "center"
    Named(NamedPosition),
    /// Percentage: "80%" -> 0.8
    Percentage(f64),
    /// Physical measurement: "+150um" or "-50um"
    Measurement(i64),
}

/// Named positions for edge offsets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamedPosition {
    Top,
    Bottom,
    Center,
}

/// Expose: `expose Pin as Alias`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expose {
    pub pin: PinReference,
    pub alias: CompactString,
    pub span: Span,
}

/// Net name with optional array index (v0.1.6 Sprint 3.4)
/// Supports both simple names (`VDD`) and indexed names (`Bus[i]`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetName {
    pub base: CompactString,
    pub index: Option<super::Expression>,
    pub span: Span,
}

impl NetName {
    /// Create a simple net name without an index
    pub fn simple(name: CompactString, span: Span) -> Self {
        NetName {
            base: name,
            index: None,
            span,
        }
    }

    /// Create an indexed net name
    pub fn indexed(name: CompactString, index: super::Expression, span: Span) -> Self {
        NetName {
            base: name,
            index: Some(index),
            span,
        }
    }

    /// Convert to string representation (for display/debugging)
    pub fn to_string(&self) -> CompactString {
        if let Some(ref idx) = self.index {
            format!("{}[{}]", self.base, idx).into()
        } else {
            self.base.clone()
        }
    }

    /// Get the base name without index
    pub fn base_name(&self) -> &str {
        &self.base
    }
}

impl std::fmt::Display for NetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref idx) = self.index {
            write!(f, "{}[{}]", self.base, idx)
        } else {
            write!(f, "{}", self.base)
        }
    }
}

/// Copper pour: `add pour(Copper) named GND_Plane on layer: l1:` or `on z: 150um:` (v0.1.7)
///
/// Phase 4 (Silent Atom): Supports explicit device binding via `device:` property
/// Example: `device: M1.gate` binds this pour to M1's gate terminal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PourPlacement {
    pub material: CompactString,
    pub name: super::component::ComponentName,
    /// Z elevation (v0.1.7): either physical `z: 150um` or semantic `layer: l1`
    pub elevation: Elevation,
    pub thickness: Option<super::Expression>, // NEW v0.1.7: Explicit thickness override
    pub boundary: Option<PourBoundary>,       // Optional boundary (rect or circle)
    pub net: Option<NetName>, // Net name to connect to (v0.1.6: supports array syntax)
    pub device: Option<DeviceBinding>, // Phase 4: Explicit device terminal binding
    pub thermal_relief: bool,
    pub waivers: super::common::Waivers, // NEW v0.1.6 Sprint 8: Intentional overlap/connectivity waivers
    pub span: Span,
}

/// Pour boundary shape: rectangle or circle
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PourBoundary {
    /// Rectangular boundary: [from] to [to]
    Rect(Box<Coordinate>, Box<Coordinate>),
    /// Circular boundary: Circle(center, radius_expression)
    Circle {
        center: Box<Coordinate>,
        radius: crate::Expression,
    },
}

/// Device binding for explicit intent-based extraction (Phase 4: Silent Atom)
///
/// Binds a pour to a specific device terminal, eliminating geometric guessing.
/// Format: `device: DeviceName.terminal` (e.g., `device: M1.gate`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBinding {
    pub device_name: CompactString, // e.g., "M1"
    pub terminal: CompactString,    // e.g., "gate", "source", "drain", "bulk"
    pub span: Span,
}

/// Cutout shape for substrate and plane cutouts
///
/// Supports two shapes:
/// - `Rectangle { width, height, at }` — rectangular cutout at a position
/// - `Circle { radius, at }` — circular cutout at a position
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CutoutShape {
    Rectangle {
        width: Expression,
        height: Expression,
        at: Coordinate,
    },
    Circle {
        radius: Expression,
        at: Coordinate,
    },
}

/// Plane placement: `add plane(Material) named Name on layer: <layer>: ...`
///
/// Represents a conductive sheet (e.g., copper pour, ground plane).
/// Similar to PourPlacement but with explicit `cutouts` and semantic layer support.
///
/// ```hardware
/// add plane(Copper) named GND_Plane on layer: l1:
///     spanning layer: l1 to l1
///     net: GND
///     cutouts:
///         Rectangle(2mm, 1mm) at [x: 5mm, y: 5mm]
///         Circle(0.5mm) at [x: 10mm, y: 10mm]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanePlacement {
    pub material: CompactString,
    pub name: super::component::ComponentName,
    /// Z elevation: either physical `z: 150um` or semantic `layer: l1`
    pub elevation: Elevation,
    pub thickness: Option<Expression>,
    pub from: Option<Coordinate>,
    pub to: Option<Coordinate>,
    pub net: Option<NetName>,
    pub cutouts: Vec<CutoutShape>,
    pub span: Span,
}

/// AC current limit for route configuration
///
/// Parsed from: `current_limit: [rms: <Value>, peak: <Value>]`
/// Also supports single value: `current_limit: <Value>` (treated as DC, both rms and peak)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentLimitAc {
    pub rms: Expression,
    pub peak: Expression,
    pub span: Span,
}

/// Custom polygon: `add polygon(Copper) named WiFi_Antenna at [x:10, y:10, z:1]:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolygonPlacement {
    pub material: CompactString,
    pub name: super::component::ComponentName,
    pub position: Coordinate,              // Origin position
    pub points: SmallVec<[(f64, f64); 8]>, // Relative points (x, y) in mm
    pub span: Span,
}

/// Contact/Via placement: `add contact(Tungsten) at [x:500um, y:325um] spanning layer: l1 to l2` or physical Z (v0.1.7)
///
/// Vertical interconnects between layers. Automatically fills the via with conductive material.
///
/// Examples:
/// - `add contact(Tungsten) at [x:500um, y:325um] spanning layer: l1 to l2`
/// - `add contact(Copper) named Via1 net: VDD at [x:1mm, y:2mm] spanning z: 0um to 200um` (v0.1.7 dual paradigm)
///
/// Type of cap for tube shapes (v0.1.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapType {
    /// No cap (open end)
    None,
    /// Annular ring (disk with a hole)
    Annular,
    /// Solid disk (no hole)
    Solid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactPlacement {
    pub material: CompactString, // Via fill material (Tungsten, Copper, etc.)
    pub name: Option<super::component::ComponentName>, // Optional name for the via
    pub position: Coordinate,    // XY position (Z is ignored, use from/to elevation)
    /// Starting elevation (v0.1.7)
    pub from_elevation: Elevation,
    /// Ending elevation (v0.1.7)
    pub to_elevation: Elevation,
    pub net: Option<NetName>, // Optional net name to connect to (v0.1.6: supports array syntax)
    pub properties: rustc_hash::FxHashMap<CompactString, Expression>, // Generic properties (v0.1.9)
    /// Polygon contour for via shape (v0.2.0).
    /// The compiler only understands polygons, not named shapes.
    #[serde(skip)]
    pub contour: Option<clipper2_rust::Path64>,
    pub span: Span,
}

/// Layout block for mapping module internals: `layout ModuleName:`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleLayoutBlock {
    pub module_instance: CompactString, // Name of the module instance
    pub statements: Vec<LayoutStatement>, // Statements within the layout block
    pub span: Span,
}

/// Statement inside a layout block (mirrors ModuleStatement but for physical placement)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutStatement {
    /// Direct component placement: `Component at [x: 10, y: 20, z: 1]`
    Placement(ModuleInternalPlacement),
    /// For loop: `for i in 0..63:`
    For {
        variable: CompactString,
        start: usize,
        end: usize,
        body: Vec<LayoutStatement>,
        span: Span,
    },
    /// If conditional: `if i > 0:`
    If {
        condition: super::module::Condition,
        then_body: Vec<LayoutStatement>,
        else_body: Option<Vec<LayoutStatement>>,
        span: Span,
    },
}

/// Internal component placement within a module layout block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleInternalPlacement {
    pub component_name: CompactString, // Name of component within the module
    pub array_index: Option<super::module::ArrayIndex>, // Optional array index expression (e.g., [i], [i-1])
    pub position: Coordinate,
    pub span: Span,
}

impl SpaceDefinition {
    pub fn components(&self) -> Vec<ComponentPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Component(c) = s {
                    Some((**c).clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn pours(&self) -> Vec<PourPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Pour(p) = s {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn planes(&self) -> Vec<PlanePlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Plane(p) = s {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn polygons(&self) -> Vec<PolygonPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Polygon(p) = s {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn contacts(&self) -> Vec<ContactPlacement> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::Contact(c) = s {
                    Some(c.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn for_loops(&self) -> Vec<SpaceForLoop> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::ForLoop(f) = s {
                    Some(f.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn route_net_policies(&self) -> Vec<RouteNetPolicy> {
        self.statements
            .iter()
            .filter_map(|s| {
                if let SpaceTopLevelStatement::RouteNetPolicy(p) = s {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
