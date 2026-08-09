use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::connectivity::{SubstrateLayerMetadata, SubstrateLayerType};
use crate::geometry::{BoundingBox, Point3D};

use super::graph::ConnectivityGraph;
use super::types::{
    ConnectivityResult, FragmentationReport, FragmentedIsland, PlanarIsland, VerticalBridge,
};

/// Contact/via placement data for vertical bridge mapping.
#[derive(Debug, Clone)]
pub struct ContactPlacement {
    /// Contact name
    pub name: CompactString,
    /// X center coordinate
    pub x: i64,
    /// Y center coordinate
    pub y: i64,
    /// Z-min of the contact span
    pub z_min: i64,
    /// Z-max of the contact span
    pub z_max: i64,
    /// Net name
    pub net_name: Option<CompactString>,
    /// Material ID
    pub material: u8,
    /// Bounding box
    pub bbox: Option<BoundingBox>,
}

/// PIVB Solver: Topological Connectivity Verification.
///
/// Replaces coordinate-snapping with a graph-based approach using:
/// 1. Planar Island Extraction (nodes) from pre-welded contours
/// 2. Vertical Bridge Mapping (edges) from via/contact placements
/// 3. Connectivity Validation via Tarjan's Strongly Connected Components
///
/// Eliminates floating-point jitter and Z-depth sensitivity by operating
/// on topological structure rather than coordinate snapping.
pub struct PivbSolver<'a> {
    substrate_layers: &'a [SubstrateLayerMetadata],
    contacts: &'a [ContactPlacement],
    conductive_material_ids: &'a rustc_hash::FxHashSet<u8>,
}

impl<'a> PivbSolver<'a> {
    pub fn new(
        substrate_layers: &'a [SubstrateLayerMetadata],
        contacts: &'a [ContactPlacement],
        conductive_material_ids: &'a rustc_hash::FxHashSet<u8>,
    ) -> Self {
        Self {
            substrate_layers,
            contacts,
            conductive_material_ids,
        }
    }

    /// Run the full PIVB validation pipeline.
    ///
    /// Returns a list of connectivity results, one per net. Each result is either
    /// a Pass (single connected component) or Fail (fragmentation report).
    pub fn validate(&self) -> Vec<ConnectivityResult> {
        // Pass 1: Extract Planar Islands from substrate layers
        let islands = self.extract_planar_islands();

        // Group islands by net
        let nets = self.group_islands_by_net(&islands);

        // Pass 2: Extract vertical bridges from contacts
        let bridges = self.extract_vertical_bridges(&islands);

        // Pass 3: For each net, build graph and validate connectivity
        let mut results = Vec::new();

        for (net_name, net_islands) in &nets {
            let net_bridges: Vec<&VerticalBridge> = bridges
                .iter()
                .filter(|b| {
                    net_islands.iter().any(|i| i.id == b.island_a)
                        || net_islands.iter().any(|i| i.id == b.island_b)
                })
                .collect();

            let result = self.validate_net(net_name, net_islands, &net_bridges);
            results.push(result);
        }

        results
    }

    /// Pass 1: Extract Planar Islands from substrate layers.
    ///
    /// Each substrate layer with a net assignment and conductive material becomes
    /// a Planar Island node. Layers without net assignments or with non-conductive
    /// materials are skipped.
    ///
    /// Overlapping same-net geometry is welded (unioned) into single islands,
    /// simulating the Boolean Union that the Geometry Refinement Engine performs
    /// on pre-welded 2D contours.
    ///
    /// v0.2.1: Device terminal bindings are preserved during island extraction.
    fn extract_planar_islands(&self) -> Vec<PlanarIsland> {
        let mut raw_islands: Vec<PlanarIsland> = Vec::new();
        let mut island_id = 0;

        for layer in self.substrate_layers {
            let net_name = match &layer.net_name {
                Some(name) => name.clone(),
                None => continue,
            };

            if !self.conductive_material_ids.contains(&layer.material) {
                eprintln!(
                    "[PIVB ISLAND DEBUG] Skipping layer with non-conductive material {} (net={})",
                    layer.material, net_name
                );
                continue;
            }

            let bbox = layer.bbox;
            let center = bbox.center();

            raw_islands.push(PlanarIsland {
                id: island_id,
                layer_name: self.layer_type_to_name(layer),
                z_min: bbox.min.z,
                z_max: bbox.max.z,
                boundary: bbox,
                bbox,
                center,
                net_name,
                net_id: layer.net,
                material: layer.material,
                device_binding: layer.device_binding.clone(),
            });

            island_id += 1;
        }

        // Weld overlapping same-net islands into single Planar Islands.
        // This simulates the Boolean Union (clipper2-rust) that the
        // Geometry Refinement Engine performs before the PIVB solver runs.
        self.weld_islands(raw_islands)
    }

