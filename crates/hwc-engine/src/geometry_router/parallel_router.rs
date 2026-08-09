//! Parallel Router: Multi-threaded domain-based routing with std::thread::scope.
//!
//! Uses TopologicalRouter as the sole routing engine within isolated domains.

use crate::constraint_manager::{ConstraintRulebook, Route, RoutedDomain, RoutingDomain};
use crate::geometry::Point3D;
use crate::geometry::{BoundingBox, TraceSegment};
use crate::geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment};
use crate::geometry_router::topological_router::TopologicalRouter;
use crate::geometry_router::EntityGraph;
use crate::netlist::NetId;
use crate::netlist::NetlistArena;

pub struct ParallelRouter {
    constraints: ConstraintRulebook,
    manufacturing_grid_nm: i64,
}

impl ParallelRouter {
    pub fn new(constraints: ConstraintRulebook) -> Self {
        let manufacturing_grid_nm = constraints.manufacturing_grid_nm;
        Self {
            constraints,
            manufacturing_grid_nm,
        }
    }

    pub fn route_domains(
        &self,
        domains: Vec<RoutingDomain>,
        netlist: &NetlistArena,
    ) -> Vec<RoutedDomain> {
        // Domains are naturally coarse-grained (typically 2-8 domains).
        // One thread per domain via std::thread::scope is optimal: each thread
        // owns its domain data (no Arc/Mutex needed) and there is no work-stealing
        // overhead for small task counts.
        let constraints = &self.constraints;
        let manufacturing_grid_nm = self.manufacturing_grid_nm;
        std::thread::scope(|s| {
            let mut handles = Vec::new();

            for domain in domains {
                let handle = s.spawn(move || {
                    let local_routes =
                        Self::route_internal_nets(&domain, netlist, constraints, manufacturing_grid_nm);

                    let grid_chunk = EntityGraph::new();

                    // v0.1.8: Occupancy tracking is now handled by the EntityGraph.
                    // No need to copy occupancy data — the TopologicalRouter uses DynamicSpatialIndex.

                    RoutedDomain {
                        id: domain.domain_id.clone(),
                        box_offset: domain.bounding_box.min,
                        routes: local_routes,
                        grid_chunk,
                    }
                });
                handles.push(handle);
            }

            handles.into_iter().map(|h| h.join().unwrap()).collect()
        })
    }

    fn route_internal_nets(
        domain: &RoutingDomain,
        netlist: &NetlistArena,
        _constraints: &ConstraintRulebook,
        manufacturing_grid_nm: i64,
    ) -> Vec<Route> {
        let mut routes = Vec::new();

        let (width, height, depth) = domain.dimensions();

        let board_bounds =
            BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(width, height, depth));

        let mut spatial_index = DynamicSpatialIndex::new();

        for (seg_id, meta) in domain
            .local_grid
            .get_component_metadata()
            .iter()
            .enumerate()
        {
            let w = meta.bbox.max.x - meta.bbox.min.x;
            let h = meta.bbox.max.y - meta.bbox.min.y;
            let trace_seg =
                TraceSegment::new(meta.bbox.min, meta.bbox.max, w.max(h), meta.material);
            let thickness_nm = meta.bbox.max.z - meta.bbox.min.z;
            let component_net_id = meta
                .net_bindings
                .values()
                .next()
                .copied()
                .unwrap_or(NetId::UNCONNECTED)
                .raw() as usize;
            spatial_index.insert(IndexedSegment {
                source: hwc_physics::spatial_index::SpatialEntitySource::ComponentInstance {
                    instance_id: seg_id,
                },
                segment_id: seg_id,
                net_id: NetId::new(component_net_id as u32),
                width_nm: trace_seg.width_nm,
                thickness_nm,
                start: trace_seg.start,
                end: trace_seg.end,
                layer: meta.bbox.min.z,
                device_binding: None, // Component instances don't have device bindings yet
            });
        }

        let fab = _constraints.fabrication.as_ref().expect(
            "ParallelRouter: fabrication constraints are required but none are loaded. \
             Declare a profile with 'trace:' and 'clearance:' constraints.",
        );

        let topo_router = TopologicalRouter::new(
            fab.min_trace_width_nm,
            manufacturing_grid_nm,
            fab.min_trace_spacing_nm,
        );

        for &net_id in &domain.internal_nets {
            let net_data = match netlist.get_net(net_id) {
                Some(data) => data,
                None => continue,
            };

            let pins = &net_data.pins;
            if pins.len() < 2 {
                continue;
            }

            let start_pin = pins[0];
            let start_pos_global = match netlist.get_pin_position(start_pin) {
                Some((x, y, z)) => Point3D::new(x, y, z),
                None => continue,
            };

            let start_local = domain.global_to_local(start_pos_global);

            for &end_pin in &pins[1..] {
                let end_pos_global = match netlist.get_pin_position(end_pin) {
                    Some((x, y, z)) => Point3D::new(x, y, z),
                    None => continue,
                };

                let end_local = domain.global_to_local(end_pos_global);

                if let Some(topo_path) =
                    topo_router.route(start_local, end_local, &spatial_index, &board_bounds)
                {
                    if topo_path.waypoints.len() >= 2 {
                        let waypoints = topo_path.waypoints;
                        routes.push(Route { net_id, waypoints });
                    }
                }
            }
        }

        routes
    }
}
