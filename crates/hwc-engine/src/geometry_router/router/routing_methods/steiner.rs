use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::Point3D;
use crate::geometry_router::EntityGraph;
use crate::netlist::NetId;

impl GeometryRouter {
    /// v0.1.8 Steiner Minimum Tree Decomposition (Native Auto-Routing)
    ///
    /// Decomposes a multi-pin net into point-to-point continuous routing jobs
    /// using the EntityGraph and continuous FixedTransform2D bounds to resolve
    /// terminal docking with absolute precision.
    pub fn decompose_net_steiner(
        &mut self,
        entity_graph: &mut EntityGraph,
        net_id: NetId,
        pins: &[Point3D],
    ) -> Result<RoutedNet, RoutingError> {
        let pin_nodes: Vec<crate::geometry_router::route_decomposition::PinNode> = pins
            .iter()
            .enumerate()
            .map(
                |(i, p)| crate::geometry_router::route_decomposition::PinNode {
                    pin_id: i,
                    component_name: String::new(),
                    pin_name: String::new(),
                    position: *p,
                    net_id,
                },
            )
            .collect();

        // Solve the Minimum Spanning Tree (MST) in continuous coordinate space
        let mut next_seg_id = 0usize;
        let mut next_junc_id = 0usize;
        let decomposed = crate::geometry_router::route_decomposition::decompose_net(
            net_id,
            pin_nodes,
            &mut next_seg_id,
            &mut next_junc_id,
        );

        // eprintln!("[DEBUG STEINER] Decomposed net {:?} into {} segments", net_id, decomposed.segments.len());

        let mut net_paths = Vec::new();
        let mut all_vias = Vec::new();

        // Check if this net has a routing pattern policy for length matching
        let has_pattern = self.route_net_policies.contains_key(&net_id);

        // v0.1.9: Get trace width for this net (needed for port escape clearance calculation)
        let trace_width = self.net_trace_widths.get(&net_id).copied().ok_or_else(|| {
            RoutingError::MissingFabricationConstraints {
                net_id,
                message: format!(
                    "No trace width declared for net_id={}. Every route must have an explicit \
                     'width:' parameter or the space must provide a default trace width.",
                    net_id.raw()
                ),
            }
        })?;

        for segment in decomposed.segments.iter() {
            // v0.1.8: Use the exact anchor point positions from the pin metadata
            // for Steiner decomposition. These are the physical connection points.
            let start_coord = segment.from_pin.position;
            let goal_coord = segment.to_pin.position;

            // Resolve boundary docking ports on-demand using global-space coordinate checks
            let start_port =
                self.resolve_boundary_port(entity_graph, start_coord, goal_coord, trace_width);
            let goal_port =
                self.resolve_boundary_port(entity_graph, goal_coord, start_coord, trace_width);

            // v0.1.8: Ensure ports are on the same Z layer.
            // In a vector-first system, traces must be coplanar with the pad anchor.
            let final_start = start_port;
            let mut final_goal = goal_port;

            if final_start.z != final_goal.z {
                // eprintln!("[DEBUG STEINER] Port Z mismatch: start.z={} goal.z={}. Forcing goal to start.z", final_start.z, final_goal.z);
                final_goal.z = final_start.z;
            }
            // Removed the unconditional forcing to 0 (top copper) to support multi-layer
            // and correct alignment with pad anchor points.

            // eprintln!(
            //     "[DEBUG STEINER] Routing segment {}: {:?} -> {:?}",
            //     i, final_start, final_goal
            // );

            let route = NetRoute {
                net_id,
                start: final_start,
                goal: final_goal,
            };

            // v0.1.9: If net has a routing pattern, use length-constrained routing
            // which will inject meander after the straight route if needed.
            let routed = if has_pattern {
                let pattern = self.route_net_policies.get(&net_id).cloned();
                // Target length = Manhattan distance between endpoints (minimum possible)
                let target_length = (final_start.x - final_goal.x).abs()
                    + (final_start.y - final_goal.y).abs()
                    + (final_start.z - final_goal.z).abs();
                self.route_net_with_length_constraint(
                    entity_graph,
                    &route,
                    target_length,
                    &pattern,
                )?
            } else {
                self.route_net_global(entity_graph, &route)?
            };

            // v0.1.8: Stitch the pin positions to the routed path to ensure physical
            // continuity and prevent gaps between the pad and the trace.
            let mut segment_path = vec![segment.from_pin.position];
            if let Some(path) = routed.paths.into_iter().next() {
                // Ensure the path starts at start_port and ends at goal_port
                // (Already guaranteed by route_net_global, but good to be explicit)
                if !path.is_empty() {
                    segment_path.extend(path);
                }
            }
            segment_path.push(segment.to_pin.position);

            net_paths.push(segment_path);
            all_vias.extend(routed.vias);
        }

        Ok(RoutedNet {
            net_id,
            paths: net_paths,
            vias: all_vias,
        })
    }
}
