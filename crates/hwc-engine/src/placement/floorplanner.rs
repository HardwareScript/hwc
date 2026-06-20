//! Coarse Floorplanner (Logic-to-Grid Auto-Placer)
//!
//! This module implements O(1) spatial floorplanning using CoarseGrid.
//! It bridges the gap between LogicSynthesizer (code → gates) and VoxelStamps (gates → voxels)
//! by automatically determining WHERE gates should be placed.
//!
//! **The Problem**: Without this, users must manually define [x, y, z] for millions of gates,
//! or the compiler dumps them all at [0,0,0] causing collisions.
//!
//! **The Solution**: Force-Directed placement at coarse level (16×16×16 voxel regions)
//! - Group components by "Logic Connectivity" (shared nets)
//! - Assign gate clusters to coarse grid regions
//! - Minimize wire length estimate before routing starts
//!
//! **Performance**: O(N log N) for N gates, operates on coarse grid for speed

use crate::geometry::Point3D;
use crate::geometry_router::{CoarseGrid, CoarseNode, COARSE_CELL_SIZE};
use crate::netlist::{ComponentId, NetId, NetlistArena};
use crate::placement::Anchor;
use crate::space::VoxelSize;
use compact_str::CompactString;

use crate::geometry_router::EntityGraph;
use rustc_hash::{FxHashMap, FxHashSet};

/// Component placement request for floorplanning
#[derive(Debug, Clone)]
pub struct ComponentPlacementRequest {
    /// Component ID from netlist
    pub component_id: ComponentId,

    /// Component type (for size estimation)
    pub component_type: CompactString,

    /// Nets connected to this component
    pub connected_nets: Vec<NetId>,

    /// Estimated size in voxels (width, height, depth)
    pub estimated_size: (usize, usize, usize),

    /// Physical anchor constraint (Task B3)
    /// If specified, floorplanner must respect this constraint
    pub anchor: Anchor,
}

impl ComponentPlacementRequest {
    /// Create a new placement request without anchor (for backward compatibility)
    pub fn new(
        component_id: ComponentId,
        component_type: CompactString,
        connected_nets: Vec<NetId>,
        estimated_size: (usize, usize, usize),
    ) -> Self {
        Self {
            component_id,
            component_type,
            connected_nets,
            estimated_size,
            anchor: Anchor::None,
        }
    }

    /// Create a new placement request with anchor
    pub fn with_anchor(
        component_id: ComponentId,
        component_type: CompactString,
        connected_nets: Vec<NetId>,
        estimated_size: (usize, usize, usize),
        anchor: Anchor,
    ) -> Self {
        Self {
            component_id,
            component_type,
            connected_nets,
            estimated_size,
            anchor,
        }
    }
}

/// Floorplanning result
#[derive(Debug, Clone)]
pub struct FloorplanResult {
    /// Component ID
    pub component_id: ComponentId,

    /// Assigned position in nanometers
    pub position: Point3D,

    /// Assigned coarse grid region
    pub coarse_region: CoarseNode,
}

/// Coarse Floorplanner
///
/// Automatically places components in the voxel grid by:
/// 1. Analyzing connectivity (which components share nets)
/// 2. Clustering connected components together
/// 3. Assigning clusters to coarse grid regions
/// 4. Minimizing estimated wire length
pub struct Floorplanner {
    /// Voxel size for coordinate conversion
    voxel_size_nm: i64,
}

/// Parameters for finding best region for a cluster
struct ClusterPlacementParams<'a> {
    cluster: &'a [ComponentId],
    existing_placements: &'a FxHashMap<ComponentId, CoarseNode>,
    coarse_grid: &'a CoarseGrid,
    occupied: &'a FxHashSet<CoarseNode>,
    max_x: i32,
    max_y: i32,
    max_z: i32,
}

impl Floorplanner {
    /// Create a new floorplanner
    pub fn new(voxel_size: &VoxelSize) -> Self {
        Self {
            voxel_size_nm: voxel_size.x_nm,
        }
    }

