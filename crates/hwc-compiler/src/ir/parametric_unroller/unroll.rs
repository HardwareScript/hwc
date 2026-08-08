//! Core unrolling logic for for loops
//!
//! Handles the main loop expansion and delegates to specialized unrollers
//! for each statement type (components, pours, contacts, routes).
//!
//! v0.2.1: Refactored to use UnrollContext + StatementProcessor pattern.
//! - Eliminates deep nesting (5-6 levels → 2 levels max)
//! - Removes code duplication across loop body, if-body, and nested-if processing
//! - Each statement type processed by a dedicated `process_*` method

use super::collision::{
    print_identity_collision_warning, print_same_iteration_collision_warnings, CollisionWarning,
};
use super::substitution::{
    format_net_name,
    unroll_component,
    unroll_contact,
    unroll_plane,
    unroll_pour,
    unroll_route,
    unroll_space_instance, // v0.2.1
};
use crate::ir::errors::IrError;
use crate::SymbolTable;
use compact_str::CompactString;
use hwc_parser::{
    ComponentPlacement, ContactPlacement, PlanePlacement, PourPlacement, Route, SpaceForLoop,
    SpaceStatement,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// v0.2.1: Contextual item with its evaluation context
#[derive(Debug, Clone)]
pub struct ContextualItem<T> {
    pub item: T,
    pub eval_context: hwc_parser::EvaluationContext,
}

/// Result of unrolling a for loop
/// v0.2.1: Each item carries its evaluation context from the loop iteration
pub struct UnrolledStatements {
    pub components: Vec<ContextualItem<ComponentPlacement>>,
    pub pours: Vec<ContextualItem<PourPlacement>>,
    pub planes: Vec<ContextualItem<PlanePlacement>>,
    pub contacts: Vec<ContextualItem<ContactPlacement>>,
    pub space_instances: Vec<ContextualItem<hwc_parser::SpaceInstancePlacement>>, // v0.2.1: Space instances
    pub routes: Vec<ContextualItem<Route>>,
}

// ============================================================================
// UnrollContext: Centralized state for a single loop iteration
// ============================================================================

/// Tracks all state for a single loop iteration being unrolled.
///
/// Replaces scattered mutable vectors with a single struct that owns:
/// - Loop variable and iteration value
/// - Evaluation context (all variables in scope)
/// - Accumulated results for each statement type
/// - Net collision tracking
struct UnrollContext<'a> {
    /// Name of the loop variable (e.g., "i", "row", "col")
    loop_variable: CompactString,
    /// Current iteration value
    iteration_value: usize,
    /// Evaluation context with all variables in scope
    eval_context: hwc_parser::EvaluationContext,
    /// Arena for looking up AST nodes by ID
    arena: &'a hwc_parser::ast::arena::AstArena,
    /// Accumulated unrolled components
    components: Vec<ContextualItem<ComponentPlacement>>,
    /// Accumulated unrolled pours
    pours: Vec<ContextualItem<PourPlacement>>,
    /// Accumulated unrolled planes
    planes: Vec<ContextualItem<PlanePlacement>>,
    /// Accumulated unrolled contacts
    contacts: Vec<ContextualItem<ContactPlacement>>,
    /// Accumulated unrolled space instances
    space_instances: Vec<ContextualItem<hwc_parser::SpaceInstancePlacement>>,
    /// Accumulated unrolled routes
    routes: Vec<ContextualItem<Route>>,
    /// Nets used in this iteration (for collision detection)
    nets_in_iteration: FxHashSet<CompactString>,
    /// Collision warnings accumulated during processing
    collision_warnings: Vec<CollisionWarning>,
}

impl<'a> UnrollContext<'a> {
    fn new(
        loop_variable: CompactString,
        iteration_value: usize,
        eval_context: hwc_parser::EvaluationContext,
        arena: &'a hwc_parser::ast::arena::AstArena,
    ) -> Self {
        Self {
            loop_variable,
            iteration_value,
            eval_context,
            arena,
            components: Vec::new(),
            pours: Vec::new(),
            planes: Vec::new(),
            contacts: Vec::new(),
            space_instances: Vec::new(),
            routes: Vec::new(),
            nets_in_iteration: FxHashSet::default(),
            collision_warnings: Vec::new(),
        }
    }

    /// Add a let binding to the evaluation context
    fn add_let_binding(&mut self, name: CompactString, value: hwc_parser::Value) {
        self.eval_context.insert(name, value);
    }

    /// Track a net name and detect collisions within this iteration
    fn track_net_collision(
        &mut self,
        net_name: CompactString,
        object_type: &str,
        object_name: CompactString,
    ) {
        if !self.nets_in_iteration.insert(net_name.clone()) {
            self.collision_warnings.push(CollisionWarning {
                iteration: self.iteration_value,
                net_name,
                object_type: object_type.into(),
                object_name,
            });
        }
    }

