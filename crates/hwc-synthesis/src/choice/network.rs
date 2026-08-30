// crates/hwc-synthesis/src/choice/network.rs

use crate::aig::arena::{Edge, PackedAigGraph};
use rustc_hash::FxHashMap;

/// Structural Choice Network preserving topologically distinct representations of functionally equivalent logic cones.
#[derive(Debug, Clone, Default)]
pub struct ChoiceNetwork {
    /// Base AIG graph
    pub graph: PackedAigGraph,
    /// Equivalence class mapping: Node ID -> list of alternative representative Edges
    pub choices: FxHashMap<u32, Vec<Edge>>,
}

impl ChoiceNetwork {
    /// Create a choice network from a base AIG graph.
    pub fn from_aig(graph: PackedAigGraph) -> Self {
        Self {
            graph,
            choices: FxHashMap::default(),
        }
    }

    /// Add an alternative structural representation for a node.
    pub fn add_choice(&mut self, node_id: u32, alt_edge: Edge) {
        if alt_edge.node() != node_id {
            self.choices.entry(node_id).or_default().push(alt_edge);
        }
    }

    /// Retrieve all structural choices for a given node (including itself).
    pub fn get_choices(&self, node_id: u32) -> Vec<Edge> {
        let mut result = vec![Edge::new(node_id, false)];
        if let Some(alts) = self.choices.get(&node_id) {
            result.extend(alts.iter().copied());
        }
        result
    }
}
