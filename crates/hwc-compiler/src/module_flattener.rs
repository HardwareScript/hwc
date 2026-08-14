//! Module Flattening and Comptime Evaluation
//!
//! Based on LANGUAGE-SPEC.md: Modules are LOGICAL, not PHYSICAL.
//! - Modules contain `add` statements WITHOUT coordinates
//! - Modules contain `route` statements WITHOUT waypoints
//! - Physical placement happens in `space` via `layout` blocks
//!
//! This module unrolls comptime constructs (for loops, if conditionals).
//!
//! ## Two-Phase Flattening (v0.1.4.2)
//!
//! Phase 1: Comptime Evaluation (existing)
//! - Unroll for loops
//! - Evaluate if conditionals
//! - Expand array indices
//!
//! Phase 2: Physical Instantiation (new)
//! - Map module instances to physical coordinates
//! - Process layout blocks
//! - Prefix component names with instance name
//! - Translate routes to global coordinate space

use compact_str::CompactString;
use hwc_parser::{
    ArrayIndex, Condition, ForLoop, IfConditional, ModuleComponentPlacement, ModuleDefinition,
    ModuleRoute, ModuleStatement,
};
use rustc_hash::FxHashMap;
use thiserror::Error;

/// Result of flattening a module
#[derive(Debug, Clone)]
pub struct FlattenedModule {
    pub components: Vec<ModuleComponentPlacement>,
    pub routes: Vec<ModuleRoute>,
}

/// Bounding box for a module (used for parallel routing)
#[derive(Debug, Clone)]
pub struct ModuleBoundingBox {
    pub min_x: i64,
    pub min_y: i64,
    pub min_z: i64,
    pub max_x: i64,
    pub max_y: i64,
    pub max_z: i64,
}

impl ModuleBoundingBox {
    /// Get the dimensions of the bounding box
    pub fn dimensions(&self) -> (i64, i64, i64) {
        (
            self.max_x - self.min_x,
            self.max_y - self.min_y,
            self.max_z - self.min_z,
        )
    }
}

#[derive(Error, Debug)]
pub enum FlattenError {
    #[error("Undefined variable: '{0}'")]
    UndefinedVariable(String),

    #[error("Invalid range: {start}..{end}")]
    InvalidRange { start: i64, end: i64 },

    #[error("Module '{0}' not found in symbol table")]
    ModuleNotFound(String),

    #[error("Component '{0}' not found in symbol table")]
    ComponentNotFound(String),

    #[error("Layout block for module instance '{0}' not found")]
    LayoutNotFound(String),

    #[error("Component '{component}' in module '{module}' has no position in layout block")]
    ComponentPositionNotFound {
        module: CompactString,
        component: String,
    },

    #[error("Failed to evaluate expression: {0}")]
    ExpressionEvaluationFailed(String),

    #[error("Nested module expansion failed for '{0}': {1}")]
    NestedModuleExpansionFailed(String, String),

    #[error("Invalid array index: {0}")]
    InvalidArrayIndex(String),
}

#[derive(Debug, Clone)]
struct ComptimeContext {
    variables: FxHashMap<CompactString, i64>,
}

impl ComptimeContext {
    fn new() -> Self {
        Self {
            variables: FxHashMap::default(),
        }
    }

    fn set_variable(&mut self, name: CompactString, value: i64) {
        self.variables.insert(name, value);
    }

    fn get_variable(&self, name: &str) -> Result<i64, FlattenError> {
        self.variables
            .get(name)
            .copied()
            .ok_or_else(|| FlattenError::UndefinedVariable(name.into()))
    }
}