    /// Drain accumulated results into the final UnrolledStatements
    fn drain_into(
        self,
        components: &mut Vec<ContextualItem<ComponentPlacement>>,
        pours: &mut Vec<ContextualItem<PourPlacement>>,
        planes: &mut Vec<ContextualItem<PlanePlacement>>,
        contacts: &mut Vec<ContextualItem<ContactPlacement>>,
        space_instances: &mut Vec<ContextualItem<hwc_parser::SpaceInstancePlacement>>,
        routes: &mut Vec<ContextualItem<Route>>,
        all_collision_warnings: &mut Vec<CollisionWarning>,
        net_usage: &mut FxHashMap<usize, FxHashSet<CompactString>>,
    ) {
        components.extend(self.components);
        pours.extend(self.pours);
        planes.extend(self.planes);
        contacts.extend(self.contacts);
        space_instances.extend(self.space_instances);
        routes.extend(self.routes);
        all_collision_warnings.extend(self.collision_warnings);
        net_usage.insert(self.iteration_value, self.nets_in_iteration);
    }
}

// ============================================================================
// StatementProcessor: Clean dispatch for each statement type
// ============================================================================

/// Trait for processing individual statement types within a loop iteration.
///
/// Each `process_*` method handles one statement type, keeping the logic
/// flat and isolated. The `process_statement` method provides the main dispatch.
trait StatementProcessor {
    fn process_component(&mut self, comp: &ComponentPlacement) -> Result<(), IrError>;
    fn process_pour(&mut self, pour: &PourPlacement) -> Result<(), IrError>;
    fn process_plane(&mut self, plane: &PlanePlacement) -> Result<(), IrError>;
    fn process_contact(&mut self, contact: &ContactPlacement) -> Result<(), IrError>;
    fn process_space_instance(
        &mut self,
        inst: &hwc_parser::SpaceInstancePlacement,
    ) -> Result<(), IrError>;
    fn process_route(&mut self, route: &Route) -> Result<(), IrError>;
    fn process_let(&mut self, let_binding: &hwc_parser::LetBinding) -> Result<(), IrError>;
    fn process_if(
        &mut self,
        if_stmt: &hwc_parser::SpaceIfConditional,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError>;
    fn process_for_loop(
        &mut self,
        for_loop: &SpaceForLoop,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError>;

    /// Main dispatch: process a single statement
    fn process_statement(
        &mut self,
        stmt: &SpaceStatement,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError>;
}

impl<'a> StatementProcessor for UnrollContext<'a> {
    fn process_statement(
        &mut self,
        stmt: &SpaceStatement,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError> {
        match stmt {
            SpaceStatement::Component(c) => self.process_component(&self.arena.components[*c]),
            SpaceStatement::Pour(p) => self.process_pour(&self.arena.pours[*p]),
            SpaceStatement::Plane(p) => self.process_plane(&self.arena.planes[*p]),
            SpaceStatement::Contact(c) => self.process_contact(&self.arena.contacts[*c]),
            SpaceStatement::SpaceInstance(si) => self.process_space_instance(&self.arena.space_instances[*si]),
            SpaceStatement::Route(r) => self.process_route(&self.arena.routes[*r]),
            SpaceStatement::Let(l) => self.process_let(l),
            SpaceStatement::If(i) => self.process_if(i, symbol_table),
            SpaceStatement::ForLoop(fl) => self.process_for_loop(fl, symbol_table),
        }
    }

    fn process_component(&mut self, comp: &ComponentPlacement) -> Result<(), IrError> {
        let unrolled = unroll_component(comp, &self.loop_variable, self.iteration_value)?;
        self.components.push(ContextualItem {
            item: unrolled,
            eval_context: self.eval_context.clone(),
        });
        Ok(())
    }

    fn process_pour(&mut self, pour: &PourPlacement) -> Result<(), IrError> {
        let unrolled = unroll_pour(pour, &self.loop_variable, self.iteration_value)?;

        if let Some(ref net) = unrolled.net {
            let net_str = format_net_name(net);
            self.track_net_collision(net_str, "pour", unrolled.name.to_string());
        }

        self.pours.push(ContextualItem {
            item: unrolled,
            eval_context: self.eval_context.clone(),
        });
        Ok(())
    }

    fn process_plane(&mut self, plane: &PlanePlacement) -> Result<(), IrError> {
        let unrolled = unroll_plane(plane, &self.loop_variable, self.iteration_value)?;
        self.planes.push(ContextualItem {
            item: unrolled,
            eval_context: self.eval_context.clone(),
        });
        Ok(())
    }

    fn process_contact(&mut self, contact: &ContactPlacement) -> Result<(), IrError> {
        let unrolled = unroll_contact(contact, &self.loop_variable, self.iteration_value)?;

        if let Some(ref net) = unrolled.net {
            let net_str = format_net_name(net);
            self.track_net_collision(net_str, "contact", unrolled.name.base.clone());
        }

        self.contacts.push(ContextualItem {
            item: unrolled,
            eval_context: self.eval_context.clone(),
        });
        Ok(())
    }

    fn process_space_instance(
        &mut self,
        space_inst: &hwc_parser::SpaceInstancePlacement,
    ) -> Result<(), IrError> {
        let unrolled =
            unroll_space_instance(space_inst, &self.loop_variable, self.iteration_value)?;

        for (_child_net, parent_net) in &unrolled.net_map {
            let net_str: CompactString = parent_net.clone();
            self.track_net_collision(
                net_str,
                "space_instance",
                unrolled.instance_name.base.clone(),
            );
        }

        self.space_instances.push(ContextualItem {
            item: unrolled,
            eval_context: self.eval_context.clone(),
        });
        Ok(())
    }

    fn process_route(&mut self, route: &Route) -> Result<(), IrError> {
        let unrolled = unroll_route(route, &self.loop_variable, self.iteration_value)?;
        self.routes.push(ContextualItem {
            item: unrolled,
            eval_context: self.eval_context.clone(),
        });
        Ok(())
    }

    fn process_let(&mut self, let_binding: &hwc_parser::LetBinding) -> Result<(), IrError> {
        let value = let_binding
            .value
            .evaluate(&self.eval_context)
            .map_err(|e| {
                IrError::InvalidExpression(format!(
                    "Failed to evaluate loop-scoped let '{}': {}",
                    let_binding.name, e
                ))
            })?;

        self.add_let_binding(let_binding.name.clone(), value);
        Ok(())
    }

    fn process_if(
        &mut self,
        if_stmt: &hwc_parser::SpaceIfConditional,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError> {
        let condition_value = if_stmt
            .condition
            .evaluate(&self.eval_context)
            .map_err(|e| {
                IrError::InvalidExpression(format!("Failed to evaluate if condition: {}", e))
            })?;

        let is_true = match condition_value {
            hwc_parser::Value::Number(n) => n != 0,
            hwc_parser::Value::Float(f) => f != 0.0,
            _ => {
                return Err(IrError::InvalidExpression(
                    "If condition must evaluate to a number (0 = false, non-zero = true)".into(),
                ))
            }
        };

        let branch = if is_true {
            &if_stmt.then_body
        } else {
            &if_stmt.else_body
        };

        for stmt in branch {
            self.process_statement(stmt, symbol_table)?;
        }

        Ok(())
    }

    fn process_for_loop(
        &mut self,
        nested_loop: &SpaceForLoop,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError> {
        let nested_result =
            unroll_for_loop_with_context(nested_loop, symbol_table, &self.eval_context, self.arena)?;

        // Merge nested results, substituting the current loop variable
        for contextual_comp in nested_result.components {
            let unrolled = unroll_component(
                &contextual_comp.item,
                &self.loop_variable,
                self.iteration_value,
            )?;
            self.components.push(ContextualItem {
                item: unrolled,
                eval_context: contextual_comp.eval_context,
            });
        }

        for contextual_pour in nested_result.pours {
            let unrolled = unroll_pour(
                &contextual_pour.item,
                &self.loop_variable,
                self.iteration_value,
            )?;
            self.pours.push(ContextualItem {
                item: unrolled,
                eval_context: contextual_pour.eval_context,
            });
        }

        for contextual_plane in nested_result.planes {
            let unrolled = unroll_plane(
                &contextual_plane.item,
                &self.loop_variable,
                self.iteration_value,
            )?;
            self.planes.push(ContextualItem {
                item: unrolled,
                eval_context: contextual_plane.eval_context,
            });
        }

        for contextual_contact in nested_result.contacts {
            let unrolled = unroll_contact(
                &contextual_contact.item,
                &self.loop_variable,
                self.iteration_value,
            )?;
            self.contacts.push(ContextualItem {
                item: unrolled,
                eval_context: contextual_contact.eval_context,
            });
        }

        for contextual_space_inst in nested_result.space_instances {
            let unrolled = unroll_space_instance(
                &contextual_space_inst.item,
                &self.loop_variable,
                self.iteration_value,
            )?;
            self.space_instances.push(ContextualItem {
                item: unrolled,
                eval_context: contextual_space_inst.eval_context,
            });
        }

        for contextual_route in nested_result.routes {
            let unrolled = unroll_route(
                &contextual_route.item,
                &self.loop_variable,
                self.iteration_value,
            )?;
            self.routes.push(ContextualItem {
                item: unrolled,
                eval_context: contextual_route.eval_context,
            });
        }

        Ok(())
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Unroll a for loop into individual statements
///
/// **CRITICAL**: Hardware Script uses **inclusive ranges** (Ruby-style)
/// - `0..7` means [0,1,2,3,4,5,6,7] (8 items, both endpoints included)
/// - This matches hardware conventions (e.g., "bus bits 0..7" = 8 bits)
///
/// **v0.1.6 Sprint 3.4**: Supports `last` keyword for relative positioning
/// - `last.right` refers to the most recently placed component in the space
/// - Resolution happens during constraint solving, not during unrolling
/// - This allows `last` to work across loop boundaries (God-Tier feature!)
///
/// **v0.2.1**: Accepts an evaluation context to support nested loops with conditionals
/// - The context contains all loop variables from outer loops
/// - This enables `if (row + col) mod 2 == 0` where both variables are in scope
/// - Also contains space-level let bindings for loop-scoped expressions
pub fn unroll_for_loop(
    for_loop: &SpaceForLoop,
    _symbol_table: &SymbolTable,
    space_eval_context: &hwc_parser::EvaluationContext,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<UnrolledStatements, IrError> {
    unroll_for_loop_with_context(for_loop, _symbol_table, space_eval_context, arena)
}

/// Internal unroller that maintains evaluation context through nested loops.
///
/// Uses `UnrollContext` + `StatementProcessor` to process each statement type
/// in isolation, eliminating deep nesting and code duplication.
fn unroll_for_loop_with_context(
    for_loop: &SpaceForLoop,
    _symbol_table: &SymbolTable,
    parent_context: &hwc_parser::EvaluationContext,
    arena: &hwc_parser::ast::arena::AstArena,
) -> Result<UnrolledStatements, IrError> {
    // Accumulators for all iterations
    let mut all_components = Vec::new();
    let mut all_pours = Vec::new();
    let mut all_planes = Vec::new();
    let mut all_contacts = Vec::new();
    let mut all_space_instances = Vec::new();
    let mut all_routes = Vec::new();
    let mut all_collision_warnings = Vec::new();
    let mut net_usage_per_iteration: FxHashMap<usize, FxHashSet<CompactString>> =
        FxHashMap::default();

    // INCLUSIVE range iteration (Hardware Engineering Convention): 0..4 = [0,1,2,3,4] (5 items)
    // This matches hardware datasheets: "Resistors R1 through R5" means all 5 resistors
    // Different from programming languages but natural for hardware engineers
    for i in for_loop.start..=for_loop.end {
        // Create context for this iteration by cloning parent and adding current variable
        let mut iteration_context = parent_context.clone();
        iteration_context.insert(
            for_loop.variable.clone(),
            hwc_parser::Value::Number(i as i64),
        );

        // Create unroll context for this iteration
        let mut ctx = UnrollContext::new(for_loop.variable.clone(), i, iteration_context, arena);

        // Process all statements in loop body via trait dispatch
        for statement in &for_loop.body {
            ctx.process_statement(statement, _symbol_table)?;
        }

        // Drain accumulated results into final vectors
        ctx.drain_into(
            &mut all_components,
            &mut all_pours,
            &mut all_planes,
            &mut all_contacts,
            &mut all_space_instances,
            &mut all_routes,
            &mut all_collision_warnings,
            &mut net_usage_per_iteration,
        );
    }

    // Check for identity collisions across iterations
    // (same net name generated by different loop values due to truncation/rounding)
    let mut all_nets_to_iterations: FxHashMap<CompactString, Vec<usize>> = FxHashMap::default();
    for (iteration, nets) in &net_usage_per_iteration {
        for net in nets {
            all_nets_to_iterations
                .entry(net.clone())
                .or_default()
                .push(*iteration);
        }
    }

    // Warn about nets that appear in multiple iterations
    for (net_name, iterations) in &all_nets_to_iterations {
        if iterations.len() > 1 {
            print_identity_collision_warning(net_name, iterations, &for_loop.variable);
        }
    }

    // Warn about nets used multiple times within the same iteration
    if !all_collision_warnings.is_empty() {
        print_same_iteration_collision_warnings(&all_collision_warnings);
    }

    Ok(UnrolledStatements {
        components: all_components,
        pours: all_pours,
        planes: all_planes,
        contacts: all_contacts,
        space_instances: all_space_instances,
        routes: all_routes,
    })
}
