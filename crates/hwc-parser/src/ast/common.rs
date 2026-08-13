//! Common types shared across AST nodes

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::lexer::Span;

/// Identifier with span information for better error messages
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identifier {
    pub name: CompactString,
    pub span: Span,
}

impl Identifier {
    /// Create a new identifier
    pub fn new(name: CompactString, span: Span) -> Self {
        Identifier { name, span }
    }

    /// Create an identifier with a dummy span (useful for testing and programmatic construction)
    pub fn with_dummy_span(name: &str) -> Self {
        Identifier {
            name: name.to_string().into(),
            span: Span { start: 0, end: 0 },
        }
    }

    /// Get the identifier name as a string slice
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl std::str::FromStr for Identifier {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Identifier {
            name: s.to_string().into(),
            span: Span { start: 0, end: 0 },
        })
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.name)
    }
}

impl std::hash::Hash for Identifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Only hash the name, not the span
        // Two identifiers with the same name but different spans should hash the same
        self.name.hash(state);
    }
}

/// Measurement: number + unit (e.g., `50mm`, `4.7kΩ`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub value: f64,
    pub unit: Unit,
    pub span: Span,
}

impl Measurement {
    /// Convert this measurement to picometers (i64).
    /// The engine always works in picometers internally.
    /// Returns `None` if the unit is not a distance unit.
    pub fn to_picometers_i64(&self) -> Option<i64> {
        match &self.unit {
            Unit::Millimeter => Some((self.value * 1_000_000_000.0) as i64), // 1mm = 1,000,000,000 pm
            Unit::Centimeter => Some((self.value * 10_000_000_000.0) as i64), // 1cm = 10,000,000,000 pm
            Unit::Micrometer => Some((self.value * 1_000_000.0) as i64), // 1µm = 1,000,000 pm (FIXED!)
            Unit::Nanometer => Some((self.value * 1_000.0) as i64),      // 1nm = 1,000 pm
            Unit::Picometer => Some(self.value as i64),                  // 1pm = 1 pm
            _ => None,
        }
    }
}

/// Resolution: user-facing snapping constraint for coordinate evaluation.
///
/// The engine always works in picometers internally. `resolution:` is a
/// snapping constraint that rounds coordinates to the nearest grid step,
/// NOT the internal representation. Maximum addressable range: +/-9,220 km.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolution {
    /// Picometer snap step — coordinates are rounded to multiples of this value.
    pub snap_step_pm: i64,
}

impl Resolution {
    /// Round `value_pm` to the nearest multiple of `snap_step_pm`.
    pub fn snap(&self, value_pm: i64) -> i64 {
        if self.snap_step_pm == 0 {
            return value_pm;
        }
        (value_pm / self.snap_step_pm) * self.snap_step_pm
    }

    /// Create a `Resolution` from any distance `Measurement`.
    /// Returns `None` if the measurement is not a distance unit.
    pub fn from_measurement(m: &Measurement) -> Option<Self> {
        m.to_picometers_i64()
            .map(|pm| Resolution { snap_step_pm: pm })
    }
}

/// Core units for Hardware Script compiler
/// Only includes units needed for geometry and safety calculations.
/// All other units are defined in stdlib/units.hw and stored as Custom(String).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Unit {
    // === CORE COMPILER UNITS (needed for geometry/safety) ===

    // Distance - for placement and routing
    Millimeter,
    Centimeter,
    Micrometer,
    Nanometer,
    Picometer,

    // Voltage - for safety clearances
    Volt,
    Millivolt,
    Kilovolt,

    // Current - for trace width calculations
    Ampere,
    Milliampere,
    Microampere,

    // Temperature - for thermal limits
    Celsius,

    // === EVERYTHING ELSE (defined in stdlib/units.hw) ===
    /// Custom/library units - includes:
    /// - Resistance (Ω, kΩ, MΩ, GΩ)
    /// - Capacitance (F, µF, nF, pF)
    /// - Inductance (H, µH, mH)
    /// - Frequency (Hz, kHz, MHz, GHz)
    /// - Tolerance (%, ppm, ppb)
    /// - Battery (mAh, Ah)
    /// - Power (W, mW, kW)
    /// - Signal (dBm, dBµV)
    /// - Material properties (kg/m³, W/mK, Ω·m, A/mm²)
    /// - Angle (°, rad, mrad)
    /// - Wire gauge (AWG, SWG)
    /// - And any future user-defined units
    Custom(String),
}

