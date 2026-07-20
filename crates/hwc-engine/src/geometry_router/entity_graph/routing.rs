//! Route registration and query methods for EntityGraph.

use crate::geometry::TraceSegment;
use crate::netlist::NetId;

use super::EntityGraph;

impl EntityGraph {
    /// Commit the current routing session (placeholder).
    pub fn commit_route(&mut self) {}

    /// Register a routed path canonically as continuous vector segments.
    pub fn register_route(
        &mut self,
        net_id: NetId,
        waypoints: &[crate::geometry::Point3D],
        material_id: u8,
        width_nm: i64,
    ) {
        self.register_route_with_z_materials(
            net_id,
            waypoints,
            material_id,
            width_nm,
            None::<fn(i64) -> Option<u8>>,
        )
    }

    /// Register a route with Z-aware material resolution (v0.1.9.1)
    pub fn register_route_with_z_materials<F>(
        &mut self,
        net_id: NetId,
        waypoints: &[crate::geometry::Point3D],
        default_material_id: u8,
        width_nm: i64,
        z_to_material: Option<F>,
    ) where
        F: Fn(i64) -> Option<u8>,
    {
        if waypoints.len() < 2 {
            return;
        }

        let deduped: Vec<crate::geometry::Point3D> = waypoints
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, p)| *i == 0 || *p != waypoints[i - 1])
            .map(|(_, p)| p)
            .collect();

        if deduped.len() < 2 {
            return;
        }

        let segments: Vec<TraceSegment> = deduped
            .windows(2)
            .filter(|w| w[0] != w[1])
            .map(|w| {
                let start = w[0];
                let end = w[1];

                let seg_material_id = if start.z == end.z {
                    if let Some(ref resolver) = z_to_material {
                        resolver(start.z).unwrap_or(default_material_id)
                    } else {
                        default_material_id
                    }
                } else {
                    default_material_id
                };

                TraceSegment::new(start, end, width_nm, seg_material_id)
            })
            .collect();

        if segments.is_empty() {
            return;
        }

        if let Some(entry) = self
            .routed_segments
            .iter_mut()
            .find(|(id, _)| *id == net_id)
        {
            entry.1.extend(segments);
        } else {
            self.routed_segments.push((net_id, segments));
        }
    }

    /// Register pre-built trace segments directly (for lockfile loading).
    pub fn register_trace_segments(&mut self, net_id: NetId, segments: Vec<TraceSegment>) {
        if segments.is_empty() {
            return;
        }

        if let Some(entry) = self
            .routed_segments
            .iter_mut()
            .find(|(id, _)| *id == net_id)
        {
            entry.1.extend(segments);
        } else {
            self.routed_segments.push((net_id, segments));
        }
    }

    /// Get all canonically registered route segments across all nets.
    pub fn get_all_routes(&self) -> &[(NetId, Vec<TraceSegment>)] {
        &self.routed_segments
    }

    /// Clear registered route segments for a specific net.
    pub fn clear_routes_for_net(&mut self, net_id: NetId) {
        self.routed_segments.retain(|(id, _)| *id != net_id);
    }

    /// Register a single point as occupied by a net (for polygon rasterization).
    pub fn occupy_point(
        &mut self,
        point: crate::geometry::Point3D,
        net_id: NetId,
        material: crate::geometry_router::substrate_types::MaterialId,
    ) {
        let segment = TraceSegment::new(point, point, 0, material);
        if let Some(entry) = self
            .routed_segments
            .iter_mut()
            .find(|(id, _)| *id == net_id)
        {
            entry.1.push(segment);
        } else {
            self.routed_segments.push((net_id, vec![segment]));
        }
    }
}
