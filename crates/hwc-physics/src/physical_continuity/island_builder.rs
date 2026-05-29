use super::spatial_grid::SpatialGrid;
use super::types::*;
use crate::connectivity::{BoundingBox, ContactMetadata, PourMetadata, SubstrateLayerMetadata};
use rustc_hash::FxHashSet;

/// Island builder - constructs conductive islands using flood-fill.
pub struct IslandBuilder<'a> {
    _voxel_size_z_nm: i64,
    pours: &'a [PourMetadata],
    contacts: &'a [ContactMetadata],
    substrate_layers: &'a [SubstrateLayerMetadata],
    bridge_rules: &'a [crate::BridgeRule],
    material_mapping: &'a rustc_hash::FxHashMap<compact_str::CompactString, u8>,
}

impl<'a> IslandBuilder<'a> {
    pub fn new(
        voxel_size_z_nm: i64,
        pours: &'a [PourMetadata],
        contacts: &'a [ContactMetadata],
        substrate_layers: &'a [SubstrateLayerMetadata],
        bridge_rules: &'a [crate::BridgeRule],
        material_mapping: &'a rustc_hash::FxHashMap<compact_str::CompactString, u8>,
    ) -> Self {
        Self {
            _voxel_size_z_nm: voxel_size_z_nm,
            pours,
            contacts,
            substrate_layers,
            bridge_rules,
            material_mapping,
        }
    }

    /// Build conductive islands using flood-fill algorithm.
    ///
    /// This is the core of Layer 3 validation. It groups all physically-connected
    /// conductive geometry into "islands" regardless of net labels.
    ///
    /// # Algorithm
    /// 1. Create a unified list of all geometry nodes (pours, contacts, substrate layers)
    /// 2. Build a spatial grid index for O(1) neighbor lookups
    /// 3. For each unvisited node:
    ///    a. Start a new island
    ///    b. Flood-fill to all touching nodes (same material only) using spatial grid
    ///    c. Mark all visited nodes as part of this island
    /// 4. For each island, find all pins that touch it
    ///
    /// # Performance
    /// - O(N) with spatial grid indexing (optimized)
    /// - O(N²) worst case without spatial indexing (fallback)
    /// - Typical: <1ms for 1000 nodes
    pub fn build_islands(&self, pin_positions: Option<&[PinPosition]>) -> Vec<ConductiveIsland> {
        let mut islands = Vec::new();
        let mut visited = FxHashSet::default();
        let mut island_id = 0;

        // Create unified node list with their bounding boxes and materials
        let all_nodes = self.collect_all_geometry_nodes();

        // println!($3"[DEBUG PHYSICAL CONTINUITY] Starting island building with {} total nodes",
        //  all_nodes.len()
        //  );

        // Build spatial grid index for O(1) neighbor lookups
        let spatial_grid = SpatialGrid::build(&all_nodes);

        // Flood-fill to build islands
        for (idx, _node) in all_nodes.iter().enumerate() {
            if visited.contains(&idx) {
                continue;
            }

            // Start new island
            let mut island_nodes = Vec::new();
            let mut stack = vec![idx];
            let material = self.get_node_material(&all_nodes[idx].0);

            // println!($3"[DEBUG PHYSICAL CONTINUITY] Starting island {} from node {}",
            //     island_id, idx
            // );

            // Flood-fill using spatial grid
            while let Some(curr_idx) = stack.pop() {
                if visited.contains(&curr_idx) {
                    continue;
                }

                visited.insert(curr_idx);
                island_nodes.push(all_nodes[curr_idx].0);

                // Find all touching neighbors with same material using spatial grid
                let neighbors = self.find_touching_neighbors(
                    curr_idx,
                    &all_nodes,
                    &spatial_grid,
                    material,
                    &visited,
                );

                for neighbor_idx in neighbors {
                    stack.push(neighbor_idx);
                }
            }

            // Create island
            let bbox = self.compute_island_bbox(&island_nodes);

            // Find pins that touch this island (if pin positions are provided)
            let pins = if let Some(pin_pos_list) = pin_positions {
                self.find_pins_touching_bbox(&bbox, pin_pos_list)
            } else {
                Vec::new()
            };

            // println!($3"[DEBUG PHYSICAL CONTINUITY] Island {} has {} nodes and {} pins",
            //    island_id,
            //    island_nodes.len(),
            //     pins.len()
            //  );

            islands.push(ConductiveIsland {
                id: island_id,
                nodes: island_nodes,
                bbox,
                material,
                pins,
            });

            island_id += 1;
        }

        // println!($3"[DEBUG PHYSICAL CONTINUITY] Built {} islands total",
        //    islands.len()
        //   );

        islands
    }

