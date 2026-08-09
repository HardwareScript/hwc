//! Core unrolling logic for for loops
//!
//! Handles the main loop expansion and delegates to specialized unrollers
//! for each statement type (components, pours, contacts, routes).
//!
//! v0.2.x: Unrolled nodes are allocated directly into the mutable `AstArena`
//! and referenced by their type-safe arena ID. No AST node is cloned or boxed,
//! and no `EvaluationContext` is cloned per item — loop variables are
//! substituted into each node's fields as it is unrolled.

use super::collision::{
    print_identity_collision_warning, print_same_iteration_collision_warnings, CollisionWarning,
};
use super::substitution::{
    format_net_name, unroll_component, unroll_contact, unroll_plane, unroll_polygon, unroll_pour,
    unroll_route, unroll_space_instance,
};
use crate::ir::errors::IrError;
use crate::SymbolTable;
use compact_str::CompactString;
use hwc_parser::ast::arena::{
    AstArena, ComponentId, ContactId, ForLoopId, PlaneId, PolygonId, PourId, RouteId,
    SpaceInstanceId,
};
use hwc_parser::{EvaluationContext, SpaceIfConditional, SpaceStatement, Value};
use rustc_hash::{FxHashMap, FxHashSet};

/// Result of unrolling a for loop.
///
/// Every item is a 4-byte arena ID. Loop variables (and loop-scoped `let`
/// bindings) are already baked into the allocated nodes, so no per-item
/// evaluation context needs to travel alongside them.
#[derive(Debug, Default)]
pub struct UnrolledStatements {
    pub components: Vec<ComponentId>,
    pub pours: Vec<PourId>,
    pub planes: Vec<PlaneId>,
    pub polygons: Vec<PolygonId>,
    pub contacts: Vec<ContactId>,
    pub space_instances: Vec<SpaceInstanceId>,
    pub routes: Vec<RouteId>,
}

// ============================================================================
// UnrollContext: Centralized state for a single loop iteration
// ============================================================================

/// Tracks all state for a single loop iteration being unrolled.
struct UnrollContext<'a> {
    loop_variable: CompactString,
    iteration_value: usize,
    eval_context: EvaluationContext,
    /// Mutable arena so unrolled nodes can be allocated in place.
    arena: &'a mut AstArena,
    components: Vec<ComponentId>,
    pours: Vec<PourId>,
    planes: Vec<PlaneId>,
    polygons: Vec<PolygonId>,
    contacts: Vec<ContactId>,
    space_instances: Vec<SpaceInstanceId>,
    routes: Vec<RouteId>,
    nets_in_iteration: FxHashSet<CompactString>,
    collision_warnings: Vec<CollisionWarning>,
}

impl<'a> UnrollContext<'a> {
    fn new(
        loop_variable: CompactString,
        iteration_value: usize,
        eval_context: EvaluationContext,
        arena: &'a mut AstArena,
    ) -> Self {
        Self {
            loop_variable,
            iteration_value,
            eval_context,
            arena,
            components: Vec::new(),
            pours: Vec::new(),
            planes: Vec::new(),
            polygons: Vec::new(),
            contacts: Vec::new(),
            space_instances: Vec::new(),
            routes: Vec::new(),
            nets_in_iteration: FxHashSet::default(),
            collision_warnings: Vec::new(),
        }
    }

    fn add_let_binding(&mut self, name: CompactString, value: Value) {
        self.eval_context.insert(name, value);
    }

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

    #[allow(clippy::too_many_arguments)]
    fn drain_into(
        self,
        out: &mut UnrolledStatements,
        all_collision_warnings: &mut Vec<CollisionWarning>,
        net_usage: &mut FxHashMap<usize, FxHashSet<CompactString>>,
    ) {
        out.components.extend(self.components);
        out.pours.extend(self.pours);
        out.planes.extend(self.planes);
        out.polygons.extend(self.polygons);
        out.contacts.extend(self.contacts);
        out.space_instances.extend(self.space_instances);
        out.routes.extend(self.routes);
        all_collision_warnings.extend(self.collision_warnings);
        net_usage.insert(self.iteration_value, self.nets_in_iteration);
    }
}

// ============================================================================
// StatementProcessor: Clean dispatch for each statement type
// ============================================================================

