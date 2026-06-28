use compact_str::CompactString;

pub mod alignment;
pub mod alignment_layer; // Sprint 4.1: Alignment Layer (replaces traditional LVS)
pub mod auto_via_inserter;
pub mod bounding_box_tracker;
pub mod bridge_resolver;
pub mod compiler;
pub mod constraint_solver;
pub mod conversions;
pub mod electrical_symbol_table;
pub mod embedded_stdlib;
pub mod error_codes;
pub mod interface_validator;
pub mod ir;
pub mod ir_integration;
pub mod logic_synthesizer;
pub mod module_flattener;
pub mod module_resolver;
pub mod optimizer;
pub mod prelude;
pub mod shape_generators;
pub mod span_utils;
pub mod symbol_table;
pub mod unit_resolver;
pub mod validator;
pub mod width_inference;

pub use alignment::{
    AlignmentError, AlignmentResult, AlignmentValidator, DeviceTypeId, DeviceTypeRegistry,
    GraphMatcher, LogicalDevice, LogicalNetlist, LogicalSynthesizer, PhysicalDevice,
    PhysicalNetlist,
};
pub use auto_via_inserter::{AutoViaInserter, ViaLibrary, ViaType};
pub use bounding_box_tracker::BoundingBoxTracker;
pub use compiler::Compiler;
pub use constraint_solver::ConstraintSolver;
pub use conversions::{populate_material_database, profile_to_constraints, ConversionError};
// Re-export DiagnosticCollector from hwc-diagnostics
pub use electrical_symbol_table::{ElectricalSymbolError, ElectricalSymbolTable};
pub use hwc_diagnostics::{DiagnosticCollector, ErrorFingerprint};
pub use interface_validator::InterfaceValidator;
pub use prelude::{Prelude, PreludeError};
pub use unit_resolver::UnitResolver;
pub use width_inference::{WidthError, WidthInference};
// Re-export from modular ir_integration
pub use alignment_layer::{AlignmentReport, AlignmentViolation}; // Sprint 4.1: Alignment Layer
pub use ir::routing::AutoRouter;
pub use ir_integration::{program_to_space, program_to_spaces, program_to_spaces_with_lockfile, IrError};
pub use logic_synthesizer::{LogicSynthesizer, SynthesisError};
pub use module_flattener::{flatten_module, FlattenError, FlattenedModule, ModuleBoundingBox};
pub use module_resolver::{ModuleResolver, ResolverError};
pub use optimizer::{
    OptimizationReport, Optimizer, PlacementSuggestion, TraceWidthAdjustment, ViaOptimization,
};
pub use symbol_table::{SymbolError, SymbolTable};
pub use validator::Validator;

// Re-export BakedComponent from hwc-engine for semantic baking
pub use hwc_engine::placement::BakedComponent;

#[derive(Debug)]
pub struct CompilationMetadata {
    pub source_file: CompactString,
    pub component_count: usize,
    pub route_count: usize,
    pub warnings: Vec<CompactString>,
}
