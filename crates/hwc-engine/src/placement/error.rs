//! Placement error types.

use compact_str::CompactString;

use crate::geometry::Point3D;

/// Placement errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum PlacementError {
    #[error("Component '{component}' collides with existing component at position {position}")]
    #[diagnostic(
        code(R12),
        url("https://docs.hw-script.org/errors/R12"),
        help("Physical Explanation: Two components occupy the same voxel space at {position}. Each voxel can only contain one component's material.\n\nSolution: Adjust placement coordinates to maintain clearance between components. Check component dimensions and ensure adequate spacing.\n\nDebugging: The collision occurs at the physical coordinates shown. Review your 'add' statements and component shapes to identify overlapping placements.")
    )]
    Collision {
        component: CompactString,
        position: Point3D,
    },

    /// Multi-label collision error showing both components.
    ///
    /// This variant is used when we have source code context and can show
    /// both components that are colliding with precise source locations.
    /// Boxed to reduce enum size.
    #[error(transparent)]
    #[diagnostic(transparent)]
    CollisionDetailed(#[from] Box<CollisionDetailedError>),

    #[error("Unknown component type '{component_type}'")]
    #[diagnostic(
        code(R13),
        url("https://docs.hw-script.org/errors/R13"),
        help("Component type not found in Symbol Table. Check that the component is defined with 'define component' before use.")
    )]
    UnknownComponent { component_type: String },

    #[error("Component '{component}' placed outside space bounds")]
    #[diagnostic(
        code(R11),
        url("https://docs.hw-script.org/errors/R11"),
        help("Physical Explanation: Components must fit within the defined space dimensions. Placing components outside bounds would result in incomplete fabrication.\n\nSolution: Either increase space dimensions or move component to valid coordinates.\n\nSpace Bounds: Check 'dimensions' field in space definition.")
    )]
    OutOfBounds { component: String },

    #[error("Invalid substrate region: start {start} must be less than end {end}")]
    #[diagnostic(
        code(R14),
        url("https://docs.hw-script.org/errors/R14"),
        help("Substrate region coordinates are invalid. Ensure start coordinates are less than end coordinates in all dimensions.")
    )]
    InvalidSubstrateRegion { start: Point3D, end: Point3D },

    #[error("Component '{component}' missing layout block")]
    #[diagnostic(
        code(R15),
        url("https://docs.hw-script.org/errors/R15"),
        help("Component definition must include a 'layout:' block with shape and pin positions for physical placement.")
    )]
    MissingLayout { component: String },

    #[error("Component layout missing shape definition")]
    #[diagnostic(
        code(R16),
        url("https://docs.hw-script.org/errors/R16"),
        help("Layout block must include 'shape:' field (e.g., 'shape: Rectangle(2.0mm, 1.25mm, 0.5mm)').")
    )]
    MissingShape,

    #[error("Invalid shape definition: '{shape}'")]
    #[diagnostic(
        code(R17),
        url("https://docs.hw-script.org/errors/R17"),
        help("Shape must be in format: Rectangle(width, height, depth) with valid measurements.")
    )]
    InvalidShape { shape: String },

    #[error("Unsupported shape type: '{shape}'")]
    #[diagnostic(
        code(R18),
        url("https://docs.hw-script.org/errors/R18"),
        help("Currently only Rectangle shapes are supported. Cylinder and custom shapes coming in future versions.")
    )]
    UnsupportedShape { shape: String },

    #[error("Invalid measurement: '{measurement}'")]
    #[diagnostic(
        code(R19),
        url("https://docs.hw-script.org/errors/R19"),
        help("Measurement must be a number followed by a unit (e.g., '2.0mm', '1.25mm'). No space between number and unit.")
    )]
    InvalidMeasurement { measurement: String },
}

/// Detailed collision error with multi-label support.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("Component collision: '{first_component}' and '{second_component}' overlap")]
#[diagnostic(
    code(R12),
    url("https://docs.hw-script.org/errors/R12"),
    help("Physical Explanation: Components cannot occupy the same voxel space. Each voxel can only contain one component's material.\n\nSolution: Increase spacing between components or adjust placement coordinates.\n\nMinimum Clearance: Check your profile definition (e.g., profiles.hw) for component-to-component clearance requirements.")
)]
pub struct CollisionDetailedError {
    #[source_code]
    pub src: String,

    pub first_component: CompactString,
    pub second_component: CompactString,

    #[label("First component placed here")]
    pub first_span: miette::SourceSpan,

    #[label("Second component overlaps here")]
    pub second_span: miette::SourceSpan,
}
