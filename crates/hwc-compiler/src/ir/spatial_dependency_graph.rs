//! Spatial dependency graph for topological sorting of placement items.
//!
//! v0.1.6 Gap 7: Spatial Topological Sorting
//! v0.2.x: Integer-based graph (pure Arena design)
//!
//! This module implements a directed graph that tracks spatial dependencies
//! between placement items (e.g., when component B is positioned relative to
//! component A). It detects circular references and determines the correct
//! placement order regardless of textual order in the source file.
//!
//! # Architecture
//!
//! Nodes are dense `usize` indices matching `ContextualPlacementItem::item_index`.
//! Entity names are only used to *resolve* a textual reference to a node index
//! (via an interning map built once during registration); the graph itself, its
//! cycle detection, and its topological sort operate purely on integers.
//! The placement hot path therefore iterates `Vec<usize>` and indexes directly
//! into the placement-item slice — no string hashing, no per-item lookups.

use compact_str::CompactString;
use hwc_parser::{Coordinate, Expression, RelativeOffset};
use rustc_hash::FxHashMap;

use crate::ir::errors::IrError;

/// A graph representing spatial dependencies between placement items.
#[derive(Debug, Clone, Default)]
pub struct SpatialDependencyGraph {
    /// Adjacency list indexed by node: `dependencies[a]` = nodes that `a` depends on.
    /// `a` depends on `b` if `a`'s position references an anchor of `b`.
    dependencies: Vec<Vec<usize>>,

    /// Maps an entity name to its node index. Used only while *building* the
    /// graph to resolve textual references; never touched during sorting.
    name_to_index: FxHashMap<CompactString, usize>,
}

impl SpatialDependencyGraph {
    /// Create a new, empty spatial dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a graph pre-sized for `capacity` nodes.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            dependencies: Vec::with_capacity(capacity),
            name_to_index: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    /// Register a node (dense index) and, optionally, the entity name that other
    /// items may reference it by.
    ///
    /// Node indices must be registered in ascending order starting at 0 so they
    /// stay dense and aligned with the placement-item slice.
    pub fn add_node(&mut self, index: usize, name: Option<&str>) {
        if self.dependencies.len() <= index {
            self.dependencies.resize(index + 1, Vec::new());
        }
        if let Some(name) = name {
            // First registration wins so duplicate names resolve to the first
            // declaration, matching the previous name-keyed behaviour.
            self.name_to_index
                .entry(CompactString::from(name))
                .or_insert(index);
        }
    }

    /// Number of registered nodes.
    pub fn len(&self) -> usize {
        self.dependencies.len()
    }

