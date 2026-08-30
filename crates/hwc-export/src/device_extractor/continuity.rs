//! Device Channel Continuity & Topology Analysis
//!
//! Strongly-typed graph-based physical continuity verification for multi-terminal
//! device channels (e.g. serpentine resistors, interdigitated capacitors, transistor channels).
//!
//! # Architecture
//! 1. **Element Extraction**: Pours and bridging contacts are lifted into strongly-typed `ChannelElement` nodes.
//! 2. **Topological Graph Construction**: 3D spatial intersections create adjacency edges between planar layers and vertical plugs.
//! 3. **Connected Component Solving**: Disjoint-set graph partitioning identifies continuous islands.
//! 4. **Conduction Path Verification**: Validates that all paired conduction terminals belong to the same component.

use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::space::{BindingPriority, ContactMetadata, PourMetadata};
use hwc_engine::HardwareSpace;
use rustc_hash::{FxHashMap, FxHashSet};

use super::error::DeviceExtractionError;

/// Strongly-typed unique identifier for a node in the channel graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChannelNodeId(pub usize);

/// Strongly-typed geometry element in a device channel assembly.
#[derive(Debug, Clone)]
pub enum ChannelElement {
    /// 2D planar pour on a single physical layer
    Pour {
        name: CompactString,
        material: CompactString,
        z_bottom_nm: i64,
        bbox: BoundingBox,
        priority: BindingPriority,
        terminals: Vec<CompactString>,
    },
    /// Vertical contact/via plug bridging layers
    Contact {
        name: CompactString,
        material: CompactString,
        z_start_nm: i64,
        z_end_nm: i64,
        bbox: BoundingBox,
    },
}

impl ChannelElement {
    /// Name of the element for diagnostics
    pub fn name(&self) -> &str {
        match self {
            Self::Pour { name, .. } => name.as_str(),
            Self::Contact { name, .. } => name.as_str(),
        }
    }

    /// Bounding box of the element
    pub fn bbox(&self) -> &BoundingBox {
        match self {
            Self::Pour { bbox, .. } => bbox,
            Self::Contact { bbox, .. } => bbox,
        }
    }

    /// Check if two channel elements physically intersect in 3D space
    pub fn intersects(&self, other: &ChannelElement) -> bool {
        match (self, other) {
            (Self::Pour { bbox: b1, .. }, Self::Pour { bbox: b2, .. }) => {
                let x_overlap = b1.min.x <= b2.max.x && b1.max.x >= b2.min.x;
                let y_overlap = b1.min.y <= b2.max.y && b1.max.y >= b2.min.y;
                let z_overlap = b1.min.z <= b2.max.z && b1.max.z >= b2.min.z;
                x_overlap && y_overlap && z_overlap
            }
            (
                Self::Pour {
                    bbox: p_bbox,
                    z_bottom_nm,
                    ..
                },
                Self::Contact {
                    bbox: c_bbox,
                    z_start_nm,
                    z_end_nm,
                    ..
                },
            )
            | (
                Self::Contact {
                    bbox: c_bbox,
                    z_start_nm,
                    z_end_nm,
                    ..
                },
                Self::Pour {
                    bbox: p_bbox,
                    z_bottom_nm,
                    ..
                },
            ) => {
                let x_overlap = c_bbox.min.x <= p_bbox.max.x && c_bbox.max.x >= p_bbox.min.x;
                let y_overlap = c_bbox.min.y <= p_bbox.max.y && c_bbox.max.y >= p_bbox.min.y;
                if !x_overlap || !y_overlap {
                    return false;
                }

                let c_z_min = c_bbox.min.z.min(*z_start_nm);
                let c_z_max = c_bbox.max.z.max(*z_end_nm);
                let p_z_min = p_bbox.min.z.min(*z_bottom_nm);
                let p_z_max = p_bbox.max.z;

                c_z_min <= p_z_max && c_z_max >= p_z_min
            }
            (
                Self::Contact {
                    bbox: c1,
                    z_start_nm: z1_start,
                    z_end_nm: z1_end,
                    ..
                },
                Self::Contact {
                    bbox: c2,
                    z_start_nm: z2_start,
                    z_end_nm: z2_end,
                    ..
                },
            ) => {
                let x_overlap = c1.min.x <= c2.max.x && c1.max.x >= c2.min.x;
                let y_overlap = c1.min.y <= c2.max.y && c1.max.y >= c2.min.y;
                if !x_overlap || !y_overlap {
                    return false;
                }
                let z1_min = c1.min.z.min(*z1_start);
                let z1_max = c1.max.z.max(*z1_end);
                let z2_min = c2.min.z.min(*z2_start);
                let z2_max = c2.max.z.max(*z2_end);
                z1_min <= z2_max && z1_max >= z2_min
            }
        }
    }
}