    /// Find all neighbors that touch the given node using spatial grid.
    ///
    /// Returns indices of nodes that:
    /// 1. Are in the same or adjacent grid cells
    /// 2. Have the same material
    /// 3. Actually touch (bbox intersection check)
    /// 4. Haven't been visited yet
    fn find_touching_neighbors(
        &self,
        curr_idx: usize,
        all_nodes: &[(GeometryNodeRef, BoundingBox)],
        spatial_grid: &SpatialGrid,
        material: u8,
        visited: &FxHashSet<usize>,
    ) -> Vec<usize> {
        let mut neighbors = Vec::new();
        let curr_bbox = &all_nodes[curr_idx].1;

        // Get candidates from spatial grid
        let candidates = spatial_grid.get_candidates(curr_bbox);

        // Check each candidate for actual touching and material match
        for neighbor_idx in candidates {
            if neighbor_idx == curr_idx || visited.contains(&neighbor_idx) {
                continue;
            }

            let neighbor = &all_nodes[neighbor_idx];
            let neighbor_material = self.get_node_material(&neighbor.0);

            // Material check: v0.1.7: Allow connection if materials match OR if a bridge exists
            let materials_can_connect = if neighbor_material == material {
                true
            } else {
                // Check bridge rules
                let curr_mat_name = self.get_material_name_from_id(material);
                let neighbor_mat_name = self.get_material_name_from_id(neighbor_material);

                if let (Some(m1), Some(m2)) = (curr_mat_name, neighbor_mat_name) {
                    self.bridge_rules.iter().any(|r| {
                        (r.from_material == m1 && r.to_material == m2)
                            || (r.from_material == m2 && r.to_material == m1)
                    })
                } else {
                    false
                }
            };

            if !materials_can_connect {
                continue;
            }

            if self.nodes_touch(curr_bbox, &neighbor.1) {
                neighbors.push(neighbor_idx);
            }
        }

        neighbors
    }

    /// Helper to get material name from ID using the mapping.
    fn get_material_name_from_id(&self, id: u8) -> Option<compact_str::CompactString> {
        for (name, &mat_id) in self.material_mapping.iter() {
            if mat_id == id {
                return Some(name.clone());
            }
        }
        None
    }

    /// Collect all geometry nodes into a unified list.
    ///
    /// Returns a vector of (GeometryNodeRef, BoundingBox) tuples for efficient
    /// flood-fill processing.
    ///
    /// NOTE: We only use substrate layers, not pours/contacts directly, because
    /// substrate layers represent the actual voxel geometry. Pours and contacts
    /// are compiled into substrate layers, so using both would create duplicates.
    fn collect_all_geometry_nodes(&self) -> Vec<(GeometryNodeRef, BoundingBox)> {
        let mut nodes = Vec::new();

        // Add substrate layers - these represent the actual voxel geometry
        for (idx, _layer) in self.substrate_layers.iter().enumerate() {
            nodes.push((GeometryNodeRef::SubstrateLayer(idx), self.substrate_layers[idx].bbox.clone()));
        }

        nodes
    }

