//! Comptime evaluation context setup for the v0.3.0 pipeline.
//!
//! Builds a fresh [`EvaluationContext`] pre-populated from the symbol table
//! (functions, structs, enums) and pre-computes the dimensions of every `space`
//! declaration before the emitter contents are consumed.

use compact_str::CompactString;
use hwc_parser::Program;
use hwc_types::UnitRegistry;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::eval::{self, EvaluationContext, Evaluator, Value};
use crate::pipeline::error::PipelineError;
use crate::symbol_table::SymbolTable;

/// Build an evaluation context seeded from the symbol table arena (functions,
/// structs, and enum variant namespaces) and the supplied unit registry.
pub fn build_eval_context(symbol_table: &SymbolTable, unit_registry: &UnitRegistry) -> Result<EvaluationContext, PipelineError> {
    let memory_emitter = eval::MemoryEmitter::new();
    let mut ctx = eval::EvaluationContext::with_emitter(Box::new(memory_emitter));
    ctx.unit_registry = Some(Arc::new(unit_registry.clone()));

    // Populate ctx.functions, ctx.structs, and enums from the symbol table arena
    // This includes all imported symbols that were registered by the module resolver
    for func_def in symbol_table.arena().function_defs.iter() {
        ctx.functions
            .insert(func_def.name.name.clone(), func_def.clone());
    }

    for struct_def in symbol_table.arena().struct_defs.iter() {
        ctx.structs
            .insert(struct_def.name.name.clone(), struct_def.clone());
    }

    // Add enum variant bindings to the evaluation context
    // For enums, we store the enum type (as a namespace) in ctx.enum_types
    for enum_def in symbol_table.arena().enum_defs.iter() {
        // Create a map of variant names to their values for this enum
        let mut variants_map = FxHashMap::default();
        for variant in &enum_def.variants {
            let variant_value = Value::EnumVariant {
                enum_name: enum_def.name.name.clone(),
                variant_name: variant.name.clone(),
                payload: None,
            };
            variants_map.insert(variant.name.clone(), variant_value);
        }

        // Store the enum type in ctx.enum_types for the bytecode compiler
        let enum_namespace = Value::EnumType {
            name: enum_def.name.name.clone(),
            variants: Arc::new(variants_map),
        };
        ctx.enum_types
            .insert(enum_def.name.name.clone(), enum_namespace);
    }

    // Evaluate and inject all constants from the symbol table arena into context
    let mut const_evaluator = Evaluator::new(&mut ctx);
    for const_def in symbol_table.arena().const_defs.iter() {
        let val = const_evaluator
            .eval_expression(&const_def.value)
            .map_err(|e| PipelineError {
                message: format!("Failed to evaluate const '{}': {}", const_def.name.name, e),
            })?;
        const_evaluator.ctx.insert_variable(const_def.name.name.clone(), val.clone());
        const_evaluator.ctx.constants.insert(const_def.name.name.clone(), val);
    }

    Ok(ctx)
}

/// Pre-evaluate the width/height (in nm) of every `space` declaration.
pub fn precompute_space_dimensions(
    program: &Program,
    evaluator: &mut Evaluator,
) -> FxHashMap<CompactString, (i64, i64)> {
    let mut space_dims = FxHashMap::default();
    for item in &program.items {
        if let hwc_parser::TopLevelItem::Space(space_decl) = item {
            let (width_nm, height_nm) = if let Some((w_expr, h_expr)) = &space_decl.dimensions {
                let w = evaluator
                    .eval_expression(w_expr)
                    .ok()
                    .and_then(|v| match v {
                        Value::Measurement(m) => Some((m.raw / 1000) as i64),
                        Value::Int(i) => Some(i),
                        _ => None,
                    })
                    .unwrap_or(20_000);
                let h = evaluator
                    .eval_expression(h_expr)
                    .ok()
                    .and_then(|v| match v {
                        Value::Measurement(m) => Some((m.raw / 1000) as i64),
                        Value::Int(i) => Some(i),
                        _ => None,
                    })
                    .unwrap_or(10_000);
                (w, h)
            } else {
                (20_000, 10_000)
            };
            space_dims.insert(space_decl.name.name.clone(), (width_nm, height_nm));
        }
    }
    space_dims
}