    /// Returns true if the graph has no nodes.
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
    }

    /// Resolve an entity name to a node index, tolerating array syntax
    /// (e.g. `J0[0]` falls back to `J0`) and hierarchical instance paths (e.g. `PMOS_Inst.Source_Pad` falls back to `PMOS_Inst`).
    fn resolve(&self, name: &str) -> Option<usize> {
        if let Some(&idx) = self.name_to_index.get(name) {
            return Some(idx);
        }
        if let Some(open_bracket) = name.find('[') {
            if let Some(&idx) = self.name_to_index.get(&name[..open_bracket]) {
                return Some(idx);
            }
        }
        if let Some((instance, _)) = name.split_once('.') {
            if let Some(&idx) = self.name_to_index.get(instance) {
                return Some(idx);
            }
        }
        None
    }

    /// Add an edge: node `dependent` depends on the entity called `dependency`.
    ///
    /// Unknown names are ignored (standard materials or references validated in
    /// separate compiler passes).
    pub fn add_dependency(&mut self, dependent: usize, dependency: &str) {
        let Some(dep_idx) = self.resolve(dependency) else {
            return;
        };
        self.add_edge(dependent, dep_idx);
    }

    /// Add an edge between two known node indices.
    pub fn add_edge(&mut self, dependent: usize, dependency: usize) {
        if dependent == dependency {
            return;
        }
        let Some(edges) = self.dependencies.get_mut(dependent) else {
            return;
        };
        if !edges.contains(&dependency) {
            edges.push(dependency);
        }
    }

    /// Extract dependencies from a coordinate expression.
    ///
    /// # Arguments
    /// * `dependent` - Node index of the item being placed.
    /// * `coord` - The coordinate expression to scan for anchor references.
    /// * `last_index` - Node index of the previously defined component (`last` keyword).
    pub fn extract_dependencies_from_coord(
        &mut self,
        dependent: usize,
        coord: &Coordinate,
        last_index: Option<usize>,
    ) {
        match coord {
            Coordinate::Positional { x, y, z, .. } | Coordinate::Declarative { x, y, z, .. } => {
                self.extract_dependencies_from_expr(dependent, x, last_index);
                self.extract_dependencies_from_expr(dependent, y, last_index);
                self.extract_dependencies_from_expr(dependent, z, last_index);
            }
            Coordinate::Relative(rel) => {
                if rel.anchor.name == "last" {
                    if let Some(last) = last_index {
                        self.add_edge(dependent, last);
                    }
                } else {
                    self.add_dependency(dependent, &rel.anchor.name);
                }

                match &rel.offset {
                    RelativeOffset::Vector { x, y, z } => {
                        self.extract_dependencies_from_expr(dependent, x, last_index);
                        self.extract_dependencies_from_expr(dependent, y, last_index);
                        self.extract_dependencies_from_expr(dependent, z, last_index);
                    }
                    RelativeOffset::Single(_) => {}
                }
            }
        }
    }

    /// Recursively scan an expression for anchor references.
    pub fn extract_dependencies_from_expr(
        &mut self,
        dependent: usize,
        expr: &Expression,
        last_index: Option<usize>,
    ) {
        match expr {
            Expression::Binary { left, right, .. } => {
                self.extract_dependencies_from_expr(dependent, left, last_index);
                self.extract_dependencies_from_expr(dependent, right, last_index);
            }
            Expression::Unary { operand, .. } => {
                self.extract_dependencies_from_expr(dependent, operand, last_index);
            }
            Expression::Grouped { expression, .. } => {
                self.extract_dependencies_from_expr(dependent, expression, last_index);
            }
            Expression::AnchorReference { anchor, .. } => {
                if anchor.name == "last" {
                    if let Some(last) = last_index {
                        self.add_edge(dependent, last);
                    }
                } else {
                    self.add_dependency(dependent, &anchor.name);
                }
            }
            _ => {}
        }
    }

    /// Perform a topological sort of the placement items.
    ///
    /// Returns node indices in an order that satisfies all spatial dependencies,
    /// preserving textual (index) order where no dependency constrains it.
    /// Detects circular references and reports the offending path.
    ///
    /// Iterative DFS: avoids stack overflow on deep dependency chains that are
    /// realistic at SoC scale.
    pub fn topological_sort(&self) -> Result<Vec<usize>, IrError> {
        const UNVISITED: u8 = 0;
        const IN_PROGRESS: u8 = 1;
        const DONE: u8 = 2;

        let n = self.dependencies.len();
        let mut state = vec![UNVISITED; n];
        let mut sorted = Vec::with_capacity(n);
        // (node, index of the next edge to explore)
        let mut stack: Vec<(usize, usize)> = Vec::new();

        for root in 0..n {
            if state[root] != UNVISITED {
                continue;
            }

            stack.push((root, 0));
            state[root] = IN_PROGRESS;

            while let Some(&mut (node, ref mut edge_cursor)) = stack.last_mut() {
                let edges = &self.dependencies[node];
                if *edge_cursor < edges.len() {
                    let dep = edges[*edge_cursor];
                    *edge_cursor += 1;

                    match state[dep] {
                        UNVISITED => {
                            state[dep] = IN_PROGRESS;
                            stack.push((dep, 0));
                        }
                        IN_PROGRESS => {
                            // Cycle: reconstruct the path from the DFS stack.
                            let start = stack.iter().position(|&(n, _)| n == dep).unwrap_or(0);
                            let mut path: Vec<String> = stack[start..]
                                .iter()
                                .map(|&(n, _)| self.node_label(n))
                                .collect();
                            path.push(self.node_label(dep));
                            return Err(IrError::CircularReference {
                                path: path.join(" -> "),
                            });
                        }
                        _ => {}
                    }
                } else {
                    state[node] = DONE;
                    sorted.push(node);
                    stack.pop();
                }
            }
        }

        Ok(sorted)
    }

    /// Best-effort human-readable label for a node (used only in error paths).
    fn node_label(&self, index: usize) -> String {
        self.name_to_index
            .iter()
            .find(|(_, &idx)| idx == index)
            .map(|(name, _)| name.to_string())
            .unwrap_or_else(|| format!("item#{}", index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_in_textual_order_without_dependencies() {
        let mut graph = SpatialDependencyGraph::new();
        graph.add_node(0, Some("a"));
        graph.add_node(1, Some("b"));
        graph.add_node(2, Some("c"));

        assert_eq!(graph.topological_sort().unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn places_dependencies_before_dependents() {
        let mut graph = SpatialDependencyGraph::new();
        graph.add_node(0, Some("a"));
        graph.add_node(1, Some("b"));
        // a depends on b, so b must come first.
        graph.add_dependency(0, "b");

        assert_eq!(graph.topological_sort().unwrap(), vec![1, 0]);
    }

    #[test]
    fn resolves_array_syntax_to_base_name() {
        let mut graph = SpatialDependencyGraph::new();
        graph.add_node(0, Some("J0"));
        graph.add_node(1, Some("pad"));
        graph.add_dependency(1, "J0[3]");

        assert_eq!(graph.topological_sort().unwrap(), vec![0, 1]);
    }

    #[test]
    fn detects_cycles() {
        let mut graph = SpatialDependencyGraph::new();
        graph.add_node(0, Some("a"));
        graph.add_node(1, Some("b"));
        graph.add_dependency(0, "b");
        graph.add_dependency(1, "a");

        assert!(matches!(
            graph.topological_sort(),
            Err(IrError::CircularReference { .. })
        ));
    }

    #[test]
    fn ignores_self_dependency_and_unknown_names() {
        let mut graph = SpatialDependencyGraph::new();
        graph.add_node(0, Some("a"));
        graph.add_dependency(0, "a");
        graph.add_dependency(0, "does_not_exist");

        assert_eq!(graph.topological_sort().unwrap(), vec![0]);
    }

    #[test]
    fn handles_deep_chains_without_stack_overflow() {
        const N: usize = 100_000;
        let mut graph = SpatialDependencyGraph::with_capacity(N);
        for i in 0..N {
            graph.add_node(i, None);
        }
        for i in 1..N {
            graph.add_edge(i, i - 1);
        }

        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted.len(), N);
        assert_eq!(sorted[0], 0);
        assert_eq!(sorted[N - 1], N - 1);
    }
}