impl Unit {
    /// Convert unit to its canonical symbol string for `UnitRegistry` lookup.
    ///
    /// This is the data-driven bridge between the AST `Unit` enum and the
    /// compiler's `UnitRegistry` table. Built-in units map to their base symbol
    /// (e.g. `Unit::Volt` -> `"V"`); library/custom units (`Unit::Custom(s)`)
    /// pass their string through verbatim (e.g. `"Hz"`, `"ns"`, `"GHz"`).
    ///
    /// This lets the exporter resolve any unit via the registry instead of
    /// hardcoding a fixed list of unit strings (per the Bloat Purge).
    pub fn to_symbol(&self) -> Cow<'static, str> {
        use std::borrow::Cow;
        match self {
            // Distance units
            Unit::Millimeter => Cow::Borrowed("mm"),
            Unit::Centimeter => Cow::Borrowed("cm"),
            Unit::Micrometer => Cow::Borrowed("um"),
            Unit::Nanometer => Cow::Borrowed("nm"),
            Unit::Picometer => Cow::Borrowed("pm"),

            // Electrical units
            Unit::Volt => Cow::Borrowed("V"),
            Unit::Millivolt => Cow::Borrowed("mV"),
            Unit::Kilovolt => Cow::Borrowed("kV"),
            Unit::Ampere => Cow::Borrowed("A"),
            Unit::Milliampere => Cow::Borrowed("mA"),
            Unit::Microampere => Cow::Borrowed("uA"),

            // Temperature
            Unit::Celsius => Cow::Borrowed("C"),

            // Custom/library units: pass the symbol through for registry lookup
            Unit::Custom(s) => Cow::Owned(s.to_string()),
        }
    }

    /// Convert unit to SPICE suffix
    ///
    /// SPICE uses: f (femto), p (pico), n (nano), u (micro), m (milli), k (kilo), meg (mega), g (giga)
    /// Returns an error for units that don't have a SPICE representation.
    pub fn to_spice_suffix(&self) -> Result<&'static str, String> {
        match self {
            // Distance units
            Unit::Millimeter => Ok("mm"),
            Unit::Centimeter => Ok("cm"),
            Unit::Micrometer => Ok("u"), // SPICE uses 'u' for micro
            Unit::Nanometer => Ok("n"),
            Unit::Picometer => Ok("p"),

            // Electrical units (no suffix - base unit)
            Unit::Volt => Ok(""),
            Unit::Millivolt => Ok("m"),
            Unit::Kilovolt => Ok("k"),
            Unit::Ampere => Ok(""),
            Unit::Milliampere => Ok("m"),
            Unit::Microampere => Ok("u"),

            // Temperature
            Unit::Celsius => Err("SPICE uses Kelvin for temperature, not Celsius".to_string()),

            // Custom units - parse the string to extract SPICE suffix
            Unit::Custom(s) => {
                // For custom units, the string itself might be the SPICE representation
                // Examples: "ohm", "F", "H", "Hz"
                Ok(Box::leak(s.clone().into_boxed_str()))
            }
        }
    }
}

/// Coordinate: `[X, Y, Z]` (1-indexed, no spaces after commas)
use super::expression::Expression;

/// Bounding box edge for relative positioning (Sprint 3, Task 3.1)
/// v0.2.1: Added CenterX, CenterY, CenterZ for comptime anchor arithmetic
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    Front,
    Back,
    MinZ,
    MaxZ,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    CenterX,
    CenterY,
    CenterZ,
}