trait StatementProcessor {
    fn process_component(&mut self, comp: ComponentId) -> Result<(), IrError>;
    fn process_pour(&mut self, pour: PourId) -> Result<(), IrError>;
    fn process_plane(&mut self, plane: PlaneId) -> Result<(), IrError>;
    fn process_polygon(&mut self, polygon: PolygonId) -> Result<(), IrError>;
    fn process_contact(&mut self, contact: ContactId) -> Result<(), IrError>;
    fn process_space_instance(&mut self, inst: SpaceInstanceId) -> Result<(), IrError>;
    fn process_route(&mut self, route: RouteId) -> Result<(), IrError>;
    fn process_let(&mut self, let_binding: &hwc_parser::LetBinding) -> Result<(), IrError>;
    fn process_if(
        &mut self,
        if_stmt: &SpaceIfConditional,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError>;
    fn process_for_loop(
        &mut self,
        for_loop_id: ForLoopId,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError>;

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
            SpaceStatement::Component(c) => self.process_component(*c),
            SpaceStatement::Pour(p) => self.process_pour(*p),
            SpaceStatement::Plane(p) => self.process_plane(*p),
            SpaceStatement::Polygon(p) => self.process_polygon(*p),
            SpaceStatement::Contact(c) => self.process_contact(*c),
            SpaceStatement::SpaceInstance(si) => self.process_space_instance(*si),
            SpaceStatement::Route(r) => self.process_route(*r),
            SpaceStatement::Let(l) => self.process_let(l),
            SpaceStatement::If(i) => self.process_if(i, symbol_table),
            SpaceStatement::ForLoop(fl) => self.process_for_loop(*fl, symbol_table),
        }
    }

    fn process_component(&mut self, comp_id: ComponentId) -> Result<(), IrError> {
        let unrolled = unroll_component(
            &self.arena.components[comp_id],
            &self.loop_variable,
            self.iteration_value,
        )?;
        let new_id = self.arena.alloc_component(unrolled);
        self.components.push(new_id);
        Ok(())
    }

    fn process_pour(&mut self, pour_id: PourId) -> Result<(), IrError> {
        let unrolled = unroll_pour(
            &self.arena.pours[pour_id],
            &self.loop_variable,
            self.iteration_value,
        )?;

        if let Some(ref net) = unrolled.net {
            let net_str = format_net_name(net);
            self.track_net_collision(net_str, "pour", unrolled.name.to_string());
        }

        let new_id = self.arena.alloc_pour(unrolled);
        self.pours.push(new_id);
        Ok(())
    }

    fn process_plane(&mut self, plane_id: PlaneId) -> Result<(), IrError> {
        let unrolled = unroll_plane(
            &self.arena.planes[plane_id],
            &self.loop_variable,
            self.iteration_value,
        )?;
        let new_id = self.arena.alloc_plane(unrolled);
        self.planes.push(new_id);
        Ok(())
    }

    fn process_polygon(&mut self, polygon_id: PolygonId) -> Result<(), IrError> {
        let unrolled = unroll_polygon(
            &self.arena.polygons[polygon_id],
            &self.loop_variable,
            self.iteration_value,
        )?;
        let new_id = self.arena.alloc_polygon(unrolled);
        self.polygons.push(new_id);
        Ok(())
    }

    fn process_contact(&mut self, contact_id: ContactId) -> Result<(), IrError> {
        let unrolled = unroll_contact(
            &self.arena.contacts[contact_id],
            &self.loop_variable,
            self.iteration_value,
        )?;

        if let Some(ref net) = unrolled.net {
            let net_str = format_net_name(net);
            self.track_net_collision(net_str, "contact", unrolled.name.base.clone());
        }

        let new_id = self.arena.alloc_contact(unrolled);
        self.contacts.push(new_id);
        Ok(())
    }

    fn process_space_instance(&mut self, space_inst_id: SpaceInstanceId) -> Result<(), IrError> {
        let unrolled = unroll_space_instance(
            &self.arena.space_instances[space_inst_id],
            &self.loop_variable,
            self.iteration_value,
        )?;

        for parent_net in unrolled.net_map.values() {
            let net_str: CompactString = parent_net.clone();
            self.track_net_collision(
                net_str,
                "space_instance",
                unrolled.instance_name.base.clone(),
            );
        }

        let new_id = self.arena.alloc_space_instance(unrolled);
        self.space_instances.push(new_id);
        Ok(())
    }

