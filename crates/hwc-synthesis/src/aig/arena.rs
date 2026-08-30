// crates/hwc-synthesis/src/aig/arena.rs

use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// 32-bit edge encoding a node index and an inversion bit.
///
/// Bit 0: Inversion flag (1 = inverted, 0 = non-inverted)
/// Bits 1..31: Node ID index in `PackedAigGraph::nodes`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge(pub u32);

impl Edge {
    /// Constant Zero edge (points to node 0, non-inverted).
    pub const ZERO: Edge = Edge(0);

    /// Constant One edge (points to node 0, inverted).
    pub const ONE: Edge = Edge(1);

    #[inline(always)]
    pub fn new(node_id: u32, inverted: bool) -> Self {
        Edge((node_id << 1) | (u32::from(inverted) & 1))
    }

    #[inline(always)]
    pub fn node(self) -> u32 {
        self.0 >> 1
    }

    #[inline(always)]
    pub fn is_inverted(self) -> bool {
        (self.0 & 1) != 0
    }

    #[inline(always)]
    pub fn not(self) -> Self {
        Edge(self.0 ^ 1)
    }
}

/// Sequential D-type Flip-Flop record within the AIG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequentialDff {
    pub name: CompactString,
    pub d_input: Edge,
    pub q_output_node: u32,
    pub clock_signal: CompactString,
    pub reset_signal: Option<CompactString>,
    pub reset_value: bool,
}

/// Contiguous flat AIG arena storing logic gates in flat `Vec<u64>` (8 bytes per node, 0 heap pointer chasing).
#[derive(Debug, Clone)]
pub struct PackedAigGraph {
    /// Node 0: Constant Zero (0u64)
    /// Node 1..N: Primary Inputs / DFF Q Outputs (fanin0=0, fanin1=0)
    /// Node N..M: 2-Input AND Gates (fanin0 packed in low 32 bits, fanin1 in high 32 bits)
    pub nodes: Vec<u64>,
    /// Primary input signal names in declaration order
    pub input_names: Vec<CompactString>,
    /// Exact Node IDs for each primary input
    pub input_nodes: Vec<u32>,
    /// Primary output edges indexed by signal name
    pub outputs: FxHashMap<CompactString, Edge>,
    /// Sequential flip-flop register records
    pub registers: Vec<SequentialDff>,
    /// Structural hashing map from packed (fanin0, fanin1) -> Node ID for O(1) canonical deduplication
    strash: FxHashMap<u64, u32>,
}

impl Default for PackedAigGraph {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl PackedAigGraph {
    /// Create a new AIG arena with pre-allocated capacity.
    pub fn with_capacity(node_count: usize) -> Self {
        let mut graph = Self {
            nodes: Vec::with_capacity(node_count.max(1)),
            input_names: Vec::new(),
            input_nodes: Vec::new(),
            outputs: FxHashMap::default(),
            registers: Vec::new(),
            strash: FxHashMap::default(),
        };
        // Reserve Constant 0 at index 0
        graph.nodes.push(0);
        graph
    }

    /// Number of nodes currently stored in the arena.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the arena only contains the constant node.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    /// Register a primary input signal.
    #[inline]
    pub fn add_input(&mut self, name: &str) -> Edge {
        let node_id = self.nodes.len() as u32;
        self.nodes.push(0); // Input nodes store 0
        self.input_names.push(CompactString::new(name));
        self.input_nodes.push(node_id);
        Edge::new(node_id, false)
    }

    /// Look up the node ID for a named primary input.
    pub fn get_input_node(&self, name: &str) -> Option<u32> {
        for (n, &id) in self.input_names.iter().zip(&self.input_nodes) {
            if n.as_str() == name {
                return Some(id);
            }
        }
        None
    }

    /// Register a sequential DFF register.
    pub fn add_dff(
        &mut self,
        name: &str,
        d_input: Edge,
        clock_signal: &str,
        reset_signal: Option<&str>,
        reset_value: bool,
    ) -> Edge {
        let q_node_id = self.nodes.len() as u32;
        self.nodes.push(0); // DFF Q output acts as a pseudo-input node in combinational cones
        let dff = SequentialDff {
            name: CompactString::new(name),
            d_input,
            q_output_node: q_node_id,
            clock_signal: CompactString::new(clock_signal),
            reset_signal: reset_signal.map(CompactString::new),
            reset_value,
        };
        self.registers.push(dff);
        Edge::new(q_node_id, false)
    }