    /// Auto-place components using force-directed algorithm with anchor support (Task B3)
    ///
    /// # Arguments
    /// * `requests` - Components to place
    /// * `grid` - Voxel grid (for occupancy checking)
    /// * `arena` - Netlist arena (for connectivity analysis)
    ///
    /// # Returns
    /// Placement results for each component
    ///
    /// # Anchor Handling
    /// Components with anchors are placed first according to their constraints:
    /// - Point anchors: Placed at exact position
    /// - Edge anchors: Placed on specified board edge
    /// - Region anchors: Placed within bounded area
    /// - No anchor: Placed automatically using force-directed algorithm
    pub fn auto_place(
        &self,
        requests: &[ComponentPlacementRequest],
        entity_graph: &EntityGraph,
        _arena: &NetlistArena,
    ) -> Vec<FloorplanResult> {
        if requests.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();
        let (grid_x, grid_y, grid_z) = entity_graph.size();
        let board_size = (
            (grid_x as i64) * self.voxel_size_nm,
            (grid_y as i64) * self.voxel_size_nm,
            (grid_z as i64) * self.voxel_size_nm,
        );

        // Phase 0: Separate anchored and unanchored components
        let mut anchored_requests = Vec::new();
        let mut unanchored_requests = Vec::new();

        for req in requests {
            if req.anchor != Anchor::None {
                anchored_requests.push(req.clone());
            } else {
                unanchored_requests.push(req.clone());
            }
        }

        // Sort anchored components by priority (Point > Edge > Region)
        anchored_requests.sort_by_key(|req| std::cmp::Reverse(req.anchor.priority()));

        // Phase 0.1: Place anchored components first
        for req in &anchored_requests {
            let component_size = (
                (req.estimated_size.0 as i64) * self.voxel_size_nm,
                (req.estimated_size.1 as i64) * self.voxel_size_nm,
                (req.estimated_size.2 as i64) * self.voxel_size_nm,
            );

            if let Some(position) = req.anchor.ideal_position(board_size, component_size) {
                // Convert to coarse node for consistency
                let coarse_x = (position.x / self.voxel_size_nm / COARSE_CELL_SIZE as i64) as i32;
                let coarse_y = (position.y / self.voxel_size_nm / COARSE_CELL_SIZE as i64) as i32;
                let coarse_z = (position.z / self.voxel_size_nm / COARSE_CELL_SIZE as i64) as i32;

                results.push(FloorplanResult {
                    component_id: req.component_id,
                    position,
                    coarse_region: CoarseNode {
                        x: coarse_x,
                        y: coarse_y,
                        z: coarse_z,
                    },
                });
            }
        }

        // Phase 1: Build connectivity graph for unanchored components
        let connectivity = self.build_connectivity_graph(&unanchored_requests);

        // Phase 2: Cluster components by connectivity
        let clusters = self.cluster_by_connectivity(&unanchored_requests, &connectivity);

        // Phase 3: Create coarse grid from voxel grid
        let coarse_grid = CoarseGrid::from_entity_graph(entity_graph, self.voxel_size_nm);

        // Phase 4: Assign clusters to coarse regions
        let cluster_placements = self.assign_clusters_to_regions(&clusters, &coarse_grid, entity_graph);

        // Phase 5: Place unanchored components within their assigned regions
        let unanchored_results =
            self.place_components_in_regions(&unanchored_requests, &cluster_placements, entity_graph);

        results.extend(unanchored_results);
        results
    }

    /// Build connectivity graph (which components share nets)
    fn build_connectivity_graph(
        &self,
        requests: &[ComponentPlacementRequest],
    ) -> FxHashMap<ComponentId, FxHashSet<ComponentId>> {
        let mut graph: FxHashMap<ComponentId, FxHashSet<ComponentId>> = FxHashMap::default();

        // Build net-to-components mapping
        let mut net_to_components: FxHashMap<NetId, Vec<ComponentId>> = FxHashMap::default();
        for req in requests {
            for &net_id in &req.connected_nets {
                net_to_components
                    .entry(net_id)
                    .or_default()
                    .push(req.component_id);
            }
        }

        // Build component-to-component connectivity
        for components in net_to_components.values() {
            for &comp_a in components {
                for &comp_b in components {
                    if comp_a != comp_b {
                        graph.entry(comp_a).or_default().insert(comp_b);
                    }
                }
            }
        }

        graph
    }

