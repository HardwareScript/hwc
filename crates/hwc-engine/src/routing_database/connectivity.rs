//! Unified connectivity view over child and parent routes.
//!
//! Child and parent routes are never stored merged. This module performs the
//! lazy, on-demand merge used by connectivity validation, preserving
//! provenance so failures can be reported against their original source.

use super::database::HierarchicalRoutingDatabase;
use super::ids::RouteId;
use super::provenance::{ProvenanceSegment, RouteSource};
use crate::geometry::TraceSegment;

impl HierarchicalRoutingDatabase {
    /// Get unified connectivity view for validation
    ///
    /// This merges child and parent routes into a single list for connectivity
    /// checking, while preserving provenance information for error reporting.
    pub fn get_connectivity_view(&self) -> Vec<ProvenanceSegment> {
        let mut segments = Vec::new();
        let mut route_id = 0u64;

        // Add child instance routes
        for ((instance, net_id), route_segs) in &self.child_instance_routes {
            for seg in route_segs {
                let id = RouteId::new(route_id);
                route_id += 1;

                let source = self.route_provenance.get(&id).cloned().unwrap_or_else(|| {
                    RouteSource::ChildInstance {
                        instance: instance.clone(),
                        original_net: "unknown".into(),
                    }
                });

                segments.push(ProvenanceSegment {
                    net_id: *net_id,
                    net_name: None, // Will be filled by caller if needed
                    segment: seg.clone(),
                    source,
                    route_id: id,
                });
            }
        }

        // Add parent interconnect routes
        for trace in &self.parent_interconnects {
            for seg_line in &trace.segments {
                let id = RouteId::new(route_id);
                route_id += 1;

                let source = self.route_provenance.get(&id).cloned().unwrap_or_else(|| {
                    RouteSource::ParentLevel {
                        from_entity: "unknown".into(),
                        to_entity: "unknown".into(),
                    }
                });

                // Convert LineSegment to TraceSegment
                let trace_seg = TraceSegment::new(
                    seg_line.start,
                    seg_line.end,
                    trace.cross_section.width_nm,
                    trace.material,
                );

                segments.push(ProvenanceSegment {
                    net_id: trace.net_id,
                    net_name: Some(trace.net_name.clone()),
                    segment: trace_seg,
                    source,
                    route_id: id,
                });
            }
        }

        segments
    }
}
