//! Main A* routing implementation

use crate::geometry::Point3D;
use crate::geometry_router::coarse_grid::CoarseGrid;
use crate::geometry_router::neighbor_generation::get_neighbors_stable;

use super::collision::try_binary_collision_skip;
use super::cost::{calculate_move_cost, MoveCostParams};
use super::heuristic::heuristic;
use super::state::{reconstruct_path, PathfindingState};
use super::types::RoutingParams;

/// Route a net using deterministic A* pathfinding with full clearance and crosstalk detection.
///
/// Uses A* algorithm with deterministic tie-breaking to ensure reproducible results.
/// The same input will always produce the same output.
///
/// **Algorithm**:
/// 1. Initialize frontier with start point
/// 2. While frontier not empty:
///    - Pop lowest f-score node
///    - If goal reached, reconstruct path
///    - Get neighbors in stable order
///    - For each neighbor:
///      - Calculate new cost (with clearance/crosstalk penalties)
///      - If better than previous, update
///      - Add to frontier
/// 3. Return path or None if impossible
///
/// **v0.1.4 Full Implementation**:
/// Now includes real-time clearance violation and crosstalk detection during pathfinding.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 700-800, Deterministic Routing)
///
/// # Arguments
/// * `start` - Starting position
/// * `goal` - Goal position
/// * `params` - Routing parameters including constraints, bounds, and clearance zones
///
/// # Returns
/// Path from start to goal, or None if no path exists
pub fn route_net_deterministic(
    start: Point3D,
    goal: Point3D,
    params: &RoutingParams,
) -> Option<Vec<Point3D>> {
    // Snap start and goal to voxel centers
    let snap_coord = |coord: i64, voxel_size: i64, max_nm: i64| -> i64 {
        if voxel_size > 0 {
            // Snap to the center of the voxel that contains this coordinate
            // index = floor(coord / voxel_size)
            // center = (index * voxel_size) + (voxel_size / 2)
            let mut index = if coord >= 0 {
                coord / voxel_size
            } else {
                (coord - voxel_size + 1) / voxel_size
            };

            // v0.1.7: Boundary-Aware Snapping
            // Prevents coordinates at the very edge (e.g., z=1.27mm) from snapping
            // to a non-existent voxel index (e.g., index 2 in a 2-voxel grid).
            let max_idx = (max_nm / voxel_size).saturating_sub(1);
            index = index.min(max_idx).max(0);

            (index * voxel_size) + (voxel_size / 2)
        } else {
            coord
        }
    };

    let start_snapped = Point3D::new(
        snap_coord(start.x, params.voxel_size.x_nm, params.bounds.width_nm),
        snap_coord(start.y, params.voxel_size.y_nm, params.bounds.height_nm),
        snap_coord(start.z, params.voxel_size.z_nm, params.bounds.depth_nm),
    );
    let goal_snapped = Point3D::new(
        snap_coord(goal.x, params.voxel_size.x_nm, params.bounds.width_nm),
        snap_coord(goal.y, params.voxel_size.y_nm, params.bounds.height_nm),
        snap_coord(goal.z, params.voxel_size.z_nm, params.bounds.depth_nm),
    );

    let mut state = PathfindingState::new();

    // Initialize with snapped start node
    let h = heuristic(start_snapped, goal_snapped);
    state.cost_so_far.insert(start_snapped, 0);
    state.add_node(start_snapped, h);

    // v0.1.7: Safety limit to prevent infinite loops when path is impossible
    const MAX_ITERATIONS: usize = 100_000;
    let mut iterations: usize = 0;

    while !state.is_empty() {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            #[cfg(debug_assertions)]
            eprintln!(
                "[ROUTER] Deterministic A* exceeded {} iterations, aborting route from ({},{},{}) to ({},{},{})",
                MAX_ITERATIONS,
                start_snapped.x, start_snapped.y, start_snapped.z,
                goal_snapped.x, goal_snapped.y, goal_snapped.z,
            );
            return None;
        }
        let current = state.pop_node()?;

        // Goal reached - reconstruct path
        if current == goal_snapped {
            let mut path = reconstruct_path(&state.came_from, start_snapped, goal_snapped);

            // Restore exact pin positions at the ends if they differ from snapped positions
            if start != start_snapped && !path.is_empty() && path[0] == start_snapped {
                path[0] = start;
            }
            if goal != goal_snapped && !path.is_empty() {
                let last_idx = path.len() - 1;
                if path[last_idx] == goal_snapped {
                    path[last_idx] = goal;
                }
            }

            // v0.1.7 Fix: Lock ALL intermediate points to the exact physical Z-plane.
            // When fixed_z_nm is set (2.5D routing), every intermediate point must
            // use the original non-snapped Z to avoid 21μm quantization noise.
            // Guard: path needs at least 3 points to have any intermediate points.
            // A path of length 0 or 1 means start==goal (same voxel), nothing to fix up.
            if let Some(fixed_z) = params.fixed_z_nm {
                if path.len() >= 3 {
                    let last_idx = path.len() - 1;
                    for point in path[1..last_idx].iter_mut() {
                        point.z = fixed_z;
                    }
                }
            }

            return Some(path);
        }

        // Skip if already visited
        if state.visited.contains(&current) {
            continue;
        }
        state.visited.insert(current);

        // v0.1.7: Planar Lock (2.5D Routing)
        if let Some(fixed_z) = params.fixed_z_nm {
            let snapped_fixed_z = snap_coord(fixed_z, params.voxel_size.z_nm, params.bounds.depth_nm);
            if current.z != snapped_fixed_z {
                continue;
            }
        }

        let current_cost = *state.cost_so_far.get(&current)?;

        // Get neighbors in stable order
        let neighbors = get_neighbors_stable(
            current,
            params.bounds,
            params.layer_direction,
            params.voxel_size,
        );

        // BINARY COLLISION SKIP: Try to validate all neighbors at once using VoxelGrid
        // If all neighbors are in the same chunk, we can check them with 1 bitwise AND
        // instead of 6 individual FxHashSet lookups (60× faster)
        let valid_neighbors = if let Some(grid) = params.voxel_grid {
            try_binary_collision_skip(current, &neighbors, grid, params.voxel_size)
        } else {
            None
        };

        // Use binary skip result if available, otherwise fall back to individual checks
        let neighbors_to_check = if let Some(valid) = valid_neighbors {
            valid
        } else {
            neighbors
        };

        for neighbor in neighbors_to_check {
            // Skip if already visited
            if state.visited.contains(&neighbor) {
                continue;
            }

            // CORRIDOR CONSTRAINT: If a corridor is specified, skip neighbors outside it
            if let Some(corridor) = params.corridor {
                if !CoarseGrid::point_in_corridor(neighbor, corridor, params.voxel_size.x_nm) {
                    continue;
                }
            }

            // HARD BLOCK: Stop A* from entering occupied voxels (unless it's the target pin)
            if neighbor != goal_snapped && params.occupied_voxels.contains(&neighbor) {
                continue;
            }

            // v0.1.7 (Strict Box Model): Block the entire interior volume of all components.
            // Exempt components containing the start or goal pins (boundary-docking).
            if let Some(voxel_grid) = params.voxel_grid {
                if let Some(component_name) =
                    voxel_grid.point_in_component(neighbor.x, neighbor.y, neighbor.z)
                {
                    // Allow routing to/from the start and goal pin's component
                    if !params.exempt_components.is_empty()
                        && params.exempt_components.contains(&component_name)
                    {
                        // Exempt: this is the start or goal component
                    } else {
                        continue; // Block routing through component interior
                    }
                }
            }

            // SOFT PENALTY: Clearance zones are handled in calculate_move_cost
            // We don't hard-block clearance zones because:
            // 1. Multiple routes from the same pin need to pass through each other's zones
            // 2. DRC will catch actual violations after routing
            // 3. The cost penalty guides the router away from violations when possible

            // Calculate new cost with layer direction preference
            let move_cost_params = MoveCostParams {
                from: current,
                to: neighbor,
                net_id: params.net_id,
                constraints: params.constraints,
                voxel_size_nm: params.voxel_size.x_nm,
                occupied_voxels: params.occupied_voxels,
                clearance_zones: params.clearance_zones,
                layer_direction: Some(params.layer_direction),
                substrate_layers: params.substrate_layers,
                is_high_speed_net: params.is_high_speed_net,
            };
            let move_cost = calculate_move_cost(&move_cost_params);
            let new_cost = current_cost + move_cost;

            // Check if this is a better path
            let is_better = match state.cost_so_far.get(&neighbor) {
                Some(&old_cost) => new_cost < old_cost,
                None => true,
            };

            if is_better {
                // Update cost and parent
                state.cost_so_far.insert(neighbor, new_cost);
                state.came_from.insert(neighbor, current);

                // Add to frontier
                let h = heuristic(neighbor, goal_snapped);
                let f_score = new_cost + h;
                state.add_node(neighbor, f_score);
            }
        }
    }

    // No path found
    None
}
