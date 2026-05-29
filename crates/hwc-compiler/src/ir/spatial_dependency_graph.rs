//! Spatial dependency graph for topological sorting of components.
//!
//! v0.1.6 Gap 7: Spatial Topological Sorting
//!
//! This module implements a directed graph that tracks spatial dependencies
//! between components (e.g., when component B is positioned relative to component A).
//! It allows the compiler to detect circular references and to determine the
//! correct placement order regardless of the textual order in the source file.

use compact_str::CompactString;
use hwc_parser::{Coordinate, Expression, RelativeOffset};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ir::errors::IrError;

/// A graph representing spatial dependencies between components.
#[derive(Debug, Clone, Default)]
pub struct SpatialDependencyGraph {
    /// Adjacency list: component_name -> set of component_names it depends on.
    /// A depends on B if A's position references an anchor of B.
    dependencies: FxHashMap<CompactString, FxHashSet<CompactString>>,

    /// The order in which components appeared in the source code (textual order).
    /// Used for resolving the 'last' keyword and for stable sorting of independent nodes.
    textual_order: Vec<CompactString>,
}

impl SpatialDependencyGraph {
    /// Create a new, empty spatial dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a component to the graph and record its textual order.
    pub fn add_component(&mut self, name: CompactString) {
        if !self.dependencies.contains_key(&name) {
            self.dependencies.insert(name.clone(), FxHashSet::default());
            self.textual_order.push(name);
        }
    }

    /// Add a dependency: `dependent` depends on `dependency`.
    pub fn add_dependency(&mut self, dependent: CompactString, dependency: CompactString) {
        self.add_component(dependent.clone());
        // We allow dependency to be added even if not yet in textual_order
        // (it will be added when its own statement is processed).
        if dependent != dependency {
            self.dependencies
                .entry(dependent)
                .or_default()
                .insert(dependency);
        }
    }

    /// Extract dependencies from a coordinate expression.
    ///
    /// # Arguments
    /// * `dependent` - The name of the component being placed.
    /// * `coord` - The coordinate expression to scan for anchor references.
    /// * `last_name` - The name of the previously defined component (for 'last' keyword).
    pub fn extract_dependencies_from_coord(
        &mut self,
        dependent: &CompactString,
        coord: &Coordinate,
        last_name: Option<&CompactString>,
    ) {
        match coord {
            Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
                self.extract_dependencies_from_expr(dependent, x, last_name);
                self.extract_dependencies_from_expr(dependent, y, last_name);
                self.extract_dependencies_from_expr(dependent, z, last_name);
            }
            Coordinate::Relative(rel) => {
                if rel.anchor.name == "last" {
                    if let Some(last) = last_name {
                        self.add_dependency(dependent.clone(), last.clone());
                    }
                } else {
                    self.add_dependency(dependent.clone(), rel.anchor.name.clone());
                }

                match &rel.offset {
                    RelativeOffset::Vector { x, y, z } => {
                        self.extract_dependencies_from_expr(dependent, x, last_name);
                        self.extract_dependencies_from_expr(dependent, y, last_name);
                        self.extract_dependencies_from_expr(dependent, z, last_name);
                    }
                    RelativeOffset::Single(_) => {}
                }
            }
        }
    }

    /// Recursively scan an expression for anchor references.
    /// Extract dependencies from an expression (v0.1.7)
    pub fn extract_dependencies_from_expr(
        &mut self,
        dependent: &CompactString,
        expr: &Expression,
        last_name: Option<&CompactString>,
    ) {
        match expr {
            Expression::Binary { left, right, .. } => {
                self.extract_dependencies_from_expr(dependent, left, last_name);
                self.extract_dependencies_from_expr(dependent, right, last_name);
            }
            Expression::Unary { operand, .. } => {
                self.extract_dependencies_from_expr(dependent, operand, last_name);
            }
            Expression::Grouped { expression, .. } => {
                self.extract_dependencies_from_expr(dependent, expression, last_name);
            }
            Expression::AnchorReference { anchor, .. } => {
                if anchor.name == "last" {
                    if let Some(last) = last_name {
                        self.add_dependency(dependent.clone(), last.clone());
                    }
                } else {
                    self.add_dependency(dependent.clone(), anchor.name.clone());
                }
            }
            _ => {}
        }
    }

    /// Detect circular dependencies using Depth-First Search.
    ///
    /// Returns an error if a cycle is found, including the path of the cycle.
    pub fn detect_circular_dependencies(&self) -> Result<(), IrError> {
        let mut visited = FxHashSet::default();
        let mut stack = FxHashSet::default();

        for node in &self.textual_order {
            if !visited.contains(node) {
                let mut path = Vec::new();
                if self.has_cycle(node, &mut visited, &mut stack, &mut path) {
                    return Err(IrError::CircularReference {
                        path: path.join(" -> "),
                    });
                }
            }
        }
        Ok(())
    }

    fn has_cycle(
        &self,
        node: &CompactString,
        visited: &mut FxHashSet<CompactString>,
        stack: &mut FxHashSet<CompactString>,
        path: &mut Vec<String>,
    ) -> bool {
        visited.insert(node.clone());
        stack.insert(node.clone());
        path.push(node.to_string());

        if let Some(deps) = self.dependencies.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if self.has_cycle(dep, visited, stack, path) {
                        return true;
                    }
                } else if stack.contains(dep) {
                    path.push(dep.to_string());
                    return true;
                }
            }
        }

        stack.remove(node);
        path.pop();
        false
    }

    /// Perform a topological sort of the components.
    ///
    /// Returns the component names in an order that satisfies all spatial dependencies.
    /// If no dependencies exist, the textual order is preserved.
    pub fn topological_sort(&self) -> Result<Vec<CompactString>, IrError> {
        self.detect_circular_dependencies()?;

        let mut sorted = Vec::new();
        let mut visited = FxHashSet::default();

        for node in &self.textual_order {
            if !visited.contains(node) {
                self.visit_topo(node, &mut visited, &mut sorted);
            }
        }

        Ok(sorted)
    }

    fn visit_topo(
        &self,
        node: &CompactString,
        visited: &mut FxHashSet<CompactString>,
        sorted: &mut Vec<CompactString>,
    ) {
        visited.insert(node.clone());

        if let Some(deps) = self.dependencies.get(node) {
            // To maintain a stable sort that is as close to textual order as possible,
            // we process dependencies in the order they appear in the graph.
            for dep in deps {
                if !visited.contains(dep) {
                    self.visit_topo(dep, visited, sorted);
                }
            }
        }

        sorted.push(node.clone());
    }
}