    /// Weld overlapping islands with the same net into single Planar Islands.
    ///
    /// Uses a union-find approach: two islands merge if they share the same net,
    /// same material, and their bounding boxes overlap in 3D space.
    fn weld_islands(&self, islands: Vec<PlanarIsland>) -> Vec<PlanarIsland> {
        if islands.len() <= 1 {
            return islands;
        }

        let n = islands.len();
        let mut parent: Vec<usize> = (0..n).collect();

        fn find(parent: &mut [usize], x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }

        // Union overlapping same-net, same-material islands
        for i in 0..n {
            for j in (i + 1)..n {
                let same_net = islands[i].net_name == islands[j].net_name;
                let same_material = islands[i].material == islands[j].material;

                // v0.2.1: MATERIAL COMPATIBILITY CHECK
                // Routes may have incorrect material IDs but should still weld with device terminals
                // if they're on the same Z-plane. This is a workaround for routing engine material assignment.
                let z_compatible =
                    islands[i].z_min <= islands[j].z_max && islands[i].z_max >= islands[j].z_min;
                let compatible = same_material
                    || (same_net
                        && z_compatible
                        && (islands[i].device_binding.is_some()
                            || islands[j].device_binding.is_some()));

                if compatible && same_net && self.islands_overlap_3d(&islands[i], &islands[j]) {
                    let rx = find(&mut parent, i);
                    let ry = find(&mut parent, j);
                    if rx != ry {
                        parent[rx] = ry;
                    }
                }
            }
        }

        // Group islands by root
        let mut groups: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
        for i in 0..n {
            let root = find(&mut parent, i);
            groups.entry(root).or_default().push(i);
        }

        // Merge groups into single Planar Islands
        let mut welded = Vec::new();

        for (welded_id, (_root, member_indices)) in groups.into_iter().enumerate() {
            let first = &islands[member_indices[0]];
            let mut merged_bbox = first.bbox;
            let mut min_z = first.z_min;
            let mut max_z = first.z_max;
            let mut center_sum_x = first.center.x;
            let mut center_sum_y = first.center.y;
            let mut center_sum_z = first.center.z;
            let count = member_indices.len();

            for &idx in &member_indices[1..] {
                let island = &islands[idx];
                merged_bbox = merged_bbox.union(&island.bbox);
                min_z = min_z.min(island.z_min);
                max_z = max_z.max(island.z_max);
                center_sum_x += island.center.x;
                center_sum_y += island.center.y;
                center_sum_z += island.center.z;
            }

            let center = if count > 0 {
                Point3D::new(
                    center_sum_x / count as i64,
                    center_sum_y / count as i64,
                    center_sum_z / count as i64,
                )
            } else {
                merged_bbox.center()
            };

            welded.push(PlanarIsland {
                id: welded_id,
                layer_name: first.layer_name.clone(),
                z_min: min_z,
                z_max: max_z,
                boundary: merged_bbox,
                bbox: merged_bbox,
                center,
                net_name: first.net_name.clone(),
                net_id: first.net_id,
                material: first.material,
                device_binding: first.device_binding.clone(),
            });
        }

        welded
    }