/// Unified Merge Waiver (v0.1.7)
///
/// Supports two modes:
/// 1. Boolean: `merge: true` (Waives all overlap/buried errors)
/// 2. List: `merge: [source, drain]` (Waives only specific sub-regions)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MergeWaiver {
    #[default]
    None,
    All,
    Specific(smallvec::SmallVec<[compact_str::CompactString; 2]>),
}

/// Intentional design waivers (v0.1.7)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Waivers {
    /// Unified merge intent (Boolean or List)
    pub merge: MergeWaiver,
    /// Allow component to be disconnected from substrate (P44)
    pub floating: bool,
    /// Allow physical contact without electrical merging
    pub isolated: bool,
    /// Snap component Z to the highest substrate/pour surface at its location (Limitation 5)
    pub snap_to_surface: bool,
    /// Exclude from BOM and Pick-and-Place export
    pub virtual_component: bool,
    /// Prevent automated movement or optimization
    pub locked: bool,
}

/// Anchor reference for relative positioning (Sprint 3, Task 3.1)
/// References another component or pour by name
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorReference {
    pub name: CompactString,
    pub span: Span,
}

/// Relative position specification (Sprint 3, Task 3.1)
/// Syntax: `at AnchorName.edge + offset`
/// Example: `at M1.right + 1mm` or `at M1.top + [0.5mm, 1mm, 0mm]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelativePosition {
    pub anchor: AnchorReference,
    pub edge: Edge,
    pub offset: RelativeOffset,
    pub span: Span,
}

/// Offset for relative positioning
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RelativeOffset {
    /// Single measurement applied to the perpendicular axis
    /// Example: `M1.right + 1mm` means 1mm to the right
    Single(Measurement),
    /// Vector offset [x, y, z] for precise control
    /// Example: `M1.right + [0.5mm, 1mm, 0mm]`
    Vector {
        x: Expression,
        y: Expression,
        z: Expression,
    },
}

/// Supports both positional and declarative syntax:
/// - Positional: `[10, 15, 2]` (XYZ order)
/// - Declarative: `[x:10, y:15, z:2]` (any order)
/// - With expressions: `[x: 20 + (i*2), y:10, z:1]`
/// - Relative (v0.1.6): `M1.right + 1mm` (anchor-based)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Coordinate {
    /// Positional coordinates: [X, Y, Z] order
    Positional {
        x: Expression, // Column (left to right) - first position
        y: Expression, // Row (direction depends on origin) - second position
        z: Expression, // Layer (1=top, 2=inner, 3=bottom) - third position
        span: Span,
    },
    /// Declarative coordinates: [x:10, y:15, z:2] (any order)
    Declarative {
        x: Expression,
        y: Expression,
        z: Expression,
        span: Span,
    },
    /// Relative positioning (v0.1.6 Sprint 3): anchor.edge + offset
    Relative(RelativePosition),
}

impl Coordinate {
    /// Get X coordinate expression (only for absolute coordinates)
    pub fn x(&self) -> &Expression {
        match self {
            Coordinate::Positional { x, .. } => x,
            Coordinate::Declarative { x, .. } => x,
            Coordinate::Relative(_) => {
                panic!("Cannot get X from relative coordinate - must resolve first")
            }
        }
    }

    /// Get Y coordinate expression (only for absolute coordinates)
    pub fn y(&self) -> &Expression {
        match self {
            Coordinate::Positional { y, .. } => y,
            Coordinate::Declarative { y, .. } => y,
            Coordinate::Relative(_) => {
                panic!("Cannot get Y from relative coordinate - must resolve first")
            }
        }
    }

    /// Get Z coordinate expression (only for absolute coordinates)
    pub fn z(&self) -> &Expression {
        match self {
            Coordinate::Positional { z, .. } => z,
            Coordinate::Declarative { z, .. } => z,
            Coordinate::Relative(_) => {
                panic!("Cannot get Z from relative coordinate - must resolve first")
            }
        }
    }

