//! v0.3.0 Compilation Pipeline
//!
//! Lowers a parsed HardwareScript [`hwc_parser::Program`] into one
//! [`hwc_engine::HardwareSpace`] per `space` declaration. Extracted from the
//! monolithic `lib.rs` for maintainability.
//!
//! The top-level entry points are [`program_to_spaces_with_lockfile`] and its
//! single-space convenience wrapper [`program_to_space`]. The work is split
//! across focused submodules:
//!
//! * [`eval_setup`]  – builds the comptime evaluation context and pre-computes space dims.
//! * [`material`]    – builds the universal [`hwc_engine::MaterialRegistry`] from the symbol table.
//! * [`profile`]     – parses a `profile` declaration into a [`hwc_materials::ConstraintSet`].
//! * [`space`]       – orchestrates a single space build (stackup, slabs, nets, primitives).
//! * [`pours`], [`contacts`], [`devices`], [`routes`] – inject emitted primitives into a space.

use compact_str::CompactString;
use hwc_engine::HardwareSpace;
use hwc_parser::Program;
use hwc_types::{NetId, UnitRegistry};
use rustc_hash::FxHashMap;

use crate::eval::{self, Evaluator};
use crate::symbol_table::SymbolTable;
use crate::DiagnosticCollector;

pub mod contacts;
pub mod devices;
pub mod error;
pub mod eval_setup;
pub mod material;
pub mod profile;
pub mod pours;
pub mod routes;
pub mod space;

pub use error::PipelineError;

/// Helper function to extract numeric value from Expression for material properties
pub(crate) fn extract_numeric_value(expr: &hwc_parser::Expression) -> Option<f64> {
    match expr {
        hwc_parser::Expression::Literal { value, .. } => Some(*value as f64),
        hwc_parser::Expression::FloatLiteral { value, .. } => Some(*value),
        hwc_parser::Expression::Measurement { value, .. } => Some(*value),
        _ => None,
    }
}

/// Transform a parsed program into one [`HardwareSpace`] per `space` declaration.
///
/// See the module-level docs for how this relates to the v0.3.0 pipeline.
pub fn program_to_spaces_with_lockfile(
    program: &Program,
    symbol_table: &SymbolTable,
    _collector: &DiagnosticCollector,
    _lockfile_path: Option<&std::path::Path>,
    _source_content: Option<&str>,
    _force_reroute: bool,
    unit_registry: &UnitRegistry,
) -> Result<FxHashMap<CompactString, HardwareSpace>, PipelineError> {
    let mut ctx = eval_setup::build_eval_context(symbol_table, unit_registry);

    // Populate ctx.functions, ctx.structs, and enums from the symbol table arena
    // This includes all imported symbols that were registered by the module resolver
    eprintln!(
        "[PIPELINE DEBUG] Loading {} functions, {} structs, {} enums from symbol table arena",
        symbol_table.arena().function_defs.len(),
        symbol_table.arena().struct_defs.len(),
        symbol_table.arena().enum_defs.len()
    );

    let mut evaluator = Evaluator::new(&mut ctx);
    if let Err(e) = evaluator.eval_program(program) {
        return Err(PipelineError {
            message: format!("Comptime evaluation error: {}", e),
        });
    }

    // Pre-evaluate space dimensions while evaluator is available
    let space_dims = eval_setup::precompute_space_dimensions(program, &mut evaluator);
    drop(evaluator);

    eprintln!(
        "[PIPELINE DEBUG] Registered Structs in Context: {:?}",
        ctx.structs.keys().collect::<Vec<_>>()
    );
    eprintln!(
        "[PIPELINE DEBUG] Registered Functions in Context: {:?}",
        ctx.functions.keys().collect::<Vec<_>>()
    );

    // 1. Build universal MaterialRegistry from symbol table
    let base_material_registry = material::build_material_registry(symbol_table);

    let mem = ctx
        .emitter
        .as_any()
        .downcast_ref::<eval::MemoryEmitter>()
        .unwrap();

    // Build reverse NetId -> NetName mapping shared across all spaces
    let mut net_id_to_name: FxHashMap<NetId, CompactString> = FxHashMap::default();
    for (name, id) in &mem.nets {
        net_id_to_name.insert(*id, name.clone());
    }

    let mut result_spaces = FxHashMap::default();
    for item in &program.items {
        if let hwc_parser::TopLevelItem::Space(space_decl) = item {
            let space_name = space_decl.name.name.clone();
            let (width_nm, height_nm) = space_dims
                .get(&space_name)
                .copied()
                .unwrap_or((20_000, 10_000));

            let hw_space = space::build_space(
                space_decl,
                width_nm,
                height_nm,
                symbol_table,
                &base_material_registry,
                mem,
                &net_id_to_name,
            )?;
            result_spaces.insert(space_name, hw_space);
        }
    }

    Ok(result_spaces)
}

/// Transform a parsed program into a single flattened [`HardwareSpace`].
pub fn program_to_space(
    program: &Program,
    symbol_table: &SymbolTable,
    collector: &DiagnosticCollector,
    unit_registry: &UnitRegistry,
) -> Result<HardwareSpace, PipelineError> {
    let spaces = program_to_spaces_with_lockfile(
        program,
        symbol_table,
        collector,
        None,
        None,
        false,
        unit_registry,
    )?;

    spaces.into_values().next().ok_or_else(|| PipelineError {
        message: "No spaces defined in program".to_string(),
    })
}