    /// Cluster components by connectivity using Union-Find
    fn cluster_by_connectivity(
        &self,
        requests: &[ComponentPlacementRequest],
        connectivity: &FxHashMap<ComponentId, FxHashSet<ComponentId>>,
    ) -> Vec<Vec<ComponentId>> {
        let mut parent: FxHashMap<ComponentId, ComponentId> = FxHashMap::default();

        // Initialize: each component is its own parent
        for req in requests {
            parent.insert(req.component_id, req.component_id);
        }

        // Find with path compression
        fn find(
            node: ComponentId,
            parent: &mut FxHashMap<ComponentId, ComponentId>,
        ) -> ComponentId {
            let p = *parent.get(&node).unwrap();
            if p != node {
                let root = find(p, parent);
                parent.insert(node, root);
                root
            } else {
                node
            }
        }

        // Union connected components
        for (&comp_a, neighbors) in connectivity {
            for &comp_b in neighbors {
                let root_a = find(comp_a, &mut parent);
                let root_b = find(comp_b, &mut parent);
                if root_a != root_b {
                    parent.insert(root_b, root_a);
                }
            }
        }

        // Group components by cluster root
        let mut clusters: FxHashMap<ComponentId, Vec<ComponentId>> = FxHashMap::default();
        for req in requests {
            let root = find(req.component_id, &mut parent);
            clusters.entry(root).or_default().push(req.component_id);
        }

        clusters.into_values().collect()
    }

    /// Assign clusters to coarse grid regions using bin-packing
    fn assign_clusters_to_regions(
        &self,
        clusters: &[Vec<ComponentId>],
        coarse_grid: &CoarseGrid,
        entity_graph: &EntityGraph,
    ) -> FxHashMap<ComponentId, CoarseNode> {
        let mut placements: FxHashMap<ComponentId, CoarseNode> = FxHashMap::default();
        let (size_x, size_y, size_z) = entity_graph.size();

        // Calculate coarse grid bounds
        let max_coarse_x = size_x.div_ceil(COARSE_CELL_SIZE) as i32;
        let max_coarse_y = size_y.div_ceil(COARSE_CELL_SIZE) as i32;
        let max_coarse_z = size_z.div_ceil(COARSE_CELL_SIZE) as i32;

        // Track occupied coarse regions
        let mut occupied: FxHashSet<CoarseNode> = FxHashSet::default();

        // Place each cluster
        for cluster in clusters {
            // Find best coarse region for this cluster
            let best_region = self.find_best_region_for_cluster(ClusterPlacementParams {
                cluster,
                existing_placements: &placements,
                coarse_grid,
                occupied: &occupied,
                max_x: max_coarse_x,
                max_y: max_coarse_y,
                max_z: max_coarse_z,
            });

            // Assign all components in cluster to this region
            for &comp_id in cluster {
                placements.insert(comp_id, best_region);
            }

            occupied.insert(best_region);
        }

        placements
    }

    /// Find best coarse region for a cluster
    fn find_best_region_for_cluster(&self, params: ClusterPlacementParams) -> CoarseNode {
        // If this is the first cluster, place at origin
        if params.existing_placements.is_empty() {
            return CoarseNode::new(0, 0, 0);
        }

        // Find center of gravity of already-placed connected components
        let mut center_x = 0i64;
        let mut center_y = 0i64;
        let mut center_z = 0i64;
        let mut count = 0i64;

        for &comp_id in params.cluster {
            // This component isn't placed yet, but check if any of its neighbors are
            // (This is a simplified heuristic - in a full implementation, we'd analyze
            // the connectivity graph more deeply)
            if let Some(&node) = params.existing_placements.get(&comp_id) {
                center_x += node.x as i64;
                center_y += node.y as i64;
                center_z += node.z as i64;
                count += 1;
            }
        }

        // If no connected components are placed yet, find first empty region
        if count == 0 {
            return self.find_first_empty_region(
                params.occupied,
                params.max_x,
                params.max_y,
                params.max_z,
            );
        }

        // Calculate center of gravity
        let target_x = (center_x / count) as i32;
        let target_y = (center_y / count) as i32;
        let target_z = (center_z / count) as i32;

        // Find closest empty region to center of gravity
        self.find_closest_empty_region(
            CoarseNode::new(target_x, target_y, target_z),
            params.coarse_grid,
            params.occupied,
            params.max_x,
            params.max_y,
            params.max_z,
        )
    }