    fn process_route(&mut self, route_id: RouteId) -> Result<(), IrError> {
        let unrolled = unroll_route(
            &self.arena.routes[route_id],
            &self.loop_variable,
            self.iteration_value,
        )?;
        let new_id = self.arena.alloc_route(unrolled);
        self.routes.push(new_id);
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
        if_stmt: &SpaceIfConditional,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError> {
        let condition_value = if_stmt
            .condition
            .evaluate(&self.eval_context)
            .map_err(|e| {
                IrError::InvalidExpression(format!("Failed to evaluate if condition: {}", e))
            })?;

        let is_true = match condition_value {
            Value::Number(n) => n != 0,
            Value::Float(f) => f != 0.0,
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
        nested_loop_id: ForLoopId,
        symbol_table: &SymbolTable,
    ) -> Result<(), IrError> {
        // Dereference the ForLoopId to get the actual SpaceForLoop data
        let (start, end, variable) = {
            let nested = &self.arena.for_loops[nested_loop_id];
            (nested.start, nested.end, nested.variable.clone())
        };

        // Take the body (leaving an empty Vec behind) so the arena can be borrowed
        // mutably during nested unroll. This is a 3-word move, not a deep clone.
        let body = std::mem::take(&mut self.arena.for_loops[nested_loop_id].body);

        let nested = run_unroll(
            start,
            end,
            &body,
            &variable,
            symbol_table,
            &self.eval_context,
            self.arena,
        );

        // Restore the body unconditionally so the arena is never left mutated
        self.arena.for_loops[nested_loop_id].body = body;

        let nested = nested?;

        // Merge nested results, substituting the current loop variable.
        // The nested unroll already applied the inner loop variable; this
        // second pass applies the *outer* loop variable (e.g. `{{i}}`).
        for id in nested.components {
            let unrolled = unroll_component(
                &self.arena.components[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_component(unrolled);
            self.components.push(new_id);
        }

        for id in nested.pours {
            let unrolled = unroll_pour(
                &self.arena.pours[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_pour(unrolled);
            self.pours.push(new_id);
        }

        for id in nested.planes {
            let unrolled = unroll_plane(
                &self.arena.planes[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_plane(unrolled);
            self.planes.push(new_id);
        }

        for id in nested.polygons {
            let unrolled = unroll_polygon(
                &self.arena.polygons[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_polygon(unrolled);
            self.polygons.push(new_id);
        }

        for id in nested.contacts {
            let unrolled = unroll_contact(
                &self.arena.contacts[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_contact(unrolled);
            self.contacts.push(new_id);
        }

        for id in nested.space_instances {
            let unrolled = unroll_space_instance(
                &self.arena.space_instances[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_space_instance(unrolled);
            self.space_instances.push(new_id);
        }

        for id in nested.routes {
            let unrolled = unroll_route(
                &self.arena.routes[id],
                &self.loop_variable,
                self.iteration_value,
            )?;
            let new_id = self.arena.alloc_route(unrolled);
            self.routes.push(new_id);
        }

        Ok(())
    }
}

/// Shared iteration body for a single for-loop expansion.
fn run_unroll(
    start: usize,
    end: usize,
    body: &[SpaceStatement],
    loop_variable: &CompactString,
    symbol_table: &SymbolTable,
    parent_context: &EvaluationContext,
    arena: &mut AstArena,
) -> Result<UnrolledStatements, IrError> {
    let mut out = UnrolledStatements::default();
    let mut all_collision_warnings = Vec::new();
    let mut net_usage_per_iteration: FxHashMap<usize, FxHashSet<CompactString>> =
        FxHashMap::default();

    // INCLUSIVE range iteration (Hardware Engineering Convention): 0..4 = [0,1,2,3,4]
    for i in start..=end {
        let mut iteration_context = parent_context.clone();
        iteration_context.insert(loop_variable.clone(), Value::Number(i as i64));

        let mut ctx = UnrollContext::new(loop_variable.clone(), i, iteration_context, arena);

        for statement in body {
            ctx.process_statement(statement, symbol_table)?;
        }

        ctx.drain_into(
            &mut out,
            &mut all_collision_warnings,
            &mut net_usage_per_iteration,
        );
    }

    let mut all_nets_to_iterations: FxHashMap<CompactString, Vec<usize>> = FxHashMap::default();
    for (iteration, nets) in &net_usage_per_iteration {
        for net in nets {
            all_nets_to_iterations
                .entry(net.clone())
                .or_default()
                .push(*iteration);
        }
    }

    for (net_name, iterations) in &all_nets_to_iterations {
        if iterations.len() > 1 {
            print_identity_collision_warning(net_name, iterations, loop_variable);
        }
    }

    if !all_collision_warnings.is_empty() {
        print_same_iteration_collision_warnings(&all_collision_warnings);
    }

    Ok(out)
}

// ============================================================================
// Public API
// ============================================================================

/// Unroll a for loop (referenced by ID) into individual arena-allocated items.
///
/// The loop body is **moved out of the arena and moved back**, never cloned:
/// this splits the borrow (shared body vs. mutable arena) at O(1) cost
/// regardless of body size, and the body is restored even on error.
pub fn unroll_for_loop(
    for_loop_id: ForLoopId,
    symbol_table: &SymbolTable,
    space_eval_context: &EvaluationContext,
    arena: &mut AstArena,
) -> Result<UnrolledStatements, IrError> {
    let (start, end, variable) = {
        let fl = &arena.for_loops[for_loop_id];
        (fl.start, fl.end, fl.variable.clone())
    };

    // Take the body (leaving an empty Vec behind) so the arena can be borrowed
    // mutably while we iterate. This is a 3-word move, not a deep clone.
    let body = std::mem::take(&mut arena.for_loops[for_loop_id].body);

    let result = run_unroll(
        start,
        end,
        &body,
        &variable,
        symbol_table,
        space_eval_context,
        arena,
    );

    // Restore the body unconditionally so the arena is never left mutated,
    // even when unrolling failed.
    arena.for_loops[for_loop_id].body = body;

    result
}