    /// Get the material ID for a geometry node.
    fn get_node_material(&self, node: &GeometryNodeRef) -> u8 {
        match node {
            GeometryNodeRef::Pour(idx) => {
                // v0.1.7: Look up actual material from pour metadata
                let name = &self.pours[*idx].material_name;
                *self.material_mapping.get(name).unwrap_or(&2) // Default to Copper (2)
            }
            GeometryNodeRef::Contact(idx) => {
                // v0.1.7: Look up actual material from contact metadata
                let name = &self.contacts[*idx].material_name;
                *self.material_mapping.get(name).unwrap_or(&2) // Default to Copper (2)
            }
            GeometryNodeRef::SubstrateLayer(idx) => self.substrate_layers[*idx].material,
        }
    }

    /// Check if two bounding boxes physically touch.
    ///
    /// This uses the same logic as connectivity.rs but operates on BoundingBox directly.
    /// Two boxes touch if:
    /// 1. Their XY projections overlap
    /// 2. They either share Z space (volume overlap) OR their Z faces touch exactly
    fn nodes_touch(&self, a: &BoundingBox, b: &BoundingBox) -> bool {
        // Step 1: Check XY plane overlap (Inclusive - touching at edges is a connection)
        let x_overlap = a.min_x <= b.max_x && a.max_x >= b.min_x;
        let y_overlap = a.min_y <= b.max_y && a.max_y >= b.min_y;

        if !x_overlap || !y_overlap {
            return false;
        }

        // Step 2: Check Z-axis contact
        // A) Volume Overlap: They share the same Z space
        let z_volume_overlap = a.min_z < b.max_z && a.max_z > b.min_z;

        // B) Face Contact: Their Z boundaries touch exactly (adjacent layers)
        let z_face_contact = (a.max_z == b.min_z) || (b.max_z == a.min_z);

        z_volume_overlap || z_face_contact
    }

    /// Compute the bounding box that contains all nodes in an island.
    fn compute_island_bbox(&self, nodes: &[GeometryNodeRef]) -> BoundingBox {
        if nodes.is_empty() {
            return BoundingBox {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 0,
                max_y: 0,
                max_z: 0,
            };
        }

        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut min_z = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;
        let mut max_z = i64::MIN;

        for node in nodes {
            let bbox = self.get_node_bbox(node);
            min_x = min_x.min(bbox.min_x);
            min_y = min_y.min(bbox.min_y);
            min_z = min_z.min(bbox.min_z);
            max_x = max_x.max(bbox.max_x);
            max_y = max_y.max(bbox.max_y);
            max_z = max_z.max(bbox.max_z);
        }

        BoundingBox {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        }
    }

    /// Get the bounding box for a geometry node.
    fn get_node_bbox(&self, node: &GeometryNodeRef) -> &BoundingBox {
        match node {
            GeometryNodeRef::Pour(idx) => self.pours[*idx].bbox.as_ref().unwrap(),
            GeometryNodeRef::Contact(idx) => self.contacts[*idx].bbox.as_ref().unwrap(),
            GeometryNodeRef::SubstrateLayer(idx) => &self.substrate_layers[*idx].bbox,
        }
    }

    /// Find all pins that touch a given bounding box.
    ///
    /// This is used to populate the `pins` field in `ConductiveIsland` by checking
    /// which component pins physically intersect with the island's geometry.
    fn find_pins_touching_bbox(
        &self,
        bbox: &BoundingBox,
        pin_positions: &[PinPosition],
    ) -> Vec<PinRef> {
        let mut touching_pins = Vec::new();

        for pin_pos in pin_positions {
            // Check if pin position is inside the bounding box
            // A pin "touches" an island if it's within the island's 3D volume
            if pin_pos.x_nm >= bbox.min_x
                && pin_pos.x_nm <= bbox.max_x
                && pin_pos.y_nm >= bbox.min_y
                && pin_pos.y_nm <= bbox.max_y
                && pin_pos.z_nm >= bbox.min_z
                && pin_pos.z_nm <= bbox.max_z
            {
                touching_pins.push(PinRef {
                    component_id: pin_pos.component_id,
                    pin_id: pin_pos.pin_id,
                });
            }
        }

        touching_pins
    }
}