/// A connected topological island representing an unbroken physical component of the device channel.
#[derive(Debug, Clone)]
pub struct ChannelIsland {
    pub root_id: ChannelNodeId,
    pub element_names: Vec<CompactString>,
    pub bound_terminals: Vec<CompactString>,
}

/// Strongly-typed analysis report produced by the channel topology solver.
#[derive(Debug, Clone)]
pub struct ChannelContinuityReport {
    pub is_continuous: bool,
    pub conduction_terminals: Vec<CompactString>,
    pub islands: Vec<ChannelIsland>,
    pub disconnected_pairs: Vec<(CompactString, CompactString)>,
}

/// Strongly-typed Disjoint-Set Forest for connected component partitioning.
struct DisjointSetForest {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl DisjointSetForest {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, i: usize) -> usize {
        if self.parent[i] != i {
            self.parent[i] = self.find(self.parent[i]);
        }
        self.parent[i]
    }

    fn union(&mut self, i: usize, j: usize) {
        let root_i = self.find(i);
        let root_j = self.find(j);
        if root_i == root_j {
            return;
        }
        match self.rank[root_i].cmp(&self.rank[root_j]) {
            std::cmp::Ordering::Less => self.parent[root_i] = root_j,
            std::cmp::Ordering::Greater => self.parent[root_j] = root_i,
            std::cmp::Ordering::Equal => {
                self.parent[root_j] = root_i;
                self.rank[root_i] += 1;
            }
        }
    }
}

/// Topological graph representation of a device's physical channel geometry.
#[derive(Debug)]
pub struct DeviceChannelGraph {
    nodes: Vec<ChannelElement>,
    pour_count: usize,
}

impl DeviceChannelGraph {
    /// Construct a channel graph from the pours and bridging contacts associated with a device.
    pub fn build(
        pours: &[PourMetadata],
        space_contacts: &[ContactMetadata],
    ) -> Self {
        let mut nodes = Vec::new();
        let mut pour_count = 0;

        for pour in pours {
            if let Some(ref bbox) = pour.bbox {
                let (priority, terminals) = if let Some(ref b) = pour.device_binding {
                    (b.priority, b.terminals.clone())
                } else {
                    (BindingPriority::Contact, Vec::new())
                };

                nodes.push(ChannelElement::Pour {
                    name: pour.name.clone(),
                    material: pour.material_name.clone(),
                    z_bottom_nm: pour.z_bottom_nm,
                    bbox: *bbox,
                    priority,
                    terminals,
                });
                pour_count += 1;
            }
        }

        // Find contacts that intersect any of the device's pours
        for contact in space_contacts {
            if let Some(ref c_bbox) = contact.bbox {
                let contact_elem = ChannelElement::Contact {
                    name: contact.name.clone(),
                    material: contact.material_name.clone(),
                    z_start_nm: contact.z_start_nm,
                    z_end_nm: contact.z_end_nm,
                    bbox: *c_bbox,
                };

                let touches_pour = nodes[..pour_count]
                    .iter()
                    .any(|p| contact_elem.intersects(p));

                if touches_pour {
                    nodes.push(contact_elem);
                }
            }
        }

        Self { nodes, pour_count }
    }

