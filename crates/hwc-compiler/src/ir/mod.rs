//! IR Integration Module: Transform parser AST into continuous physical representation.
//!
//! This module bridges System 1 (Parser & Compiler) and System 2 (Continuous Engine).
//! It takes the parsed AST with Symbol Table and transforms it into a fully populated
//! entity graph with placed components and routed traces.
//!
//! ## Module Structure
//! - `errors`: Error types for IR transformation
//! - `conversions`: Unit conversions and coordinate transformations
//! - `space_builder`: Hardware space creation
//! - `placement`: Component and substrate placement
//! - `routing`: Trace routing (automatic and manual)
//! - `logic`: Logic synthesis integration (directly places into HardwareSpace)
//! - `compilation`: Modular compilation pipeline (orchestrates the full flow)
//! - `placement_item`: Placement item enum for topological sort
//!
//! ## Known Limitations (v0.1.7)
//! - **Realization Lag**: Physical boundaries of pours/contacts referencing component anchors may default
//!   to [0,0,0] if the anchor isn't fully realized at the time of evaluation.
//! - **No Collision Avoidance**: Conductive pours can interpenetrate component geometry.
//! - **Analytic Complexity**: Very large designs may require spatial index optimization.

pub mod anchor_arithmetic; // v0.2.1: Comptime anchor arithmetic evaluator
pub mod bridge_validator;
pub mod compilation;
pub mod conversions;
pub mod device_registry; // v0.2.1: Device instance registry population
pub mod errors;
pub mod logic;
pub mod meander_injection;
pub mod parametric_unroller;
pub mod placement;
pub mod placement_item;
pub mod query_engine;
pub mod relational_resolver;
pub mod routing;
pub mod space_builder;
pub mod spatial_dependency_graph;
pub mod stackup_manager;
pub mod units;

// Re-export commonly used items
pub use compilation::{
    compile_single_space, program_to_space, program_to_spaces, program_to_spaces_with_lockfile,
    save_routes_to_lockfile,
};
pub use errors::IrError;
pub use placement_item::PlacementItem;
pub use routing::route_trace;
pub use space_builder::create_hardware_space;
pub use stackup_manager::StackupManager;
pub use units::{format_distance, format_position_mm, nm_to_mm};