    /// Appends a 2-input AND gate with instant algebraic constant folding and structural hashing.
    #[inline]
    pub fn add_and(&mut self, mut e0: Edge, mut e1: Edge) -> Edge {
        // 1. Trivial Algebraic Constant Folding
        if e0.0 == Edge::ZERO.0 || e1.0 == Edge::ZERO.0 {
            return Edge::ZERO; // 0 AND x = 0
        }
        if e0.0 == Edge::ONE.0 {
            return e1; // 1 AND x = x
        }
        if e1.0 == Edge::ONE.0 {
            return e0; // x AND 1 = x
        }
        if e0.0 == e1.0 {
            return e0; // x AND x = x
        }
        if e0.0 == (e1.0 ^ 1) {
            return Edge::ZERO; // x AND (NOT x) = 0
        }

        // 2. Canonical Ordering (Smaller Edge ID first for structural hashing)
        if e0.0 > e1.0 {
            std::mem::swap(&mut e0, &mut e1);
        }

        let packed = (u64::from(e1.0) << 32) | u64::from(e0.0);

        // 3. Structural Hashing lookup
        if let Some(&existing_node) = self.strash.get(&packed) {
            return Edge::new(existing_node, false);
        }

        let node_id = self.nodes.len() as u32;
        self.nodes.push(packed);
        self.strash.insert(packed, node_id);
        Edge::new(node_id, false)
    }

    /// Constructs an OR gate: A OR B = NOT(NOT A AND NOT B)
    #[inline(always)]
    pub fn add_or(&mut self, e0: Edge, e1: Edge) -> Edge {
        self.add_and(e0.not(), e1.not()).not()
    }

    /// Constructs a XOR gate: A XOR B = (A AND NOT B) OR (NOT A AND B)
    #[inline]
    pub fn add_xor(&mut self, e0: Edge, e1: Edge) -> Edge {
        if e0 == e1 {
            return Edge::ZERO;
        }
        if e0 == e1.not() {
            return Edge::ONE;
        }
        if e0 == Edge::ZERO {
            return e1;
        }
        if e1 == Edge::ZERO {
            return e0;
        }
        if e0 == Edge::ONE {
            return e1.not();
        }
        if e1 == Edge::ONE {
            return e0.not();
        }

        let a = self.add_and(e0, e1.not());
        let b = self.add_and(e0.not(), e1);
        self.add_or(a, b)
    }

    /// Constructs a 2-to-1 Multiplexer: MUX(cond, then_e, else_e) = (cond AND then_e) OR (NOT cond AND else_e)
    #[inline]
    pub fn add_mux(&mut self, cond: Edge, then_e: Edge, else_e: Edge) -> Edge {
        if cond == Edge::ONE {
            return then_e;
        }
        if cond == Edge::ZERO {
            return else_e;
        }
        if then_e == else_e {
            return then_e;
        }
        let a = self.add_and(cond, then_e);
        let b = self.add_and(cond.not(), else_e);
        self.add_or(a, b)
    }

    /// Set an output edge for a named primary output.
    pub fn set_output(&mut self, name: &str, edge: Edge) {
        self.outputs.insert(CompactString::new(name), edge);
    }

    /// Get fanins for an AND node (returns (fanin0, fanin1)).
    #[inline(always)]
    pub fn get_fanins(&self, node_id: u32) -> (Edge, Edge) {
        let packed = self.nodes[node_id as usize];
        let e0 = Edge(packed as u32);
        let e1 = Edge((packed >> 32) as u32);
        (e0, e1)
    }

    /// Check if a node is an internal 2-input AND gate.
    #[inline(always)]
    pub fn is_and(&self, node_id: u32) -> bool {
        node_id > 0 && self.nodes[node_id as usize] != 0
    }
}