    /// Analyze continuity between the specified conduction terminals.
    pub fn analyze(
        &self,
        conduction_terminals: &[CompactString],
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    ) -> ChannelContinuityReport {
        let n = self.nodes.len();
        let mut dsf = DisjointSetForest::new(n);

        // Build edges based on 3D spatial intersections
        for i in 0..n {
            for j in (i + 1)..n {
                if self.nodes[i].intersects(&self.nodes[j]) {
                    dsf.union(i, j);
                }
            }
        }

        // Map conduction terminals to their representative entry node indices in the graph
        let mut term_entry_nodes: FxHashMap<CompactString, Vec<usize>> = FxHashMap::default();
        for term in conduction_terminals {
            if let Some(pours_for_term) = terminal_pours.get(term) {
                // Priority 1: Contact-priority pours
                let contact_nodes: Vec<usize> = pours_for_term
                    .iter()
                    .filter(|p| {
                        p.device_binding
                            .as_ref()
                            .map_or(false, |b| b.priority == BindingPriority::Contact)
                    })
                    .filter_map(|p| self.nodes[..self.pour_count].iter().position(|node| node.name() == p.name.as_str()))
                    .collect();

                if !contact_nodes.is_empty() {
                    term_entry_nodes.insert(term.clone(), contact_nodes);
                } else {
                    // Fallback: All pours bound to this terminal
                    let all_nodes: Vec<usize> = pours_for_term
                        .iter()
                        .filter_map(|p| self.nodes[..self.pour_count].iter().position(|node| node.name() == p.name.as_str()))
                        .collect();
                    term_entry_nodes.insert(term.clone(), all_nodes);
                }
            }
        }

        // Check connectivity between all pairs of conduction terminals
        let mut disconnected_pairs = Vec::new();
        for i in 0..conduction_terminals.len() {
            for j in (i + 1)..conduction_terminals.len() {
                let term_a = &conduction_terminals[i];
                let term_b = &conduction_terminals[j];

                let nodes_a = term_entry_nodes.get(term_a).map(|v| v.as_slice()).unwrap_or(&[]);
                let nodes_b = term_entry_nodes.get(term_b).map(|v| v.as_slice()).unwrap_or(&[]);

                let mut connected = false;
                for &na in nodes_a {
                    for &nb in nodes_b {
                        if dsf.find(na) == dsf.find(nb) {
                            connected = true;
                            break;
                        }
                    }
                    if connected {
                        break;
                    }
                }

                if !connected {
                    disconnected_pairs.push((term_a.clone(), term_b.clone()));
                }
            }
        }

        // Aggregate connected islands
        let mut island_map: FxHashMap<usize, (Vec<CompactString>, FxHashSet<CompactString>)> =
            FxHashMap::default();

        for i in 0..self.pour_count {
            let root = dsf.find(i);
            let entry = island_map.entry(root).or_default();
            entry.0.push(self.nodes[i].name().into());

            if let ChannelElement::Pour { ref terminals, .. } = self.nodes[i] {
                for t in terminals {
                    entry.1.insert(t.clone());
                }
            }
        }

        let islands = island_map
            .into_iter()
            .map(|(root_id, (elements, terminals))| ChannelIsland {
                root_id: ChannelNodeId(root_id),
                element_names: elements,
                bound_terminals: terminals.into_iter().collect(),
            })
            .collect();

        ChannelContinuityReport {
            is_continuous: disconnected_pairs.is_empty(),
            conduction_terminals: conduction_terminals.to_vec(),
            islands,
            disconnected_pairs,
        }
    }
}