    /// Check if two islands overlap in 3D space (XY overlap + Z range overlap).
    /// Uses inclusive checks (<= and >=) because touching boundaries are conductive.
    fn islands_overlap_3d(&self, a: &PlanarIsland, b: &PlanarIsland) -> bool {
        let xy_overlap = a.bbox.min.x <= b.bbox.max.x
            && a.bbox.max.x >= b.bbox.min.x
            && a.bbox.min.y <= b.bbox.max.y
            && a.bbox.max.y >= b.bbox.min.y;

        let z_overlap = a.z_min <= b.z_max && a.z_max >= b.z_min;

        xy_overlap && z_overlap
    }

    /// Map SubstrateLayerType to a human-readable layer name.
    fn layer_type_to_name(&self, layer: &SubstrateLayerMetadata) -> CompactString {
        match layer.layer_type {
            SubstrateLayerType::Pour => "copper_pour".into(),
            SubstrateLayerType::Contact => "contact".into(),
            SubstrateLayerType::Substrate => "substrate".into(),
        }
    }

    /// Group islands by net name.
    fn group_islands_by_net<'b>(
        &self,
        islands: &'b [PlanarIsland],
    ) -> FxHashMap<CompactString, Vec<&'b PlanarIsland>> {
        let mut nets: FxHashMap<CompactString, Vec<&PlanarIsland>> = FxHashMap::default();
        for island in islands {
            nets.entry(island.net_name.clone())
                .or_default()
                .push(island);
        }
        nets
    }

    /// Pass 2: Extract Vertical Bridges from contact placements.
    ///
    /// For each contact, check which Planar Islands the contact's Z-span overlaps
    /// with (same net, XY containment). A via from z=0 to z=300 bridges an island
    /// at z=0-100 and an island at z=200-300. This creates graph edges between
    /// the connected islands.
    fn extract_vertical_bridges(&self, islands: &[PlanarIsland]) -> Vec<VerticalBridge> {
        let mut bridges = Vec::new();
        let mut bridge_id = 0;

        for contact in self.contacts {
            let contact_net = match &contact.net_name {
                Some(name) => name.clone(),
                None => continue,
            };

            // Find islands that this contact bridges:
            // - Same net name
            // - XY containment: contact center (x,y) falls within island's XY bounds
            // - Z span overlap: contact's Z range overlaps with island's Z range
            let matching_islands: Vec<&PlanarIsland> = islands
                .iter()
                .filter(|i| {
                    i.net_name == contact_net
                        && self.contact_xy_overlaps_island(contact, i)
                        && self.contact_z_overlaps_island(contact, i)
                })
                .collect();

            // Sort by Z to identify layer A (lower) and layer B (upper)
            let mut sorted = matching_islands;
            sorted.sort_by_key(|i| i.z_min);

            // Create bridges between adjacent Z-layer pairs
            for window in sorted.windows(2) {
                let island_a = window[0];
                let island_b = window[1];

                bridges.push(VerticalBridge {
                    id: bridge_id,
                    island_a: island_a.id,
                    island_b: island_b.id,
                    x: contact.x,
                    y: contact.y,
                    z_min: contact.z_min,
                    z_max: contact.z_max,
                });

                bridge_id += 1;
            }
        }

        bridges
    }

    /// Check if a contact overlaps an island in XY.
    /// Uses contact bbox overlap if available, otherwise falls back to center point check.
    fn contact_xy_overlaps_island(
        &self,
        contact: &ContactPlacement,
        island: &PlanarIsland,
    ) -> bool {
        if let Some(c_bbox) = &contact.bbox {
            // XY overlap check
            c_bbox.min.x <= island.bbox.max.x
                && c_bbox.max.x >= island.bbox.min.x
                && c_bbox.min.y <= island.bbox.max.y
                && c_bbox.max.y >= island.bbox.min.y
        } else {
            // Fallback to center point check
            contact.x >= island.bbox.min.x
                && contact.x <= island.bbox.max.x
                && contact.y >= island.bbox.min.y
                && contact.y <= island.bbox.max.y
        }
    }

    /// Check if a contact's Z span overlaps with an island's Z range.
    ///
    /// Uses inclusive checks (<= and >=) because contacts usually touch
    /// the top/bottom surface of conductive layers.
    fn contact_z_overlaps_island(&self, contact: &ContactPlacement, island: &PlanarIsland) -> bool {
        contact.z_min <= island.z_max && contact.z_max >= island.z_min
    }

    /// Pass 3: Validate connectivity for a single net.
    ///
    /// Builds a connectivity graph from the net's islands and bridges,
    /// then checks if the graph has exactly one connected component.
    ///
    /// v0.2.1: Device-aware connectivity validation.
    /// Device terminal islands are treated as "connectivity anchors" that
    /// provide implicit connectivity across their entire surface. Other islands
    /// (pads, routes) that physically overlap with device terminals are considered
    /// connected to the device.
    fn validate_net(
        &self,
        net_name: &CompactString,
        islands: &[&PlanarIsland],
        bridges: &[&VerticalBridge],
    ) -> ConnectivityResult {
        // Single island is always connected
        if islands.len() <= 1 {
            return ConnectivityResult::Pass {
                net_name: net_name.clone(),
                island_count: islands.len(),
                bridge_count: bridges.len(),
            };
        }

        // Build graph
        let mut graph = ConnectivityGraph::new();

        // Add all islands as nodes
        for island in islands {
            graph.add_node((*island).clone());
        }

        // Add bridges as edges
        // Map island_id -> graph_node_index
        let id_to_idx: FxHashMap<usize, usize> = islands
            .iter()
            .enumerate()
            .map(|(idx, i)| (i.id, idx))
            .collect();

        for bridge in bridges {
            if let (Some(&u), Some(&v)) = (
                id_to_idx.get(&bridge.island_a),
                id_to_idx.get(&bridge.island_b),
            ) {
                graph.add_edge(u, v);
            }
        }

        // v0.2.1: Device-aware connectivity bridging
        // Add implicit edges between device terminal islands and other islands that physically overlap
        let device_islands: Vec<(usize, &PlanarIsland)> = islands
            .iter()
            .enumerate()
            .filter(|(_, island)| island.device_binding.is_some())
            .map(|(idx, island)| (idx, *island))
            .collect();

        let non_device_islands: Vec<(usize, &PlanarIsland)> = islands
            .iter()
            .enumerate()
            .filter(|(_, island)| island.device_binding.is_none())
            .map(|(idx, island)| (idx, *island))
            .collect();

        eprintln!(
            "[PIVB DEVICE DEBUG] Net '{}': {} device islands, {} non-device islands",
            net_name,
            device_islands.len(),
            non_device_islands.len()
        );

        // Connect non-device islands to device terminals if they physically overlap
        for (device_idx, device_island) in &device_islands {
            eprintln!(
                "[PIVB DEVICE DEBUG]   Device island {}: {:?} bbox=({},{},{}) -> ({},{},{})",
                device_idx,
                device_island.device_binding,
                device_island.bbox.min.x,
                device_island.bbox.min.y,
                device_island.bbox.min.z,
                device_island.bbox.max.x,
                device_island.bbox.max.y,
                device_island.bbox.max.z
            );

            for (non_device_idx, non_device_island) in &non_device_islands {
                let overlaps = self.islands_overlap_3d(device_island, non_device_island);
                eprintln!("[PIVB DEVICE DEBUG]     Non-device island {}: bbox=({},{},{}) -> ({},{},{}) overlaps={}",
                    non_device_idx,
                    non_device_island.bbox.min.x, non_device_island.bbox.min.y, non_device_island.bbox.min.z,
                    non_device_island.bbox.max.x, non_device_island.bbox.max.y, non_device_island.bbox.max.z,
                    overlaps);

                if overlaps {
                    eprintln!("[PIVB DEVICE DEBUG]       -> Adding implicit edge!");
                    graph.add_edge(*device_idx, *non_device_idx);
                }
            }
        }

        // Check connected components using Tarjan's SCC
        let sccs = graph.tarjan_scc();

        if sccs.len() == 1 {
            ConnectivityResult::Pass {
                net_name: net_name.clone(),
                island_count: islands.len(),
                bridge_count: bridges.len(),
            }
        } else {
            let report = self.generate_fragmentation_report(net_name, &graph, &sccs);
            ConnectivityResult::Fail(report)
        }
    }

    /// Generate a diagnostic report for a fragmented net.
    ///
    /// Provides structured island-level diagnostics including:
    /// - Number of disconnected components
    /// - Representative bounding boxes for each component
    /// - Layer information for each component
    /// - Center coordinates for viewport focus
    /// - Suggested fix for bridging the gap
    fn generate_fragmentation_report(
        &self,
        net_name: &CompactString,
        graph: &ConnectivityGraph,
        sccs: &[Vec<usize>],
    ) -> FragmentationReport {
        let mut fragmented_islands = Vec::new();

        for (idx, component) in sccs.iter().enumerate() {
            let mut comp_bbox = graph.node(component[0]).bbox;
            let mut layers: Vec<CompactString> = Vec::new();
            let mut total_center = Point3D::new(0, 0, 0);
            let mut count = 0;

            for &node_idx in component {
                let island = graph.node(node_idx);
                comp_bbox = comp_bbox.union(&island.bbox);
                total_center = Point3D::new(
                    total_center.x + island.center.x,
                    total_center.y + island.center.y,
                    total_center.z + island.center.z,
                );
                count += 1;

                if !layers.contains(&island.layer_name) {
                    layers.push(island.layer_name.clone());
                }
            }

            let center = if count > 0 {
                Point3D::new(
                    total_center.x / count,
                    total_center.y / count,
                    total_center.z / count,
                )
            } else {
                comp_bbox.center()
            };

            fragmented_islands.push(FragmentedIsland {
                group_index: idx,
                island_count: component.len(),
                bbox: comp_bbox,
                center,
                layers,
            });
        }

        // Generate smart suggestion based on gap analysis
        let suggested_fix = if fragmented_islands.len() >= 2 {
            let island_a = &fragmented_islands[0];
            let island_b = &fragmented_islands[1];

            let z_gap = if island_a.bbox.max.z < island_b.bbox.min.z {
                island_b.bbox.min.z - island_a.bbox.max.z
            } else if island_b.bbox.max.z < island_a.bbox.min.z {
                island_a.bbox.min.z - island_b.bbox.max.z
            } else {
                0
            };

            if z_gap > 0 {
                format!(
                    "Z-layer gap detected: {} nm between components on net '{}'.\n    \
                     Component 1 is at z:{}-{}, Component 2 is at z:{}-{}.\n    \
                     Suggested fix: Add a via or bridge to connect layers on net '{}'.",
                    z_gap,
                    net_name,
                    island_a.bbox.min.z,
                    island_a.bbox.max.z,
                    island_b.bbox.min.z,
                    island_b.bbox.max.z,
                    net_name
                )
                .into()
            } else {
                let x_gap = if island_a.bbox.max.x < island_b.bbox.min.x {
                    island_b.bbox.min.x - island_a.bbox.max.x
                } else if island_b.bbox.max.x < island_a.bbox.min.x {
                    island_a.bbox.min.x - island_b.bbox.max.x
                } else {
                    0
                };

                let y_gap = if island_a.bbox.max.y < island_b.bbox.min.y {
                    island_b.bbox.min.y - island_a.bbox.max.y
                } else if island_b.bbox.max.y < island_a.bbox.min.y {
                    island_a.bbox.min.y - island_b.bbox.max.y
                } else {
                    0
                };

                format!(
                    "XY-plane gap detected between components on net '{}'.\n    \
                     X-gap: {} nm, Y-gap: {} nm.\n    \
                     Suggested fix: Add a pour or route to bridge the gap on net '{}'.",
                    net_name, x_gap, y_gap, net_name
                )
                .into()
            }
        } else {
            format!(
                "Net '{}' is fragmented into {} disconnected components.\n    \
                 Suggested fix: Verify all conductive segments are physically touching.",
                net_name,
                fragmented_islands.len()
            )
            .into()
        };

        FragmentationReport {
            net_name: net_name.clone(),
            component_count: sccs.len(),
            islands: fragmented_islands,
            suggested_fix,
        }
    }
}