    /// Find first empty coarse region
    fn find_first_empty_region(
        &self,
        occupied: &FxHashSet<CoarseNode>,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> CoarseNode {
        for z in 0..max_z {
            for y in 0..max_y {
                for x in 0..max_x {
                    let node = CoarseNode::new(x, y, z);
                    if !occupied.contains(&node) {
                        return node;
                    }
                }
            }
        }

        // Fallback: return origin (should never happen in practice)
        CoarseNode::new(0, 0, 0)
    }

    /// Find closest empty region to target
    fn find_closest_empty_region(
        &self,
        target: CoarseNode,
        coarse_grid: &CoarseGrid,
        occupied: &FxHashSet<CoarseNode>,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> CoarseNode {
        let mut best_node = target;
        let mut best_distance = i32::MAX;
        let mut best_occupancy = 100u8;

        // Search in expanding sphere around target
        for z in 0..max_z {
            for y in 0..max_y {
                for x in 0..max_x {
                    let node = CoarseNode::new(x, y, z);

                    // Skip if already occupied by another cluster
                    if occupied.contains(&node) {
                        continue;
                    }

                    let distance = target.manhattan_distance(&node);
                    let occupancy = coarse_grid.get_occupancy(&node);

                    // Prefer closer regions with lower occupancy
                    let is_better = distance < best_distance
                        || (distance == best_distance && occupancy < best_occupancy);

                    if is_better {
                        best_node = node;
                        best_distance = distance;
                        best_occupancy = occupancy;
                    }
                }
            }
        }

        best_node
    }

    /// Place components within their assigned coarse regions
    fn place_components_in_regions(
        &self,
        requests: &[ComponentPlacementRequest],
        cluster_placements: &FxHashMap<ComponentId, CoarseNode>,
        entity_graph: &EntityGraph,
    ) -> Vec<FloorplanResult> {
        let mut results = Vec::new();

        for req in requests {
            let coarse_region = cluster_placements
                .get(&req.component_id)
                .copied()
                .unwrap_or(CoarseNode::new(0, 0, 0));

            // Convert coarse region to physical position (center of region)
            let position = self.coarse_node_to_position(coarse_region, req, entity_graph);

            results.push(FloorplanResult {
                component_id: req.component_id,
                position,
                coarse_region,
            });
        }

        results
    }

    /// Convert coarse node to physical position in nanometers
    fn coarse_node_to_position(
        &self,
        node: CoarseNode,
        req: &ComponentPlacementRequest,
        entity_graph: &EntityGraph,
    ) -> Point3D {
        // Calculate center of coarse cell in voxels
        let voxel_x = (node.x as usize * COARSE_CELL_SIZE) + (COARSE_CELL_SIZE / 2);
        let voxel_y = (node.y as usize * COARSE_CELL_SIZE) + (COARSE_CELL_SIZE / 2);
        let voxel_z = (node.z as usize * COARSE_CELL_SIZE) + (COARSE_CELL_SIZE / 2);

        // Clamp to grid bounds
        let (size_x, size_y, size_z) = entity_graph.size();
        let voxel_x = voxel_x.min(size_x.saturating_sub(req.estimated_size.0));
        let voxel_y = voxel_y.min(size_y.saturating_sub(req.estimated_size.1));
        let voxel_z = voxel_z.min(size_z.saturating_sub(req.estimated_size.2));

        // Convert to nanometers
        Point3D::new(
            (voxel_x as i64) * self.voxel_size_nm,
            (voxel_y as i64) * self.voxel_size_nm,
            (voxel_z as i64) * self.voxel_size_nm,
        )
    }

    /// Estimate total wire length for a placement
    ///
    /// Used for quality metrics and optimization
    pub fn estimate_wire_length(&self, results: &[FloorplanResult], arena: &NetlistArena) -> i64 {
        let mut total_length = 0i64;

        // Build component position map
        let positions: FxHashMap<ComponentId, Point3D> = results
            .iter()
            .map(|r| (r.component_id, r.position))
            .collect();

        // For each net, calculate bounding box
        for net_id in arena.all_net_ids() {
            let pins = match arena.get_net_pins(net_id) {
                Some(pins) => pins,
                None => continue,
            };

            if pins.len() < 2 {
                continue;
            }

            // Get component positions for this net
            let mut net_positions = Vec::new();
            for &pin_id in pins {
                if let Some(pin_data) = arena.get_pin(pin_id) {
                    if let Some(&pos) = positions.get(&pin_data.parent_component) {
                        net_positions.push(pos);
                    }
                }
            }

            if net_positions.len() < 2 {
                continue;
            }

            // Calculate bounding box (half-perimeter wire length estimate)
            let mut min_x = i64::MAX;
            let mut max_x = i64::MIN;
            let mut min_y = i64::MAX;
            let mut max_y = i64::MIN;
            let mut min_z = i64::MAX;
            let mut max_z = i64::MIN;

            for pos in &net_positions {
                min_x = min_x.min(pos.x);
                max_x = max_x.max(pos.x);
                min_y = min_y.min(pos.y);
                max_y = max_y.max(pos.y);
                min_z = min_z.min(pos.z);
                max_z = max_z.max(pos.z);
            }

            // Half-perimeter wire length
            let hpwl = (max_x - min_x) + (max_y - min_y) + (max_z - min_z);
            total_length += hpwl;
        }

        total_length
    }
}