/// Topology Validator service for verifying device physical integrity.
pub struct DeviceTopologyValidator<'a> {
    space: &'a HardwareSpace,
}

impl<'a> DeviceTopologyValidator<'a> {
    pub fn new(space: &'a HardwareSpace) -> Self {
        Self { space }
    }

    /// Extract conduction terminals for a device from its pour metadata.
    ///
    /// A device opts into channel continuity verification if and only if it declares
    /// one or more channel body pours bound to multiple terminals (e.g., `device: R1.A, R1.B`).
    /// Electrostatic or isolated single-terminal plates (like Capacitor plates `device: C1.c0`,
    /// `device: C1.c1`) do not declare shared channel bodies and are not forced to conduct.
    pub fn extract_conduction_terminals(
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    ) -> Vec<CompactString> {
        let mut terminals: Vec<CompactString> = Vec::new();

        // Only pours explicitly declaring a multi-terminal channel body opt into continuity checking
        for pours in terminal_pours.values() {
            for pour in pours {
                if let Some(ref binding) = pour.device_binding {
                    if binding.priority == BindingPriority::Channel && binding.terminals.len() >= 2 {
                        for t in &binding.terminals {
                            if !terminals.contains(t) {
                                terminals.push(t.clone());
                            }
                        }
                    }
                }
            }
        }

        terminals
    }

    /// Validate channel topological continuity for a device.
    pub fn validate_channel_continuity(
        &self,
        device_name: &str,
        device_type: &str,
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    ) -> Result<ChannelContinuityReport, DeviceExtractionError> {
        let conduction_terminals = Self::extract_conduction_terminals(terminal_pours);

        if conduction_terminals.len() < 2 {
            return Ok(ChannelContinuityReport {
                is_continuous: true,
                conduction_terminals,
                islands: Vec::new(),
                disconnected_pairs: Vec::new(),
            });
        }

        // Collect all unique pours
        let mut all_pours: Vec<PourMetadata> = Vec::new();
        let mut seen = FxHashSet::default();
        for pours in terminal_pours.values() {
            for pour in pours {
                if seen.insert(&pour.name) {
                    all_pours.push(pour.clone());
                }
            }
        }

        // Construct graph and analyze
        let graph = DeviceChannelGraph::build(&all_pours, &self.space.contacts);
        let report = graph.analyze(&conduction_terminals, terminal_pours);

        if !report.is_continuous {
            let mut island_lines = Vec::new();
            for (idx, island) in report.islands.iter().enumerate() {
                let term_str = if island.bound_terminals.is_empty() {
                    String::new()
                } else {
                    format!(" [Terminals: {}]", island.bound_terminals.join(", "))
                };
                island_lines.push(format!(
                    "  • Island {}{}: {}",
                    idx + 1,
                    term_str,
                    island.element_names.join(", ")
                ));
            }

            let pair_strs: Vec<String> = report
                .disconnected_pairs
                .iter()
                .map(|(a, b)| format!("'{}' ↮ '{}'", a, b))
                .collect();

            return Err(DeviceExtractionError::InvalidGeometry {
                device_name: device_name.to_string().into(),
                device_type: device_type.to_string().into(),
                reason: format!(
                    "Device channel fragmentation: Physical geometry for device '{}' is disconnected.\n\
                     \n\
                     Disconnected Terminal Pairs: {}\n\
                     The channel geometry is fragmented into {} disjoint islands (open circuit).\n\
                     \n\
                     Disjoint Island Groups:\n\
                     {}\n\
                     \n\
                     How to fix:\n\
                     1. Check your loop bounds or segment placement in the layout\n\
                     2. Ensure every segment and connector in the channel physically overlaps its neighbors\n\
                     3. Verify that via bridges span between the channel layer and contact layers",
                    device_name,
                    pair_strs.join(", "),
                    report.islands.len(),
                    island_lines.join("\n")
                )
                .into(),
            });
        }

        
        Ok(report)
    }
}
