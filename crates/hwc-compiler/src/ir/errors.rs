//! Error types for IR integration.

use compact_str::CompactString;
use miette::Diagnostic;
use thiserror::Error;

/// Errors that can occur during IR transformation.
#[derive(Error, Diagnostic, Debug)]
pub enum IrError {
    #[error("No space definition found in program")]
    #[diagnostic(
        code(C28),
        url("https://docs.hw-script.org/errors/C28"),
        help("Hardware Script files must contain a 'space Name:' block")
    )]
    NoSpaceDefinition,

    #[error("Space dimensions not specified")]
    #[diagnostic(
        code(C31),
        url("https://docs.hw-script.org/errors/C31"),
        help("Add 'dimensions: <width> by <height>' or similar to the space definition")
    )]
    MissingDimensions {
        #[label("dimensions must be specified for this space")]
        span: miette::SourceSpan,
    },

    #[error("Space grid not specified")]
    #[diagnostic(
        code(C33),
        url("https://docs.hw-script.org/errors/C33"),
        help("Add 'grid: <x> by <y> by <z>' to the space definition")
    )]
    MissingGrid,

    #[error("Invalid coordinate: [{0}, {1}, {2}] exceeds grid bounds")]
    #[diagnostic(
        code(C27),
        url("https://docs.hw-script.org/errors/C27"),
        help("Coordinates must be within the grid dimensions specified in the space definition")
    )]
    InvalidCoordinate(usize, usize, usize),

    #[error("Bridge material transition invalid: {from_material} -> {to_material}: {reason}")]
    #[diagnostic(
        code(R15),
        url("https://docs.hw-script.org/errors/R15"),
        help("Check bridge material compatibility")
    )]
    BridgeValidationFailed {
        from_material: CompactString,
        to_material: CompactString,
        reason: String,
    },

    #[error("Failed to resolve coordinate expression '{coordinate_str}': {reason}")]
    #[diagnostic(
        code(R17),
        url("https://docs.hw-script.org/errors/R17"),
        help("Verify coordinate syntax and that all variables are defined")
    )]
    CoordinateResolutionFailed {
        coordinate_str: String,
        reason: String,
    },

    #[error("Failed to resolve layer '{layer_name}': {reason}")]
    #[diagnostic(
        code(R18),
        url("https://docs.hw-script.org/errors/R18"),
        help("Verify layer name exists in the profile stackup")
    )]
    StackupResolutionFailed {
        layer_name: CompactString,
        reason: String,
    },

    #[error("Placement constraint violation: {message}")]
    #[diagnostic(
        code(R19),
        url("https://docs.hw-script.org/errors/R19"),
        help("Check placement constraints for this component")
    )]
    PlacementConstraint { message: String, component: String },

    #[error("Expression evaluation failed: {message}")]
    #[diagnostic(
        code(R25),
        url("https://docs.hw-script.org/errors/R25"),
        help("Check expression syntax and ensure all variables are defined")
    )]
    ExpressionEvaluation { message: String },

    #[error("No route path found from {from_pin} to {to_pin}")]
    #[diagnostic(
        code(R16),
        url("https://docs.hw-script.org/errors/R16"),
        help("No valid path exists for this net. With the legalization-only workflow, this is a terminal error — there is no rip-up or retry mechanism.\n\nCheck that components are within routing reach, or reduce congestion.")
    )]
    NoPathFound {
        net: CompactString,
        from_pin: CompactString,
        to_pin: CompactString,
    },

    #[error("Route for net '{net}' has no waypoints")]
    #[diagnostic(
        code(R20),
        url("https://docs.hw-script.org/errors/R20"),
        help("Add waypoints or use auto-routing")
    )]
    EmptyRoute { net: CompactString },

    #[error("Invalid route expression '{expression}': {reason}")]
    #[diagnostic(
        code(R21),
        url("https://docs.hw-script.org/errors/R21"),
        help("Check route expression syntax")
    )]
    InvalidRouteExpression { expression: String, reason: String },

    #[error("Manual route missing required field: {missing_field}")]
    #[diagnostic(
        code(R22),
        url("https://docs.hw-script.org/errors/R22"),
        help("Add the missing field to the route definition")
    )]
    ManualRouteIncomplete { missing_field: String },

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

    #[error("Profile '{name}' not found")]
    #[diagnostic(
        code(C40),
        url("https://docs.hw-script.org/errors/C40"),
        help("Import or declare profile '{name}'.")
    )]
    ProfileNotFound { name: CompactString },

    #[error("Logic synthesis failed: {message}")]
    #[diagnostic(
        code(C41),
        url("https://docs.hw-script.org/errors/C41"),
        help("Check the logic definition for errors.")
    )]
    LogicSynthesisFailed { message: String },

    #[error("Compilation aborted: {error_count} previous error{}", if *error_count == 1 { "" } else { "s" })]
    #[diagnostic(
        code(C42),
        url("https://docs.hw-script.org/errors/C42"),
        help("Fix the preceding errors and try again.")
    )]
    CompilationAborted { error_count: usize },

    /// P42: Static geometry guard detected coplanar short circuits before routing.
    #[error(
        "Static short circuit: net '{net_a}' overlaps net '{net_b}' at ({x_nm},{y_nm},{z_nm}) nm"
    )]
    #[diagnostic(
        code(P42),
        url("https://docs.hw-script.org/errors/P42"),
        help(
            "Coplanar conductors on different nets overlap in the XY and Z planes. \
              Separate the overlapping geometry or verify that these nets should be connected. \
              Detected by the static geometry guard before routing to fail fast."
        )
    )]
    StaticGeometryShort {
        net_a: CompactString,
        net_b: CompactString,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
    },

    #[error("Missing profile constraint: {field}")]
    #[diagnostic(
        code(C34),
        url("https://docs.hw-script.org/errors/C34"),
        help("The profile must specify '{field}'. Add it to the profile definition.")
    )]
    MissingProfileConstraint { field: String },

    #[error("Undeclared material: '{material}'")]
    #[diagnostic(
        code(M01),
        url("https://docs.hw-script.org/errors/M01"),
        help("Material '{material}' is used but never declared or imported. Add a 'material {material}: category: conductor|semiconductor|insulator' declaration, or import it from a standard library: 'import * from @std/materials/conductors'")
    )]
    UndeclaredMaterial { material: CompactString },

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

    #[error("Device terminal pour '{pour_name}' missing explicit net assignment")]
    #[diagnostic(
        code(D01),
        url("https://docs.hw-script.org/errors/D01"),
        help("HardwareScript Philosophy: Zero Compiler Magic\n\nDevice terminal pours MUST have explicit 'net:' assignments. The compiler does NOT infer connectivity from physical layout.\n\nFix: Add 'net: <NetName>' to the pour definition:\n\n  add pour({material}) named {pour_name}:\n      device: {device}.{terminal}\n      net: YourNetName  ← ADD THIS LINE\n      dimensions: ...")
    )]
    DeviceTerminalMissingNet {
        pour_name: CompactString,
        device: CompactString,
        terminal: CompactString,
        material: CompactString,
    },

    #[error("Invalid expression in loop: {0}")]
    #[diagnostic(
        code(C43),
        url("https://docs.hw-script.org/errors/C43"),
        help("Check the expression syntax and ensure all variables are defined")
    )]
    InvalidExpression(String),

    #[error("Dimensional unit mismatch in expression: {expression}")]
    #[diagnostic(
        code(C44),
        url("https://docs.hw-script.org/errors/C44"),
        help("Cannot {operation} incompatible units. {detail}\n\nHardware Script enforces dimensional type safety to prevent physically nonsensical operations.")
    )]
    DimensionalUnitMismatch {
        expression: String,
        operation: String,
        detail: String,
    },

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

    #[error("Failed to resolve route endpoint '{endpoint}'")]
    #[diagnostic(code(R22), url("https://docs.hw-script.org/errors/R22"))]
    UnresolvedEndpoint {
        endpoint: String,
        #[label("this endpoint could not be resolved")]
        span: miette::SourceSpan,
        #[help]
        help_message: String,
    },

    #[error("ASIC compile failed: {message}")]
    #[diagnostic(
        code(C36),
        url("https://docs.hw-script.org/errors/C36"),
        help("Under ASIC technology, all physical constraints must be explicitly declared. No implicit defaults are permitted.\n\n{hint}")
    )]
    MissingAsicConstraint { message: String, hint: String },

    #[error("ASIC compile failed: {message}")]
    #[diagnostic(
        code(C36),
        url("https://docs.hw-script.org/errors/C36"),
        help("Under ASIC technology, all physical constraints must be explicitly declared. No implicit defaults are permitted.\n\n{hint}")
    )]
    MissingAsicConstraintWithSpan {
        message: String,
        hint: String,
        #[label("missing constraint here")]
        span: miette::SourceSpan,
    },

    #[error("Material '{material}' is missing required property '{property}'")]
    #[diagnostic(
        code(C37),
        url("https://docs.hw-script.org/errors/C37"),
        help("Declare the required property '{property}' in the material definition for '{material}'")
    )]
    MissingPhysicalProperty {
        material: CompactString,
        property: String,
    },

    #[error("Placement error: {0}")]
    PlacementError(String),

    #[error("Routing error: {0}")]
    RoutingError(String),

    #[error("Active net '{net}' is missing electrical specifications")]
    #[diagnostic(
        code(C38),
        url("https://docs.hw-script.org/errors/C38"),
        help("Add explicit voltage, current, or classification properties to the net '{net}': {detail}")
    )]
    MissingElectricalSpecification { net: CompactString, detail: String },

    // =====================================================================
    // v0.1.8 Physical Synthesis Guardrails
    // =====================================================================
    /// R25: Pathfinder attempted to place a trace on a layer with `routable: false`.
    #[error("Trace placed on non-routable layer '{layer}' (material: {material})")]
    #[diagnostic(
        code(R25),
        url("https://docs.hw-script.org/errors/R25"),
        help(
            "Layer '{layer}' is declared as routable: false in the profile stackup. \
              Routing is only permitted on layers with routable: true. \
              Route the trace on a different layer, or change the layer's routable attribute."
        )
    )]
    NonRoutableLayer {
        layer: CompactString,
        material: CompactString,
    },

    /// R25a: Pathfinder exceeded the max route length on a `local_only` layer.
    #[error("Local-only route on layer '{layer}' exceeded maximum length ({actual_nm} nm > {max_nm} nm)")]
    #[diagnostic(
        code(R25),
        url("https://docs.hw-script.org/errors/R25"),
        help(
            "Layer '{layer}' is declared as routable: local_only. Routes on this layer \
              must not exceed {max_nm} nm. Use a routable: true metal layer for longer routes, \
              or increase routing.max_local_route_length in the profile."
        )
    )]
    LocalRouteExceeded {
        layer: CompactString,
        actual_nm: i64,
        max_nm: i64,
    },

    /// R30: Post-route validation found a trace midpoint inside a component's bounding box.
    #[error("Route penetrates interior of component '{component}' at ({x_nm}, {y_nm}, {z_nm})")]
    #[diagnostic(
        code(R30),
        url("https://docs.hw-script.org/errors/R30"),
        help(
            "A routed trace segment passes through the physical body of component '{component}'. \
              Traces must terminate at boundary ports (exit:/enter: cardinal directions) \
              and must not penetrate the component's bounding box. \
              Adjust the route's exit/enter directions to dock at the component boundary."
        )
    )]
    RoutePenetratesComponent {
        component: CompactString,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
    },

    /// P45: Coplanar conductor-semiconductor contact without a declared bridge.
    #[error(
        "Forbidden junction: {mat_a} touching {mat_b} at ({x_nm}, {y_nm}, {z_nm}) without a bridge"
    )]
    #[diagnostic(
        code(P45),
        url("https://docs.hw-script.org/errors/P45"),
        help(
            "Material '{mat_a}' (category: {cat_a}) is in direct coplanar contact with \
              '{mat_b}' (category: {cat_b}) without an intermediate ohmic contact bridge. \
              Declare a bridge rule in the profile: \
              'bridge {mat_a} to {mat_b}: <BridgeMaterial>' \
              where <BridgeMaterial> is a material with category: ohmic_contact."
        )
    )]
    ForbiddenJunction {
        mat_a: CompactString,
        cat_a: CompactString,
        mat_b: CompactString,
        cat_b: CompactString,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
    },

    /// v0.1.8: Missing routing heuristic weights from PDK profile.
    #[error("Missing routing heuristic '{field}' in profile")]
    #[diagnostic(
        code(R25),
        url("https://docs.hw-script.org/errors/R25"),
        help(
            "The PDK profile must declare all routing heuristic weights in the 'routing:' block. \
              The compiler is a deterministic engine — no hardcoded fallbacks. \
              {hint}"
        )
    )]
    MissingRoutingHeuristics { field: CompactString, hint: String },

    /// v0.1.9: Clearance violation during placement (early DRC)
    #[error("Clearance violation during placement of '{entity_name}'")]
    #[diagnostic(
        code(P46),
        url("https://docs.hw-script.org/errors/P46"),
        help("{reason}")
    )]
    ClearanceViolation {
        entity_type: CompactString,
        entity_name: CompactString,
        reason: CompactString,
    },

    /// v0.1.9: Undeclared shape reference
    #[error("Undeclared shape: '{shape}'")]
    #[diagnostic(
        code(S15),
        url("https://docs.hw-script.org/errors/S15"),
        help("Shape '{shape}' is used but never declared. Add a 'shape {shape}(...)' definition.")
    )]
    UndeclaredShape { shape: CompactString },

    /// v0.1.9: Failed to resolve shape geometry
    #[error("Failed to resolve shape '{shape}': {reason}")]
    #[diagnostic(
        code(S16),
        url("https://docs.hw-script.org/errors/S16"),
        help("The shape could not be instantiated or evaluated")
    )]
    ShapeResolutionFailed {
        shape: CompactString,
        reason: String,
    },

    // =====================================================================
    // v0.1.9 Salsa Constraint Solver Errors
    // =====================================================================
    /// R31: Corridor extraction failed - no valid path through C-space.
    #[error("No corridor found from ({start_x}, {start_y}, {start_z}) to ({end_x}, {end_y}, {end_z}) in G-cell {gcell_id}")]
    #[diagnostic(
        code(R31),
        url("https://docs.hw-script.org/errors/R31"),
        help(
            "The spatial decomposer could not extract a navigable corridor between these points. \
              Possible causes: \
              1. All corridors are narrower than the required width (trace_width + 2 * clearance). \
              2. Obstacles completely block the route. \
              3. Start or end point is inside an inflated obstacle.\n\n\
              Suggestions: \
              - Reduce trace_width_nm or min_clearance_nm for this net. \
              - Check that start/end ports are in free space. \
              - Verify board_bounds encompass the routing area."
        )
    )]
    CorridorExtractionFailed {
        gcell_id: u32,
        start_x: i64,
        start_y: i64,
        start_z: i64,
        end_x: i64,
        end_y: i64,
        end_z: i64,
    },

    /// R32: Corridor too narrow for trace + clearance.
    #[error("Corridor in G-cell {gcell_id} is too narrow ({actual_nm} nm) for trace + clearance ({required_nm} nm)")]
    #[diagnostic(
        code(R32),
        url("https://docs.hw-script.org/errors/R32"),
        help(
            "The extracted corridor's bottleneck width ({actual_nm} nm) is less than the required \
              width ({required_nm} nm = trace_width + 2 * clearance).\n\n\
              The optimizer attempted to expand to adjacent G-cells but all alternatives were also insufficient. \
              Reduce trace width or clearance, or restructure the layout to create wider channels."
        )
    )]
    CorridorTooNarrow {
        gcell_id: u32,
        actual_nm: i64,
        required_nm: i64,
    },

    /// R33: Optimization loop exhausted without convergence.
    #[error("Optimization for net {net_id} in G-cell {gcell_id} stalled after {iterations} iterations ({violations} unresolved violations)")]
    #[diagnostic(
        code(R33),
        url("https://docs.hw-script.org/errors/R33"),
        help(
            "The Measure → Optimize → Measure loop did not converge within the allowed iterations. \
              The route has {violations} unresolved constraint violations.\n\n\
              Possible causes: \
              - Soft constraints are conflicting (e.g., length target vs. obstacle avoidance). \
              - Hard constraints are unsatisfiable in this G-cell.\n\n\
              Increase max_iterations in the optimization config, or relax soft constraints."
        )
    )]
    OptimizationStalled {
        net_id: u32,
        gcell_id: u32,
        iterations: usize,
        violations: usize,
    },

    /// R34: Repair attempts exhausted for a net/G-cell.
    #[error("Routing failed for net {net_id} in G-cell {gcell_id} after {attempts} repair attempts ({violations} unresolved violations)")]
    #[diagnostic(
        code(R34),
        url("https://docs.hw-script.org/errors/R34"),
        help(
            "All repair attempts have been exhausted. The router tried {attempts} alternative strategies \
              but could not resolve {violations} constraint violations.\n\n\
              Possible causes: \
              - The G-cell is congested or has geometric impossibilities. \
              - G-cell has repeated failures across multiple nets (check repair history).\n\n\
              Increase max_repair_attempts, restructure the layout, or declare this net as unroutable."
        )
    )]
    RepairExhausted {
        net_id: u32,
        gcell_id: u32,
        attempts: usize,
        violations: usize,
    },

    /// R35: Spatial decomposer received invalid parameters.
    #[error("Invalid spatial decomposer parameters: {reason}")]
    #[diagnostic(
        code(R35),
        url("https://docs.hw-script.org/errors/R35"),
        help(
            "The spatial decomposer requires valid physical parameters.\n\n\
              {reason}"
        )
    )]
    InvalidDecomposerParams { reason: String },

    /// R36: Navigable space extraction failed.
    #[error("Navigable space extraction failed for G-cell {gcell_id}: {reason}")]
    #[diagnostic(
        code(R36),
        url("https://docs.hw-script.org/errors/R36"),
        help(
            "Could not decompose free space into navigable cells.\n\n\
              {reason}"
        )
    )]
    NavigableSpaceFailed { gcell_id: u32, reason: String },

    /// Unit conversion error during IR building.
    #[error("Unit conversion error: {message}")]
    #[diagnostic(
        code(C45),
        url("https://docs.hw-script.org/errors/C45"),
        help("Check that the unit matches the expected dimension for this field.")
    )]
    UnitConversion {
        message: String,
        #[label("invalid unit here")]
        span: Option<miette::SourceSpan>,
    },

    /// R37: Constraint validation found hard violations that cannot be resolved.
    #[error("Hard constraint violation for net {net_id}: {description}")]
    #[diagnostic(
        code(R37),
        url("https://docs.hw-script.org/errors/R37"),
        help(
            "A hard constraint was violated and cannot be resolved by the optimizer.\n\n\
              {description}"
        )
    )]
    HardConstraintViolation { net_id: u32, description: String },

    /// CIR1: Interface capability constraint violated during routing.
    #[error("Interface capability constraint violated: trace width {actual_nm}nm < required {required_nm}nm")]
    #[diagnostic(
        code(CIR1),
        url("https://docs.hw-script.org/errors/CIR1"),
        help("Increase the trace width or reduce the current requirement on this interface")
    )]
    InterfaceConstraintViolation {
        actual_nm: i64,
        required_nm: i64,
        #[label("interface capability requires wider trace")]
        span: miette::SourceSpan,
    },

    // =====================================================================
    // v0.2.0 Database-Driven Architecture Errors
    // =====================================================================
    /// No connection point found for entity on a routing layer.
    #[error("No connection point for entity '{entity}' on routing layer '{layer}'")]
    #[diagnostic(
        code(R40),
        url("https://docs.hw-script.org/errors/R40"),
        help(
            "Entity '{entity}' does not have a registered connection on layer '{layer}'. \
              Check that a via or contact connects this entity to the target routing layer.\n\n\
              {hint}"
        )
    )]
    NoConnectionPoint {
        entity: CompactString,
        layer: CompactString,
        hint: String,
    },

    /// Routing layer Z doesn't match connection Z (compiler bug indicator).
    #[error("Via connection Z mismatch: entity '{entity}' connection at {connection_z}nm but routing layer '{layer}' expects {expected_z}nm")]
    #[diagnostic(
        code(R41),
        url("https://docs.hw-script.org/errors/R41"),
        help(
            "The via connection Z coordinate ({connection_z}nm) doesn't match the routing layer Z ({expected_z}nm). \
              This indicates a compiler bug in via registration."
        )
    )]
    ConnectionZMismatch {
        entity: CompactString,
        connection_z: i64,
        expected_z: i64,
        layer: CompactString,
    },

    /// Pre-routing validation failed.
    #[error("Pre-routing validation failed for route '{route}' on layer '{layer}'")]
    #[diagnostic(
        code(R42),
        url("https://docs.hw-script.org/errors/R42"),
        help("{problem}\n\n{hint}")
    )]
    PreRoutingValidationFailed {
        route: CompactString,
        layer: CompactString,
        problem: String,
        hint: String,
    },

    /// Post-routing validation failed.
    #[error("Post-routing validation failed for net '{net}'")]
    #[diagnostic(
        code(R43),
        url("https://docs.hw-script.org/errors/R43"),
        help("{problem}\n\n{hint}")
    )]
    PostRoutingValidationFailed {
        net: CompactString,
        problem: String,
        hint: String,
    },

    /// Invalid routing layer — not found in the routing layer database.
    #[error("Invalid routing layer: '{layer}'")]
    #[diagnostic(
        code(R44),
        url("https://docs.hw-script.org/errors/R44"),
        help(
            "Layer '{layer}' is not a valid routing layer.\n\n\
              Available routing layers: {available_layers}"
        )
    )]
    InvalidRoutingLayer {
        layer: CompactString,
        available_layers: CompactString,
    },

    /// Missing route parameter — layer must be specified.
    #[error("Route missing required parameter: '{parameter}'")]
    #[diagnostic(
        code(R45),
        url("https://docs.hw-script.org/errors/R45"),
        help(
            "Every route must explicitly declare which layer to use.\n\n\
              {hint}"
        )
    )]
    MissingRouteParameter {
        parameter: CompactString,
        route: CompactString,
        hint: String,
    },

    /// Via connection not found for material pair.
    #[error("No via connection defined from '{from_material}' to '{to_material}'")]
    #[diagnostic(
        code(R46),
        url("https://docs.hw-script.org/errors/R46"),
        help(
            "No bridge rule connects material '{from_material}' to '{to_material}'.\n\n\
              {hint}"
        )
    )]
    ViaConnectionNotFound {
        from_material: CompactString,
        to_material: CompactString,
        hint: String,
    },

    /// Device definition lookup failed during device registration.
    #[error("Device registration failed: {message}")]
    #[diagnostic(
        code(D02),
        url("https://docs.hw-script.org/errors/D02"),
        help("Device bindings in the layout do not match any device definition in the symbol table.")
    )]
    DeviceRegistryError { message: String },

    /// Layer connection database error.
    #[error("Layer connection database error: {message}")]
    #[diagnostic(
        code(R47),
        url("https://docs.hw-script.org/errors/R47"),
        help("{hint}")
    )]
    LayerConnectionError { message: String, hint: String },

    /// Routing layer database error.
    #[error("Routing layer database error: {message}")]
    #[diagnostic(
        code(R48),
        url("https://docs.hw-script.org/errors/R48"),
        help("{hint}")
    )]
    RoutingLayerError { message: String, hint: String },
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

// Conversion from ConversionError to IrError
impl From<crate::conversions::ConversionError> for IrError {
    fn from(err: crate::conversions::ConversionError) -> Self {
        match err {
            crate::conversions::ConversionError::MissingProfileConstraint(field) => {
                IrError::MissingProfileConstraint { field }
            }
            crate::conversions::ConversionError::MissingProperty { material, property } => {
                IrError::MissingPhysicalProperty { material, property }
            }
            crate::conversions::ConversionError::InvalidProfileConstraint(field) => {
                IrError::MissingProfileConstraint { field }
            }
            crate::conversions::ConversionError::InvalidUnit(msg) => {
                IrError::PlacementError(format!("Invalid unit: {}", msg))
            }
        }
    }
}
