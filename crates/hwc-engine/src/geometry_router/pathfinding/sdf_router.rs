//! SDF-accelerated A* routing (Leap-Frog Router)

use crate::geometry::Point3D;
use crate::geometry_router::coarse_grid::CoarseGrid;
use crate::geometry_router::neighbor_generation::get_neighbors_stable;
use crate::geometry_router::sdf_generator::SdfGenerator;

use super::collision::{try_binary_collision_skip, voxel_to_coords};
use super::cost::{calculate_move_cost, MoveCostParams};
use super::heuristic::heuristic;
use super::state::{reconstruct_path, PathfindingState};
use super::types::RoutingParams;

/// Route a net using SDF-accelerated A* pathfinding (Leap-Frog Router).
///
/// This version uses Signed Distance Fields to skip over empty space, dramatically
/// reducing the number of voxels that need to be checked.
///
/// **Sphere Tracing Algorithm**:
/// 1. Query SDF at current position
/// 2. If distance is D, we can safely skip D voxels in the direction of the goal
/// 3. Jump to that position and repeat
/// 4. When close to obstacles (distance < threshold), fall back to normal A*
///
/// **Expected Performance**:
/// - Sparse boards: 5-10× faster than traditional A*
/// - Dense boards: 2-3× faster than traditional A*
///
/// # Arguments
/// * `start` - Starting position
/// * `goal` - Goal position
/// * `params` - Routing parameters
/// * `sdf` - Signed distance field for leap-frog acceleration
///
/// # Returns
/// Path from start to goal, or None if no path exists
pub fn route_net_sdf_accelerated(
    start: Point3D,
    goal: Point3D,
    params: &RoutingParams,
    sdf: &SdfGenerator,
) -> Option<Vec<Point3D>> {
    // Snap start and goal to voxel centers
    let snap_coord = |coord: i64, voxel_size: i64| -> i64 {
        if voxel_size > 0 {
            // Snap to the center of the voxel that contains this coordinate
            // index = floor(coord / voxel_size)
            // center = (index * voxel_size) + (voxel_size / 2)
            let index = if coord >= 0 {
                coord / voxel_size
            } else {
                (coord - voxel_size + 1) / voxel_size
            };
            (index * voxel_size) + (voxel_size / 2)
        } else {
            coord
        }
    };

    let start_snapped = Point3D::new(
        snap_coord(start.x, params.voxel_size.x_nm),
        snap_coord(start.y, params.voxel_size.y_nm),
        if params.fixed_z_nm.is_some() {
            start.z // v0.1.7: Lock to exact physical Z for 2.5D routing
        } else {
            snap_coord(start.z, params.voxel_size.z_nm)
        },
    );
    let goal_snapped = Point3D::new(
        snap_coord(goal.x, params.voxel_size.x_nm),
        snap_coord(goal.y, params.voxel_size.y_nm),
        if params.fixed_z_nm.is_some() {
            goal.z // v0.1.7: Lock to exact physical Z for 2.5D routing
        } else {
            snap_coord(goal.z, params.voxel_size.z_nm)
        },
    );

    eprintln!("[ROUTER DEBUG] Starting SDF-accelerated routing");
    eprintln!(
        "[ROUTER DEBUG]   Start: {:?} nm -> Snapped: {:?}",
        start, start_snapped
    );
    eprintln!(
        "[ROUTER DEBUG]   Goal: {:?} nm -> Snapped: {:?}",
        goal, goal_snapped
    );
    eprintln!("[ROUTER DEBUG]   Voxel size: {:?} nm", params.voxel_size);
    eprintln!(
        "[ROUTER DEBUG]   Manhattan distance: {} voxels",
        start_snapped.manhattan_distance(&goal_snapped) / params.voxel_size.x_nm
    );

    let mut state = PathfindingState::new();

    // Initialize with snapped start node
    let h = heuristic(start_snapped, goal_snapped);
    state.cost_so_far.insert(start_snapped, 0);
    state.add_node(start_snapped, h);

    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 100_000;

    while !state.is_empty() {
        iterations += 1;

        if iterations % 10_000 == 0 {
            eprintln!(
                "[ROUTER DEBUG] Iteration {}: {} nodes in queue, {} visited",
                iterations,
                state.priority_queue.len(),
                state.visited.len()
            );
        }

        if iterations > MAX_ITERATIONS {
            eprintln!(
                "[ROUTER ERROR] Exceeded maximum iterations ({})",
                MAX_ITERATIONS
            );
            eprintln!(
                "[ROUTER ERROR] Final state: {} nodes in queue, {} visited",
                state.priority_queue.len(),
                state.visited.len()
            );
            return None;
        }
        let current = state.pop_node()?;

        // if iterations <= 10 || iterations % 10_000 == 0 {
        //     eprintln!("[ROUTER DEBUG] Iteration {}: current={:?}, goal={:?}, distance={}",
        //         iterations, current, goal_snapped, current.manhattan_distance(&goal_snapped));
        // }

        // Goal reached - reconstruct path
        if current == goal_snapped {
            eprintln!(
                "[ROUTER DEBUG] Goal reached after {} iterations!",
                iterations
            );
            let mut path = reconstruct_path(&state.came_from, start_snapped, goal_snapped);

            // v0.1.7: Path Refinement - Adjust all points to exact nanometer coordinates
            // This eliminates "stair-stepping" or "hooks" by ensuring the
            // trace stays on the exact physical Z-plane of the target layer.
            if path.len() >= 2 {
                eprintln!("[ROUTER DEBUG] Reconstructed path length: {}", path.len());
                eprintln!("[ROUTER DEBUG]   Start Voxel: {:?}", path[0]);
                eprintln!("[ROUTER DEBUG]   Goal Voxel: {:?}", path[path.len() - 1]);

                // v0.1.7: MANHATTAN ESCAPE (GOD-TIER FIX)
                // Instead of just replacing the first/last points (which creates diagonals),
                // we insert Manhattan corners to escape from the pin to the voxel grid.
                let start_snapped = path[0];
                let goal_snapped = *path.last().unwrap();

                // 1. Start Escape: start -> (start_snapped.x, start.y, start.z) -> start_snapped
                let mut final_path = Vec::with_capacity(path.len() + 4);
                final_path.push(start);
                if start.y != start_snapped.y {
                    final_path.push(Point3D::new(start.x, start_snapped.y, start.z));
                }
                if start.x != start_snapped.x {
                    final_path.push(Point3D::new(start_snapped.x, start_snapped.y, start.z));
                }

                // 2. Intermediate points
                for item in path.iter().take(path.len() - 1).skip(1) {
                    final_path.push(*item);
                }

                // 3. Goal Escape: goal_snapped -> (goal_snapped.x, goal.y, goal.z) -> goal
                if goal.x != goal_snapped.x {
                    final_path.push(Point3D::new(goal_snapped.x, goal_snapped.y, goal.z));
                }
                if goal.y != goal_snapped.y {
                    final_path.push(Point3D::new(goal.x, goal_snapped.y, goal.z));
                }
                final_path.push(goal);

                path = final_path;

                // v0.1.7 Fix: Lock ALL points to the exact physical Z-plane if requested.
                if let Some(fixed_z) = params.fixed_z_nm {
                    eprintln!(
                        "[ROUTER DEBUG]   Planar Lock Active: Locking {} points to Z={}nm",
                        path.len(),
                        fixed_z
                    );
                    for point in path.iter_mut() {
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
        // If fixed_z_nm is provided, ensure we don't move to a different Z plane.
        if let Some(fixed_z) = params.fixed_z_nm {
            if current.z != fixed_z {
                // If we somehow ended up on the wrong plane, skip it.
                continue;
            }
        }

        let current_cost = *state.cost_so_far.get(&current)?;

        // SDF ACCELERATION: Check if we can leap over empty space
        let (gx, gy, gz) = voxel_to_coords(current, params.voxel_size);
        let distance = sdf.get_distance_with_exemptions(gx, gy, gz, params.exempt_components);

        // if iterations == 1 {
        //     eprintln!("[ROUTER DEBUG] First iteration SDF query: gx={}, gy={}, gz={}, distance={}",
        //         gx, gy, gz, distance);
        //     eprintln!("[ROUTER DEBUG] SDF component_count={}",
        //         sdf.component_count());
        // }

        // CRITICAL FIX (GAP 2): For empty boards, calculate distance analytically
        // If SDF returns MAX_DISTANCE (255), the board is empty - leap directly toward goal
        let effective_distance = if distance == 255 {
            // Board is empty - distance is just the Manhattan distance to goal
            (current.manhattan_distance(&goal_snapped) / params.voxel_size.x_nm) as u8
        } else {
            distance
        };

        // if iterations <= 5 {
        //     eprintln!("[ROUTER DEBUG] Iteration {}: SDF distance={}, effective_distance={}",
        //         iterations, distance, effective_distance);
        // }

        // If we're far from obstacles (distance > 3), try to leap
        if effective_distance > 3 {
            // Calculate direction to goal
            let dx = goal_snapped.x - current.x;
            let dy = goal_snapped.y - current.y;
            let dz = goal_snapped.z - current.z;

            // Normalize to unit direction (Manhattan)
            let step_x = if dx > 0 {
                1
            } else if dx < 0 {
                -1
            } else {
                0
            };
            let step_y = if dy > 0 {
                1
            } else if dy < 0 {
                -1
            } else {
                0
            };
            // v0.1.7: Planar Lock (2.5D Routing) - No vertical leaping if locked
            let step_z = if params.fixed_z_nm.is_some() {
                0
            } else if dz > 0 {
                1
            } else if dz < 0 {
                -1
            } else {
                0
            };

            // Leap distance: min(SDF distance, distance to goal)
            let leap_dist = (effective_distance as i64)
                .min(current.manhattan_distance(&goal_snapped) / params.voxel_size.x_nm);

            if leap_dist > 1 {
                // Try to leap
                let leap_target = Point3D::new(
                    current.x + (step_x * leap_dist * params.voxel_size.x_nm),
                    current.y + (step_y * leap_dist * params.voxel_size.y_nm),
                    current.z + (step_z * leap_dist * params.voxel_size.z_nm),
                );

                // if iterations <= 5 {
                //     eprintln!("[ROUTER DEBUG] Iteration {}: LEAPING {} voxels to {:?}",
                //         iterations, leap_dist, leap_target);
                // }

                // Check if leap target is valid
                if params.bounds.contains(leap_target) && !state.visited.contains(&leap_target) {
                    let (lx, ly, lz) = voxel_to_coords(leap_target, params.voxel_size);

                    // Only leap if target is also empty (obeying exemptions)
                    // Check both SDF distance AND component interior lockout
                    let leap_blocked_by_component = if let Some(voxel_grid) = params.voxel_grid {
                        voxel_grid.point_in_component(leap_target.x, leap_target.y, leap_target.z).map(|name| {
                            // Block if inside a component that is NOT exempt
                            params.exempt_components.is_empty()
                                || !params.exempt_components.contains(&name)
                        }).unwrap_or(false)
                    } else {
                        false
                    };
                    if !leap_blocked_by_component && sdf.get_distance_with_exemptions(lx, ly, lz, params.exempt_components) > 0 {
                        let move_cost = leap_dist; // Cost is proportional to distance
                        let new_cost = current_cost + move_cost;

                        let is_better = match state.cost_so_far.get(&leap_target) {
                            Some(&old_cost) => new_cost < old_cost,
                            None => true,
                        };

                        if is_better {
                            state.cost_so_far.insert(leap_target, new_cost);
                            state.came_from.insert(leap_target, current);

                            let h = heuristic(leap_target, goal_snapped);
                            let f_score = new_cost + h;
                            state.add_node(leap_target, f_score);
                        }

                        // Continue to next iteration (skip normal neighbor expansion)
                        continue;
                    }
                }
            }
        }

        // NORMAL A*: Get neighbors in stable order (used when close to obstacles)
        let neighbors = get_neighbors_stable(
            current,
            params.bounds,
            params.layer_direction,
            params.voxel_size,
        );

        // BINARY COLLISION SKIP: Try to validate all neighbors at once
        let valid_neighbors = if let Some(grid) = params.voxel_grid {
            try_binary_collision_skip(current, &neighbors, grid, params.voxel_size)
        } else {
            None
        };

        let neighbors_to_check = if let Some(valid) = valid_neighbors {
            valid
        } else {
            neighbors
        };

        for neighbor in neighbors_to_check {
            if state.visited.contains(&neighbor) {
                continue;
            }

            // CORRIDOR CONSTRAINT: If a corridor is specified, skip neighbors outside it
            if let Some(corridor) = params.corridor {
                if !CoarseGrid::point_in_corridor(neighbor, corridor, params.voxel_size.x_nm) {
                    continue;
                }
            }

            // Hard block occupied voxels
            if neighbor != goal_snapped && params.occupied_voxels.contains(&neighbor) {
                continue;
            }

            // v0.1.7 (Strict Box Model): Block the entire interior volume of all components.
            // Exempt components containing the start or goal pins (boundary-docking).
            // This is the primary guard against routing through pad interiors — the SDF
            // alone cannot catch pads that are not registered as component_metadata.
            if let Some(voxel_grid) = params.voxel_grid {
                if let Some(component_name) =
                    voxel_grid.point_in_component(neighbor.x, neighbor.y, neighbor.z)
                {
                    if !params.exempt_components.is_empty()
                        && params.exempt_components.contains(&component_name)
                    {
                        // Exempt: this is the start or goal component
                    } else {
                        continue; // Block routing through component interior
                    }
                }
            }

            // SDF OBSTACLE DETECTION (v0.1.7): Hard block if inside a component or substrate
            // This ensures that the A* fallback (effective_distance <= 3) is not blind to obstacles
            if neighbor != goal_snapped && neighbor != start_snapped {
                let (nx, ny, nz) = voxel_to_coords(neighbor, params.voxel_size);
                if sdf.get_distance_with_exemptions(nx, ny, nz, params.exempt_components) == 0 {
                    continue;
                }
            }

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

            let is_better = match state.cost_so_far.get(&neighbor) {
                Some(&old_cost) => new_cost < old_cost,
                None => true,
            };

            if is_better {
                state.cost_so_far.insert(neighbor, new_cost);
                state.came_from.insert(neighbor, current);

                let h = heuristic(neighbor, goal_snapped);
                let f_score = new_cost + h;
                state.add_node(neighbor, f_score);
            }
        }
    }

    // No path found
    eprintln!(
        "[ROUTER ERROR] No path found after {} iterations",
        iterations
    );
    eprintln!(
        "[ROUTER ERROR] Start: {:?}, Goal: {:?}",
        start_snapped, goal_snapped
    );
    eprintln!(
        "[ROUTER ERROR] Final state: {} visited nodes",
        state.visited.len()
    );
    None
}