pub fn flatten_module(
    module: &ModuleDefinition,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<FlattenedModule, FlattenError> {
    let mut context = ComptimeContext::new();
    let mut components = Vec::new();
    let mut routes = Vec::new();

    for statement in &module.statements {
        flatten_statement(statement, &mut context, &mut components, &mut routes, arena)?;
    }

    Ok(FlattenedModule { components, routes })
}

fn flatten_statement(
    statement: &ModuleStatement,
    context: &mut ComptimeContext,
    components: &mut Vec<ModuleComponentPlacement>,
    routes: &mut Vec<ModuleRoute>,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<(), FlattenError> {
    match statement {
        ModuleStatement::AddComponent(add_id) => {
            // Look up the actual placement from arena
            let add = &arena.module_components[*add_id];
            // Evaluate array index if present
            let mut flattened_comp = add.clone();
            if let Some(ref array_index) = add.array_index {
                let index_value = evaluate_array_index(array_index, context)?;
                // Replace the array index with the evaluated value in the name
                if let Some(ref base_name) = add.name {
                    flattened_comp.name = Some(format!("{}[{}]", base_name, index_value).into());
                    flattened_comp.array_index = None; // Clear the array index since it's now in the name
                }
            }
            components.push(flattened_comp);
        }
        ModuleStatement::Route(route) => {
            routes.push(route.clone());
        }
        ModuleStatement::For(for_loop) => {
            flatten_for_loop(for_loop, context, components, routes, arena)?;
        }
        ModuleStatement::If(if_cond) => {
            flatten_if_conditional(if_cond, context, components, routes, arena)?;
        }
    }
    Ok(())
}

fn flatten_for_loop(
    for_loop: &ForLoop,
    context: &mut ComptimeContext,
    components: &mut Vec<ModuleComponentPlacement>,
    routes: &mut Vec<ModuleRoute>,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<(), FlattenError> {
    let start = for_loop.start as i64;
    let end = for_loop.end as i64;

    if start > end {
        return Err(FlattenError::InvalidRange { start, end });
    }

    // Range semantics (Rust/Swift-style explicit):
    // - `0..3` (exclusive): Iterates 3 times [0, 1, 2] - count-driven
    // - `0..=3` (inclusive): Iterates 4 times [0, 1, 2, 3] - bound-driven
    if for_loop.inclusive {
        for i in start..=end {
            context.set_variable(for_loop.variable.clone(), i);
            for statement in &for_loop.body {
                flatten_statement(statement, context, components, routes, arena)?;
            }
        }
    } else {
        for i in start..end {
            context.set_variable(for_loop.variable.clone(), i);
            for statement in &for_loop.body {
                flatten_statement(statement, context, components, routes, arena)?;
            }
        }
    }

    Ok(())
}

fn flatten_if_conditional(
    if_cond: &IfConditional,
    context: &mut ComptimeContext,
    components: &mut Vec<ModuleComponentPlacement>,
    routes: &mut Vec<ModuleRoute>,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<(), FlattenError> {
    let condition_result = evaluate_condition(&if_cond.condition, context)?;

    if condition_result {
        for statement in &if_cond.then_body {
            flatten_statement(statement, context, components, routes, arena)?;
        }
    } else if let Some(else_body) = &if_cond.else_body {
        for statement in else_body {
            flatten_statement(statement, context, components, routes, arena)?;
        }
    }

    Ok(())
}

fn evaluate_condition(
    condition: &Condition,
    context: &ComptimeContext,
) -> Result<bool, FlattenError> {
    match condition {
        Condition::Equals { left, right } => {
            let left_val = evaluate_array_index(left, context)?;
            let right_val = evaluate_array_index(right, context)?;
            Ok(left_val == right_val)
        }
        Condition::NotEquals { left, right } => {
            let left_val = evaluate_array_index(left, context)?;
            let right_val = evaluate_array_index(right, context)?;
            Ok(left_val != right_val)
        }
        Condition::LessThan { left, right } => {
            let left_val = evaluate_array_index(left, context)?;
            let right_val = evaluate_array_index(right, context)?;
            Ok(left_val < right_val)
        }
        Condition::GreaterThan { left, right } => {
            let left_val = evaluate_array_index(left, context)?;
            let right_val = evaluate_array_index(right, context)?;
            Ok(left_val > right_val)
        }
    }
}

fn evaluate_array_index(
    index: &ArrayIndex,
    context: &ComptimeContext,
) -> Result<i64, FlattenError> {
    match index {
        ArrayIndex::Literal(n) => Ok(*n as i64),
        ArrayIndex::Variable(var) => context.get_variable(var),
        ArrayIndex::Arithmetic { left, op, right } => {
            let left_val = evaluate_array_index(left, context)?;
            let right_val = evaluate_array_index(right, context)?;
            match op {
                hwc_parser::ArithmeticOp::Add => Ok(left_val.saturating_add(right_val)),
                hwc_parser::ArithmeticOp::Subtract => Ok(left_val.saturating_sub(right_val)),
                hwc_parser::ArithmeticOp::Multiply => Ok(left_val.saturating_mul(right_val)),
                hwc_parser::ArithmeticOp::Divide => {
                    if right_val == 0 {
                        Err(FlattenError::InvalidArrayIndex(
                            "Division by zero in array index".into(),
                        ))
                    } else {
                        Ok(left_val / right_val)
                    }
                }
            }
        }
    }
}

// ============================================================================
// PHASE 2: PHYSICAL INSTANTIATION (v0.1.4.2)
// ============================================================================
// NOTE: Physical instantiation now happens in ir/placement/module.rs
// This phase is no longer implemented here.
