//! IR Integration: Transform parser AST into continuous physical representation.
//!
//! This module bridges System 1 (Parser & Compiler) and System 2 (Engine).
//! It takes the parsed AST with Symbol Table and transforms it into a fully populated
//! entity graph with placed components and routed traces.
//!
//! **Phase 4.1 Complete**: Moved from hwc-engine to hwc-compiler to access Symbol Table.
//! **Modular Structure**: The implementation is now split into logical submodules in `src/ir/`:
//! - `errors.rs`: Error types for IR transformation
//! - `conversions.rs`: Unit conversions and coordinate transformations  
//! - `space_builder.rs`: Hardware space creation
//! - `placement.rs`: Component and substrate placement
//! - `routing/`: Trace routing (automatic A* and manual waypoint)
//! - `tests.rs`: Integration tests
//!
//! This file serves as the public API entry point, re-exporting from the modular implementation.

// Re-export from the modular ir implementation
pub use crate::ir::{program_to_space, program_to_spaces, program_to_spaces_with_lockfile, IrError};

// Additional re-exports for convenience
pub use crate::ir::{create_hardware_space, route_trace};