    /// Get span regardless of syntax used
    pub fn span(&self) -> Span {
        match self {
            Coordinate::Positional { span, .. } => *span,
            Coordinate::Declarative { span, .. } => *span,
            Coordinate::Relative(rel) => rel.span,
        }
    }

    /// Check if this coordinate is relative (needs resolution)
    pub fn is_relative(&self) -> bool {
        matches!(self, Coordinate::Relative(_))
    }
}

/// Rotation: arbitrary numeric angle
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rotation {
    pub angle: f64,
    pub span: Span,
}

/// Pin reference in space routes: `Component.Pin` or `Component[0].Pin[1]`
///
/// Examples:
/// - `Component.Pin` - simple reference
/// - `Component[0].Pin` - array component with literal index
/// - `Component.Bus[0]` - array pin with literal index
/// - `Component[0].Bus[1]` - both array component and pin
///
/// Note: Array indices must be resolved to literals before reaching the engine.
/// Pin reference with support for parametric indices (Sprint 3.10)
///
/// Supports both literal indices and expressions with loop variables:
/// - `Adder[0].carry_out` - literal index
/// - `Adder[i].carry_out` - loop variable
/// - `Adder[i+1].carry_in` - expression with loop variable
///
/// The parser handles expression syntax, and the parametric unroller evaluates it during loop unrolling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinReference {
    pub component: CompactString,
    pub component_index: Option<super::Expression>, // For Component[i] or Component[i+1]
    pub pin: CompactString,
    pub pin_index: Option<super::Expression>, // For Pin[i] or Pin[i+1]
    pub span: Span,
}

/// Dimensions: `dimensions: 50mm by 50mm`
///
/// v0.2.1 (Bloat Purge Category 1.3): Z-depth is NOT user-specified. The board
/// height is derived from the sum of `profile.stackup` layer thicknesses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: Measurement,
    pub height: Measurement,
    pub span: Span,
}

/// Grid: `grid: 500 by 500 by 4`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub span: Span,
}

/// Property: key-value pair with measurement or string value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub key: CompactString,
    pub value: PropertyValue,
    pub span: Span,
}

/// Property value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropertyValue {
    Measurement(Measurement),
    String(String),
    Number(f64),
    Boolean(bool),
}

/// Shape definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Rectangle {
        width: Measurement,
        height: Measurement,
        depth: Measurement,
    },
    Cylinder {
        radius: Measurement,
        height: Measurement,
    },
    Sphere {
        radius: Measurement,
    },
}

use super::expression::EvaluationContext;
use compact_str::CompactString;

