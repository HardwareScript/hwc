use super::super::super::types::{NetRoute, RoutedNet, RoutingError};
use super::super::core::GeometryRouter;
use crate::geometry::Point3D;
use crate::netlist::NetId;

impl GeometryRouter {
    /// v0.1.8 Steiner Minimum Tree Decomposition (Native Auto-Routing)
    ///
    /// Decomposes a multi-pin net into point-to-point continuous routing jobs
    /// using the EntityGraph and continuous FixedTransform2D bounds to resolve
    /// terminal docking with absolute precision.
    pub fn decompose_net_steiner(
        &mut self,
        net_id: NetId,
        pins: &[Point3D],
    ) -> Result<RoutedNet, RoutingError> {
        let pin_nodes: Vec<crate::geometry_router::route_decomposition::PinNode> = pins
            .iter()
            .enumerate()
            .map(|(i, p)| crate::geometry_router::route_decomposition::PinNode {
                pin_id: i,
                component_name: String::new(),
                pin_name: String::new(),
                position: *p,
                net_id,
            })
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

        for (_i, segment) in decomposed.segments.iter().enumerate() {
            // v0.1.8: Use the exact anchor point positions from the pin metadata
            // for Steiner decomposition. These are the physical connection points.
            let start_coord = segment.from_pin.position;
            let goal_coord = segment.to_pin.position;

            // Resolve boundary docking ports on-demand using global-space coordinate checks
            let start_port = self.resolve_boundary_port(start_coord, goal_coord);
            let goal_port = self.resolve_boundary_port(goal_coord, start_coord);

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

            let routed = self.route_net_global(&route)?;
            
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
