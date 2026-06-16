//! Error types for IR integration.

use compact_str::CompactString;
use miette::Diagnostic;
use thiserror::Error;

/// Errors that can occur during IR transformation.
#[derive(Error, Diagnostic, Debug)]
pub enum IrError {
    #[error("No space definition found in program")]
    #[diagnostic(
        code(C31),
        url("https://docs.hw-script.org/errors/C31"),
        help("Hardware Script files must contain a 'space Name:' block")
    )]
    NoSpaceDefinition,

    #[error("Space dimensions not specified")]
    #[diagnostic(
        code(C32),
        url("https://docs.hw-script.org/errors/C32"),
        help("Add 'dimensions: <width> by <height> by <depth>' to the space definition")
    )]
    MissingDimensions,

    #[error("Space grid not specified")]
    #[diagnostic(
        code(C33),
        url("https://docs.hw-script.org/errors/C33"),
        help("Add 'grid: <x> by <y> by <z>' to the space definition")
    )]
    MissingGrid,

    #[error("Invalid coordinate: [{0}, {1}, {2}] exceeds grid bounds")]
    #[diagnostic(
        code(C21),
        url("https://docs.hw-script.org/errors/C21"),
        help("Coordinates must be within the grid dimensions specified in the space definition")
    )]
    InvalidCoordinate(usize, usize, usize),

    #[error("Component placement failed: {0}")]
    #[diagnostic(
        code(R15),
        url("https://docs.hw-script.org/errors/R15"),
        help("Check component position, rotation, and ensure no collisions with other components")
    )]
    PlacementError(String),

    #[error("Routing failed: {0}")]
    #[diagnostic(
        code(R16),
        url("https://docs.hw-script.org/errors/R16"),
        help("Verify waypoints are valid and path is not blocked by components")
    )]
    RoutingError(String),

    #[error("Pin reference not found: {component}.{pin}")]
    #[diagnostic(
        code(C12),
        url("https://docs.hw-script.org/errors/C12"),
        help("Verify component name and pin name are correct")
    )]
    PinNotFound {
        component: CompactString,
        pin: String,
    },

    #[error("Disconnected net: {}", .0.route_name)]
    #[diagnostic(
        code(R12),
        url("https://docs.hw-script.org/errors/R12"),
        help("The {} waypoint at {}mm is {} away from pin {} at {}mm. Manual waypoints must start and end at the exact pin positions.", .0.waypoint_type, .0.waypoint_pos, .0.distance, .0.pin_name, .0.pin_pos)
    )]
    DisconnectedNet(Box<DisconnectedNetDetails>),

    #[error("Compilation error: {0}")]
    #[diagnostic(
        code(C99),
        url("https://docs.hw-script.org/errors/C99"),
        help("Check the error message for details")
    )]
    CompilationError(String),

    #[error("Missing profile constraint: {field}")]
    #[diagnostic(
        code(C34),
        url("https://docs.hw-script.org/errors/C34"),
        help("The profile must specify '{field}'. Add it to the profile definition.")
    )]
    MissingProfileConstraint { field: String },

    #[error("Material interpenetration detected at z = {z_nm} nm")]
    #[diagnostic(
        code(P43),
        url("https://docs.hw-script.org/errors/P43"),
        help("Pour '{pour_a}' (material: {mat_a}) overlaps with pour '{pour_b}' (material: {mat_b}). Different materials cannot occupy the same physical space. Adjust boundaries so pours touch at edges but do not overlap.")
    )]
    MaterialInterpenetration {
        pour_a: CompactString,
        mat_a: CompactString,
        pour_b: CompactString,
        mat_b: CompactString,
        z_nm: i64,
    },

    #[error("Invalid expression in loop: {0}")]
    #[diagnostic(
        code(C34),
        url("https://docs.hw-script.org/errors/C34"),
        help("Check the expression syntax and ensure all variables are defined")
    )]
    InvalidExpression(String),

    #[error("Component '{component}' overlaps with substrate material", component = .0.component)]
    #[diagnostic(
        code(P44),
        url("https://docs.hw-script.org/errors/P44"),
        help("Place component at z:{suggested_z_layer} or higher (above substrate surface). Computed position: ({x_mm:.3}mm, {y_mm:.3}mm, {z_mm:.3}mm)",
            suggested_z_layer = .0.suggested_z_layer,
            x_mm = .0.x_mm,
            y_mm = .0.y_mm,
            z_mm = .0.z_mm)
    )]
    #[label("component overlaps substrate here", .0.span)]
    SubstrateOverlap(Box<SubstrateOverlapDetails>),

    /// Sprint 5.5: Component floating above substrate
    #[error("Component '{component}' is floating in air above substrate", component = .0.component)]
    #[diagnostic(
        code(P44),
        url("https://docs.hw-script.org/errors/P44"),
        help("Place component at z:{substrate_max_layer} (substrate surface). Computed position: ({x_mm:.3}mm, {y_mm:.3}mm, {z_mm:.3}mm)",
            substrate_max_layer = .0.substrate_max_layer,
            x_mm = .0.x_mm,
            y_mm = .0.y_mm,
            z_mm = .0.z_mm)
    )]
    #[label("component floats {gap_mm:.3}mm above substrate here", .0.span)]
    ComponentFloatingInAir(Box<ComponentFloatingInAirDetails>),

    /// Sprint 5.5: Component buried below substrate
    #[error("Component '{component}' is buried below substrate", component = .0.component)]
    #[diagnostic(
        code(P44),
        url("https://docs.hw-script.org/errors/P44"),
        help("Place component at z:{substrate_max_layer} or higher (on or above substrate surface). Computed position: ({x_mm:.3}mm, {y_mm:.3}mm, {z_mm:.3}mm)",
            substrate_max_layer = .0.substrate_max_layer,
            x_mm = .0.x_mm,
            y_mm = .0.y_mm,
            z_mm = .0.z_mm)
    )]
    #[label("component buried {gap_mm:.3}mm below substrate here", .0.span)]
    ComponentBuriedInSubstrate(Box<ComponentBuriedInSubstrateDetails>),

    #[error("Invalid Z elevation: {value} nm")]
    #[diagnostic(
        code(C25),
        url("https://docs.hw-script.org/errors/C25"),
        help("Z elevations cannot be negative. The evaluated height is below the origin.\n\nEvaluated: {value} nm\n\nUse z: 0mm or higher, or adjust expressions so the result is non-negative.")
    )]
    NegativeLayerIndex {
        value: i64,
        #[label("negative layer index evaluated here")]
        span: miette::SourceSpan,
    },

    #[error("Symbol error: {0}")]
    SymbolError(#[from] crate::SymbolError),

    #[error("Geometric collision in array '{array_name}': instances {instance_a} and {instance_b} have overlapping geometry", array_name = .0.array_name, instance_a = .0.instance_a, instance_b = .0.instance_b)]
    #[diagnostic(
        code(P12),
        url("https://docs.hw-script.org/errors/P12"),
        help("Array instances have overlapping geometry without explicit merge intent.\n\nPhysical Reality: Two pieces of material cannot occupy the same physical space.\n\nProblem: Pour '{}' in instances {} and {} overlap at:\n  Instance {}: [{:.3}mm, {:.3}mm] to [{:.3}mm, {:.3}mm]\n  Instance {}: [{:.3}mm, {:.3}mm] to [{:.3}mm, {:.3}mm]\n\nSolutions:\n1. Increase pitch to prevent overlap: pitch: {:.3}mm (currently {:.3}mm)\n2. Add explicit merge intent if overlap is intentional:\n   merge: [{}]  # Declares: \"I know these overlap. Melt them.\"\n\nPhilosophy: Hardware Script has NO IMPLICIT MAGIC. Overlapping geometry must be explicitly declared.",
            .0.pour_name, .0.instance_a, .0.instance_b,
            .0.instance_a, .0.bbox_a_min_x, .0.bbox_a_min_y, .0.bbox_a_max_x, .0.bbox_a_max_y,
            .0.instance_b, .0.bbox_b_min_x, .0.bbox_b_min_y, .0.bbox_b_max_x, .0.bbox_b_max_y,
            .0.suggested_pitch, .0.current_pitch,
            .0.terminal_name)
    )]
    GeometricCollision(Box<GeometricCollisionDetails>),

    #[error("Circular spatial dependency detected: {path}")]
    #[diagnostic(
        code(C35),
        url("https://docs.hw-script.org/errors/C35"),
        help(
            "Component positioning forms a cycle (e.g., A depends on B, and B depends on A). \
              Hardware Script requires a directed acyclic graph (DAG) for spatial placement."
        )
    )]
    CircularReference { path: String },
}