impl Coordinate {
    /// Evaluate coordinate expressions to concrete values
    ///
    /// ENFORCES SCALE-INVARIANT ARCHITECTURE:
    /// - X and Y MUST be physical measurements (mm, µm, nm, cm)
    /// - Z MUST be an integer layer index (no measurements)
    ///
    /// This ensures Hardware Script code is 100% interoperable regardless of grid resolution.
    ///
    /// NOTE: Relative coordinates must be resolved to absolute coordinates before evaluation.
    pub fn evaluate(
        &self,
        context: &EvaluationContext,
    ) -> Result<
        (
            super::expression::Value,
            super::expression::Value,
            super::expression::Value,
        ),
        String,
    > {
        // Relative coordinates must be resolved first
        if let Coordinate::Relative(_) = self {
            return Err(
                "Relative coordinates must be resolved to absolute coordinates before evaluation.\n\
                 Use ConstraintSolver::resolve_position() to convert relative to absolute coordinates."
                    .to_string(),
            );
        }

        // Only declarative syntax is allowed for scale-invariant coordinates
        match self {
            Coordinate::Positional { .. } => {
                return Err(
                    "Raw grid indices are deprecated to ensure scale-invariant designs.\n\
                     Use named physical measurements for X and Y, and an integer for Z layer.\n\
                     Example: [x: 5.0mm, y: 5.0mm, z: 1]"
                        .to_string(),
                );
            }
            Coordinate::Declarative { .. } => {
                // Continue with evaluation
            }
            Coordinate::Relative(_) => unreachable!("Already checked above"),
        }

        let x_val = self.x().evaluate(context)?;
        let y_val = self.y().evaluate(context)?;
        let z_val = self.z().evaluate(context)?;

        // ENFORCE: X must be a physical measurement OR percentage
        if !x_val.is_physical_or_relative() {
            return Err(
                "X coordinate must be a physical measurement (e.g., 10mm, 500nm) or percentage (e.g., 50%).\n\
                 Raw numbers are not allowed to ensure physics-grounded designs.\n\
                 Example: [x: 10mm, y: 15mm, z: 1] or [x: 50%, y: 50%, z: 1]".into()
            );
        }

        // ENFORCE: Y must be a physical measurement OR percentage
        if !y_val.is_physical_or_relative() {
            return Err(
                "Y coordinate must be a physical measurement (e.g., 10mm, 500nm) or percentage (e.g., 50%).\n\
                 Raw numbers are not allowed to ensure physics-grounded designs.\n\
                 Example: [x: 10mm, y: 15mm, z: 1] or [x: 50%, y: 50%, z: 1]".into()
            );
        }

        // ENFORCE: Z must be an integer (layer index), not a measurement or percentage
        match &z_val {
            super::expression::Value::Number(n) if *n < 0 => {
                return Err(format!(
                    "Z coordinate (layer index) cannot be negative: {}",
                    n
                ));
            }
            super::expression::Value::Measurement { .. } => {
                return Err(
                    "Z coordinate must be a layer index (integer), not a physical measurement.\n\
                     The Z-axis represents logical layers whose physical thickness is defined in profile.hw.\n\
                     This makes designs immune to manufacturing stackup changes.\n\
                     Example: [x: 10mm, y: 15mm, z: 1]".into()
                );
            }
            super::expression::Value::Percentage(_) => {
                return Err(
                    "Z coordinate must be a layer index (integer), not a percentage.\n\
                     The Z-axis represents logical layers (1=Top, 2=Inner1, etc.).\n\
                     Example: [x: 10mm, y: 15mm, z: 1] or [x: 50%, y: 50%, z: 1]"
                        .to_string(),
                );
            }
            _ => {}
        }

        Ok((x_val, y_val, z_val))
    }

    /// Evaluate coordinate with empty context (no variables)
    pub fn evaluate_const(
        &self,
    ) -> Result<
        (
            super::expression::Value,
            super::expression::Value,
            super::expression::Value,
        ),
        String,
    > {
        self.evaluate(&rustc_hash::FxHashMap::default())
    }

    /// Evaluate coordinate expressions to picometer values (i64).
    ///
    /// X and Y are converted to picometers using `to_picometers()`.
    /// Z remains as an integer layer index (not affected by pm change).
    pub fn evaluate_picometers(
        &self,
        context: &EvaluationContext,
    ) -> Result<(i64, i64, i32), String> {
        let (x_val, y_val, z_val) = self.evaluate(context)?;

        let x_pm = x_val.to_picometers()?;
        let y_pm = y_val.to_picometers()?;
        let z_idx = z_val.as_integer()? as i32;

        Ok((x_pm, y_pm, z_idx))
    }

    /// Evaluate coordinate expressions to picometer values with resolution snapping.
    ///
    /// X and Y are converted to picometers, then snapped to the resolution's
    /// `snap_step_pm` grid. Z remains as an integer layer index.
    pub fn evaluate_with_resolution(
        &self,
        context: &EvaluationContext,
        resolution: &Resolution,
    ) -> Result<(i64, i64, i32), String> {
        let (x_pm, y_pm, z_idx) = self.evaluate_picometers(context)?;

        Ok((resolution.snap(x_pm), resolution.snap(y_pm), z_idx))
    }
}
