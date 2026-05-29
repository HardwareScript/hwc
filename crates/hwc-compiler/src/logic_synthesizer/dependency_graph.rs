//! Dependency graph analysis for combinational loop detection
//!
//! This module builds a dependency graph of all wires and expressions in a logic block,
//! then detects cycles that would create combinational loops. Cycles are only allowed
//! if they pass through a register boundary (Reg()).
use compact_str::CompactString;

use super::SynthesisError;
use hwc_parser::logic::*;
use rustc_hash::{FxHashMap, FxHashSet};

/// Dependency graph for tracking wire dependencies
pub struct DependencyGraph {
    /// Map from wire name to the wires it depends on
    dependencies: FxHashMap<CompactString, FxHashSet<CompactString>>,
    /// Set of register names (cycles through registers are allowed)
    registers: FxHashSet<CompactString>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            dependencies: FxHashMap::default(),
            registers: FxHashSet::default(),
        }
    }

    /// Mark a wire as a register (cycles through registers are allowed)
    pub fn mark_as_register(&mut self, name: CompactString) {
        self.registers.insert(name);
    }

    /// Add a dependency: `target` depends on `source`
    pub fn add_dependency(&mut self, target: CompactString, source: CompactString) {
        self.dependencies.entry(target).or_default().insert(source);
    }

    /// Add multiple dependencies for a target wire
    pub fn add_dependencies(&mut self, target: CompactString, sources: Vec<CompactString>) {
        let deps = self.dependencies.entry(target).or_default();
        for source in sources {
            deps.insert(source);
        }
    }

    /// Extract all variable names from an expression
    pub fn extract_variables(&self, expr: &LogicExpression) -> Vec<CompactString> {
        let mut vars = Vec::new();
        self.extract_variables_recursive(expr, &mut vars);
        vars
    }

    /// Recursively extract variables from an expression
    fn extract_variables_recursive(&self, expr: &LogicExpression, vars: &mut Vec<CompactString>) {
        match expr {
            LogicExpression::Variable { name, .. } => {
                vars.push(name.clone());
            }
            LogicExpression::Binary { left, right, .. } => {
                self.extract_variables_recursive(left, vars);
                self.extract_variables_recursive(right, vars);
            }
            LogicExpression::Grouped { expression, .. } => {
                self.extract_variables_recursive(expression, vars);
            }
            LogicExpression::If {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                self.extract_variables_recursive(condition, vars);
                self.extract_variables_from_block(then_expr, vars);
                self.extract_variables_from_block(else_expr, vars);
            }
            LogicExpression::Match { selector, arms, .. } => {
                self.extract_variables_recursive(selector, vars);
                for arm in arms {
                    self.extract_variables_from_block(&arm.body, vars);
                }
            }
            LogicExpression::FieldAccess { base, .. } => {
                self.extract_variables_recursive(base, vars);
            }
            LogicExpression::ArrayAccess { base, .. } => {
                self.extract_variables_recursive(base, vars);
                // Range is static, no variables to extract
            }
            LogicExpression::Cast { expression, .. } => {
                self.extract_variables_recursive(expression, vars);
            }
            LogicExpression::Bundle { items, .. } => {
                for item in items {
                    match item {
                        BundleItem::Expression(expr) => {
                            self.extract_variables_recursive(expr, vars);
                        }
                        BundleItem::Duplication { value, .. } => {
                            self.extract_variables_recursive(value, vars);
                        }
                    }
                }
            }
            LogicExpression::RegisterInit {
                clock, reset, init, ..
            } => {
                // Registers break combinational loops, but we still track their dependencies
                self.extract_variables_recursive(clock, vars);
                self.extract_variables_recursive(reset, vars);
                self.extract_variables_recursive(init, vars);
            }
            LogicExpression::Unary { operand, .. } => {
                self.extract_variables_recursive(operand, vars);
            }
            // Literals and booleans have no dependencies
            LogicExpression::Literal { .. } | LogicExpression::Boolean { .. } => {}
        }
    }

    /// Extract variables from a block or expression
    fn extract_variables_from_block(&self, block: &BlockOrExpr, vars: &mut Vec<CompactString>) {
        match block {
            BlockOrExpr::Expression(expr) => {
                self.extract_variables_recursive(expr, vars);
            }
            BlockOrExpr::Block(statements) => {
                for stmt in statements {
                    self.extract_variables_from_statement(stmt, vars);
                }
            }
            BlockOrExpr::Pass(_) => {
                // Empty block, no variables
            }
        }
    }

    /// Extract variables from a statement
    fn extract_variables_from_statement(
        &self,
        stmt: &LogicStatement,
        vars: &mut Vec<CompactString>,
    ) {
        match stmt {
            LogicStatement::Expression(expr) => {
                self.extract_variables_recursive(expr, vars);
            }
            LogicStatement::Let { expression, .. } => {
                self.extract_variables_recursive(expression, vars);
            }
            LogicStatement::Assignment { expression, .. } => {
                self.extract_variables_recursive(expression, vars);
            }
            LogicStatement::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.extract_variables_recursive(condition, vars);
                self.extract_variables_from_block(then_block, vars);
                if let Some(else_blk) = else_block {
                    self.extract_variables_from_block(else_blk, vars);
                }
            }
        }
    }

    /// Detect combinational loops in the dependency graph
    /// Returns Ok(()) if no loops found, or Err with the loop chain
    pub fn detect_combinational_loops(&self) -> Result<(), SynthesisError> {
        let mut visited = FxHashSet::default();
        let mut rec_stack = FxHashSet::default();
        let mut path = Vec::new();

        // Check each wire for cycles
        for wire in self.dependencies.keys() {
            // Skip registers - cycles through registers are allowed
            if self.registers.contains(wire) {
                continue;
            }

            if !visited.contains(wire) {
                if let Some(cycle) =
                    self.detect_cycle_dfs(wire, &mut visited, &mut rec_stack, &mut path)
                {
                    // We don't have span information in the dependency graph,
                    // so we use a default span (0..0)
                    return Err(SynthesisError::combinational_loop(
                        hwc_parser::Span { start: 0, end: 0 },
                        cycle,
                    ));
                }
            }
        }

        Ok(())
    }

    /// Depth-first search to detect cycles
    /// Returns Some(cycle_description) if a cycle is found
    fn detect_cycle_dfs(
        &self,
        wire: &str,
        visited: &mut FxHashSet<CompactString>,
        rec_stack: &mut FxHashSet<CompactString>,
        path: &mut Vec<CompactString>,
    ) -> Option<CompactString> {
        // Mark this wire as visited and add to recursion stack
        visited.insert(wire.into());
        rec_stack.insert(wire.into());
        path.push(wire.into());

        // Check all dependencies
        if let Some(deps) = self.dependencies.get(wire) {
            for dep in deps {
                // Skip registers - they break combinational loops
                if self.registers.contains(dep) {
                    continue;
                }

                // If dependency is in recursion stack, we found a cycle
                if rec_stack.contains(dep) {
                    // Build the cycle path
                    let cycle_start = path.iter().position(|w| w == dep).unwrap();
                    let mut cycle_path: Vec<CompactString> = path[cycle_start..].to_vec();
                    cycle_path.push(dep.clone()); // Close the loop

                    return Some(format!(
                        "{}\n  Hint: Insert a register to break the loop: let {} = reg(clock: Clk, reset: Rst, init: 0)",
                        cycle_path.join(" → "),
                        cycle_path[0]
                    ).into());
                }

                // If not visited, recurse
                if !visited.contains(dep) {
                    if let Some(cycle) = self.detect_cycle_dfs(dep, visited, rec_stack, path) {
                        return Some(cycle);
                    }
                }
            }
        }

        // Remove from recursion stack and path
        rec_stack.remove(wire);
        path.pop();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cycle() {
        let mut graph = DependencyGraph::new();

        // a = input
        // b = a + 1
        // c = b + 1
        graph.add_dependency("b".into(), "a".into());
        graph.add_dependency("c".into(), "b".into());

        assert!(graph.detect_combinational_loops().is_ok());
    }

    #[test]
    fn test_simple_cycle() {
        let mut graph = DependencyGraph::new();

        // a = b + 1
        // b = a + 1  <- Cycle!
        graph.add_dependency("a".into(), "b".into());
        graph.add_dependency("b".into(), "a".into());

        let result = graph.detect_combinational_loops();
        assert!(result.is_err());

        if let Err(SynthesisError::CombinationalLoop { chain, .. }) = result {
            assert!(chain.contains("→"));
            assert!(chain.contains("Hint"));
        }
    }

    #[test]
    fn test_cycle_through_register() {
        let mut graph = DependencyGraph::new();

        // a = b + 1
        // b = reg(...)  <- Register breaks the cycle
        // b.next = a
        graph.add_dependency("a".into(), "b".into());
        graph.add_dependency("b".into(), "a".into());
        graph.mark_as_register("b".into());

        // Should be OK - cycle goes through register
        assert!(graph.detect_combinational_loops().is_ok());
    }

    #[test]
    fn test_complex_cycle() {
        let mut graph = DependencyGraph::new();

        // a = b + 1
        // b = c + 1
        // c = d + 1
        // d = a + 1  <- Cycle through 4 wires!
        graph.add_dependency("a".into(), "b".into());
        graph.add_dependency("b".into(), "c".into());
        graph.add_dependency("c".into(), "d".into());
        graph.add_dependency("d".into(), "a".into());

        let result = graph.detect_combinational_loops();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_dependencies() {
        let mut graph = DependencyGraph::new();

        // a = input1
        // b = input2
        // c = a + b  <- Depends on both a and b
        graph.add_dependencies("c".into(), vec!["a".into(), "b".into()]);

        assert!(graph.detect_combinational_loops().is_ok());
    }
}