#[derive(Debug, Clone)]
pub struct SubstrateOverlapDetails {
    pub component: CompactString,
    pub component_z_layer: usize,
    pub component_z_mm: f64,
    pub substrate_min_layer: usize,
    pub substrate_max_layer: usize,
    pub substrate_min_mm: f64,
    pub substrate_max_mm: f64,
    pub suggested_z_layer: usize,
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: f64,
    pub span: miette::SourceSpan,
    pub suggestion: String,
}

#[derive(Debug, Clone)]
pub struct ComponentFloatingInAirDetails {
    pub component: CompactString,
    pub component_z_layer: usize,
    pub component_z_mm: f64,
    pub substrate_max_layer: usize,
    pub substrate_max_mm: f64,
    pub gap_mm: f64,
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: f64,
    pub span: miette::SourceSpan,
    pub suggestion: String,
}

#[derive(Debug, Clone)]
pub struct ComponentBuriedInSubstrateDetails {
    pub component: CompactString,
    pub component_z_layer: usize,
    pub component_z_mm: f64,
    pub substrate_min_layer: usize,
    pub substrate_min_mm: f64,
    pub substrate_max_layer: usize,
    pub substrate_max_mm: f64,
    pub gap_mm: f64,
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: f64,
    pub span: miette::SourceSpan,
    pub suggestion: String,
}

/// Details for geometric collision errors (boxed to reduce enum size)
#[derive(Debug, Clone)]
pub struct GeometricCollisionDetails {
    pub array_name: CompactString,
    pub instance_a: usize,
    pub instance_b: usize,
    pub pour_name: CompactString,
    pub terminal_name: CompactString,
    pub bbox_a_min_x: f64,
    pub bbox_a_min_y: f64,
    pub bbox_a_max_x: f64,
    pub bbox_a_max_y: f64,
    pub bbox_b_min_x: f64,
    pub bbox_b_min_y: f64,
    pub bbox_b_max_x: f64,
    pub bbox_b_max_y: f64,
    pub current_pitch: f64,
    pub suggested_pitch: f64,
}

#[derive(Debug, Clone)]
pub struct DisconnectedNetDetails {
    pub route_name: CompactString,
    pub waypoint_type: CompactString,
    pub waypoint_pos: CompactString,
    pub pin_name: CompactString,
    pub pin_pos: CompactString,
    pub distance: CompactString,
}
