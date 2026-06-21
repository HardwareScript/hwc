use super::spatial_grid::SpatialGrid;
use super::types::*;
use crate::connectivity::{BoundingBox, SubstrateLayerMetadata};
use rustc_hash::FxHashSet;

pub struct IslandBuilder<'a> {
    substrate_layers: &'a [SubstrateLayerMetadata],
    route_segments: &'a [RouteSegmentMetadata],
    bridge_rules: &'a [crate::BridgeRule],
    material_mapping: &'a rustc_hash::FxHashMap<compact_str::CompactString, u8>,
}

impl<'a> IslandBuilder<'a> {
    pub fn new(
        substrate_layers: &'a [SubstrateLayerMetadata],
        route_segments: &'a [RouteSegmentMetadata],
        bridge_rules: &'a [crate::BridgeRule],
        material_mapping: &'a rustc_hash::FxHashMap<compact_str::CompactString, u8>,
    ) -> Self {
        Self {
            substrate_layers,
            route_segments,
            bridge_rules,
            material_mapping,
        }
    }

    pub fn build_islands(&self, pin_positions: Option<&[PinPosition]>) -> Vec<ConductiveIsland> {
        let mut islands = Vec::new();
        let mut visited = FxHashSet::default();
        let mut island_id = 0;

        let all_nodes = self.collect_all_geometry_nodes();
        let spatial_grid = SpatialGrid::build(&all_nodes);

        for (idx, _) in all_nodes.iter().enumerate() {
            if visited.contains(&idx) {
                continue;
            }

            let mut island_nodes = Vec::new();
            let mut stack = vec![idx];
            let material = self.get_node_material(&all_nodes[idx].0);

            while let Some(curr_idx) = stack.pop() {
                if visited.contains(&curr_idx) {
                    continue;
                }

                visited.insert(curr_idx);
                island_nodes.push(all_nodes[curr_idx].0);

                let neighbors =
                    self.find_touching_neighbors(curr_idx, &all_nodes, &spatial_grid, material, &visited);

                for neighbor_idx in neighbors {
                    stack.push(neighbor_idx);
                }
            }

            let bbox = self.compute_island_bbox(&island_nodes);

            let pins = if let Some(pin_pos_list) = pin_positions {
                self.find_pins_touching_bbox(&bbox, pin_pos_list)
            } else {
                Vec::new()
            };

            islands.push(ConductiveIsland {
                id: island_id,
                nodes: island_nodes,
                bbox,
                material,
                pins,
            });

            island_id += 1;
        }

        islands
    }

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
        let curr_net = self.get_node_net(&all_nodes[curr_idx].0);

        let candidates = spatial_grid.get_candidates(curr_bbox);

        for neighbor_idx in candidates {
            if neighbor_idx == curr_idx || visited.contains(&neighbor_idx) {
                continue;
            }

            let neighbor_bbox = &all_nodes[neighbor_idx].1;
            let neighbor_material = self.get_node_material(&all_nodes[neighbor_idx].0);
            let neighbor_net = self.get_node_net(&all_nodes[neighbor_idx].0);

            if !self.nodes_touch_bbox(curr_bbox, neighbor_bbox) {
                continue;
            }

            // Same net: always connected (material is irrelevant for same-net connectivity)
            if curr_net.is_some() && curr_net == neighbor_net {
                neighbors.push(neighbor_idx);
                continue;
            }

            // Different net or unknown net: check material compatibility
            let materials_can_connect = if neighbor_material == material {
                true
            } else {
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

            if materials_can_connect {
                neighbors.push(neighbor_idx);
            }
        }

        neighbors
    }

    fn get_material_name_from_id(&self, id: u8) -> Option<compact_str::CompactString> {
        for (name, &mat_id) in self.material_mapping.iter() {
            if mat_id == id {
                return Some(name.clone());
            }
        }
        None
    }

    fn collect_all_geometry_nodes(&self) -> Vec<(GeometryNodeRef, BoundingBox)> {
        let mut nodes = Vec::new();

        for (idx, layer) in self.substrate_layers.iter().enumerate() {
            nodes.push((GeometryNodeRef::SubstrateLayer(idx), layer.bbox.clone()));
        }

        for (idx, seg) in self.route_segments.iter().enumerate() {
            nodes.push((GeometryNodeRef::RouteSegment(idx), seg.bbox.clone()));
        }

        nodes
    }

    fn get_node_material(&self, node: &GeometryNodeRef) -> u8 {
        match node {
            GeometryNodeRef::SubstrateLayer(idx) => self.substrate_layers[*idx].material,
            GeometryNodeRef::SubstrateLayerRegion(_, _) => {
                unreachable!("SubstrateLayerRegion nodes should not appear in flat substrate_layers list")
            }
            GeometryNodeRef::RouteSegment(idx) => self.route_segments[*idx].material,
            GeometryNodeRef::Pour(_) | GeometryNodeRef::Contact(_) => {
                unreachable!("Pour/Contact nodes are not produced by this builder")
            }
        }
    }

    fn get_node_net(&self, node: &GeometryNodeRef) -> Option<u32> {
        match node {
            GeometryNodeRef::SubstrateLayer(idx) => Some(self.substrate_layers[*idx].net),
            GeometryNodeRef::RouteSegment(idx) => Some(self.route_segments[*idx].net),
            _ => None,
        }
    }

    fn nodes_touch_bbox(&self, a: &BoundingBox, b: &BoundingBox) -> bool {
        let x_overlap = a.min_x <= b.max_x && a.max_x >= b.min_x;
        let y_overlap = a.min_y <= b.max_y && a.max_y >= b.min_y;

        if !x_overlap || !y_overlap {
            return false;
        }

        let z_volume_overlap = a.min_z < b.max_z && a.max_z > b.min_z;
        let z_face_contact = (a.max_z - b.min_z).abs() <= 1 || (b.max_z - a.min_z).abs() <= 1;

        z_volume_overlap || z_face_contact
    }

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

    fn get_node_bbox(&self, node: &GeometryNodeRef) -> &BoundingBox {
        match node {
            GeometryNodeRef::SubstrateLayer(idx) => &self.substrate_layers[*idx].bbox,
            GeometryNodeRef::SubstrateLayerRegion(_, _) => {
                unreachable!("SubstrateLayerRegion nodes should not appear in flat substrate_layers list")
            }
            GeometryNodeRef::RouteSegment(idx) => &self.route_segments[*idx].bbox,
            GeometryNodeRef::Pour(_) | GeometryNodeRef::Contact(_) => {
                unreachable!("Pour/Contact nodes are not produced by this builder")
            }
        }
    }

    fn find_pins_touching_bbox(
        &self,
        bbox: &BoundingBox,
        pin_positions: &[PinPosition],
    ) -> Vec<PinRef> {
        let mut touching_pins = Vec::new();

        for pin_pos in pin_positions {
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
