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
        let curr_node_ref = &all_nodes[curr_idx].0;
        let curr_bbox = &all_nodes[curr_idx].1;

        // Get candidates from spatial_grid
        let candidates = spatial_grid.get_candidates(curr_bbox);

        // Check each candidate for actual touching and material match
        for neighbor_idx in candidates {
            if neighbor_idx == curr_idx || visited.contains(&neighbor_idx) {
                continue;
            }

            let neighbor_node_ref = &all_nodes[neighbor_idx].0;
            let neighbor_bbox = &all_nodes[neighbor_idx].1;
            let neighbor_material = self.get_node_material(neighbor_node_ref);

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

            if self.nodes_touch_precise(curr_node_ref, curr_bbox, neighbor_node_ref, neighbor_bbox)
            {
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

    /// Create a unified node list from substrate layers (v0.1.7).
    ///
    /// v0.1.7: We only use SubstrateLayers because they contain the "physical truth"
    /// including drilled holes and cutouts. Pours and Contacts are represented
    /// as substrate layers in the VoxelGrid.
    fn collect_all_geometry_nodes(&self) -> Vec<(GeometryNodeRef, BoundingBox)> {
        let mut nodes = Vec::new();

        // Add substrate layers - these represent the actual voxel geometry
        for (idx, layer) in self.substrate_layers.iter().enumerate() {
            nodes.push((GeometryNodeRef::SubstrateLayer(idx), layer.bbox.clone()));
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

    /// Check if two nodes physically touch, with precise cutout awareness (v0.1.7).
    fn nodes_touch_precise(
        &self,
        a_ref: &GeometryNodeRef,
        a_bbox: &BoundingBox,
        b_ref: &GeometryNodeRef,
        b_bbox: &BoundingBox,
    ) -> bool {
        // Step 1: Basic bbox touch check
        if !self.nodes_touch_bbox(a_bbox, b_bbox) {
            return false;
        }

        // Step 2: Precise check for SubstrateLayers (v0.1.7)
        // If either node has cutouts or non-rect shape, we must verify contact.
        match (a_ref, b_ref) {
            (GeometryNodeRef::SubstrateLayer(idx_a), GeometryNodeRef::SubstrateLayer(idx_b)) => {
                let layer_a = &self.substrate_layers[*idx_a];
                let layer_b = &self.substrate_layers[*idx_b];

                // Precise check (v0.1.7):
                // If they only touch at a face (adjacent layers), bbox check is sufficient
                // because cutouts are 3D volumes and face-only contact is always conductive
                // if the bboxes touch.
                let z_volume_overlap = a_bbox.min_z < b_bbox.max_z && a_bbox.max_z > b_bbox.min_z;
                if !z_volume_overlap {
                    return true;
                }

                // If they overlap in volume, we must check for cutouts (Auto-Drill clearance).
                // Heuristic: Sample the center of the intersection volume.
                let intersect_min_x = a_bbox.min_x.max(b_bbox.min_x);
                let intersect_max_x = a_bbox.max_x.min(b_bbox.max_x);
                let intersect_min_y = a_bbox.min_y.max(b_bbox.min_y);
                let intersect_max_y = a_bbox.max_y.min(b_bbox.max_y);
                let intersect_min_z = a_bbox.min_z.max(b_bbox.min_z);
                let intersect_max_z = a_bbox.max_z.min(b_bbox.max_z);

                // v0.1.7: Sample 5 points (center + 4 mid-radius points) to handle Tube/Hollow shapes
                let test_x = (intersect_min_x + intersect_max_x) / 2;
                let test_y = (intersect_min_y + intersect_max_y) / 2;

                let dx = (intersect_max_x - intersect_min_x) / 3;
                let dy = (intersect_max_y - intersect_min_y) / 3;

                let x_points = [test_x, test_x - dx, test_x + dx];
                let y_points = [test_y, test_y - dy, test_y + dy];

                let z_mid = (intersect_min_z + intersect_max_z) / 2;
                let z_points = [intersect_min_z, z_mid, intersect_max_z];

                // Contact if both layers contain any of the test points
                let mut any_contact = false;
                for &tx in &x_points {
                    for &ty in &y_points {
                        // Skip corners (redundant with center/mid checks for circles)
                        if tx != test_x && ty != test_y {
                            continue;
                        }

                        for &tz in &z_points {
                            if layer_a.contains_nm(tx, ty, tz) && layer_b.contains_nm(tx, ty, tz) {
                                any_contact = true;
                                break;
                            }
                        }
                        if any_contact {
                            break;
                        }
                    }
                    if any_contact {
                        break;
                    }
                }

                if !any_contact {
                    // Fallback: If center sample fails, try sampling the corners of the intersection
                    // v0.1.7: Added 1nm inner offset to guarantee we hit solid copper and avoid edge precision issues.
                    let inner_offset = 1; // 1nm
                    let corners = [
                        (
                            intersect_min_x + inner_offset,
                            intersect_min_y + inner_offset,
                        ),
                        (
                            intersect_max_x - inner_offset,
                            intersect_min_y + inner_offset,
                        ),
                        (
                            intersect_min_x + inner_offset,
                            intersect_max_y - inner_offset,
                        ),
                        (
                            intersect_max_x - inner_offset,
                            intersect_max_y - inner_offset,
                        ),
                    ];

                    for (cx, cy) in corners {
                        for &cz in &z_points {
                            if layer_a.contains_nm(cx, cy, cz) && layer_b.contains_nm(cx, cy, cz) {
                                return true;
                            }
                        }
                    }
                }

                any_contact
            }
            _ => true, // Fallback to bbox check for Pours/Contacts (already analytic)
        }
    }

    /// Check if two bounding boxes physically touch (XY edge-inclusive, Z volume-overlapping or face-touching).
    fn nodes_touch_bbox(&self, a: &BoundingBox, b: &BoundingBox) -> bool {
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
        // v0.1.7: Added 1nm tolerance for rounding stability
        let z_face_contact = (a.max_z - b.min_z).abs() <= 1 || (b.max_z - a.min_z).abs() <= 1;

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
