//! Export of database contents as flat `TraceSegment` lists.
//!
//! **v0.2.2 ARCHITECTURAL FIX: Direct Layer Lineage**
//!
//! Routes store their layer name explicitly. Materials and Z-coordinates are
//! looked up directly from the `RoutingLayerDatabase`, eliminating the old
//! reverse spatial (Z-coordinate) guessing.
//!
//! NOTE: The old `export_as_routed_segments_with_stackup()` method has been
//! REMOVED. It used a single hardcoded material ID for entire traces, causing
//! incorrect material assignment for multi-layer routes. All call sites must
//! use [`HierarchicalRoutingDatabase::export_as_routed_segments_with_lineage`],
//! which performs direct layer lineage lookup.

use super::database::HierarchicalRoutingDatabase;
use crate::geometry::TraceSegment;
use crate::netlist::NetId;
use crate::routing_layer_database::RoutingLayerDatabase;
use rustc_hash::FxHashMap;

impl HierarchicalRoutingDatabase {
    /// Export for legacy `entity_graph.routed_segments()` compatibility.
    ///
    /// Export all routes (child + parent) as `TraceSegment`s with proper
    /// per-segment material lookup.
    ///
    /// # Arguments
    /// * `routing_layer_db` - Layer database for direct material/Z lookup by layer name
    ///
    /// # Returns
    /// Vector of (net_id, segments) tuples where each segment has the correct material_id
    /// derived directly from its layer assignment.
    pub fn export_as_routed_segments_with_lineage(
        &self,
        routing_layer_db: &RoutingLayerDatabase,
    ) -> Vec<(NetId, Vec<TraceSegment>)> {
        let mut net_segments: FxHashMap<NetId, Vec<TraceSegment>> = FxHashMap::default();

        // Add child routes (these already have correct materials from flattening)
        for ((_, net_id), segments) in &self.child_instance_routes {
            net_segments
                .entry(*net_id)
                .or_default()
                .extend(segments.clone());
        }

        // Add parent routes with DIRECT LAYER LINEAGE LOOKUP
        for trace in &self.parent_interconnects {
            // ARCHITECTURAL FIX: Look up layer definition directly by name stored in trace
            let layer_def = match routing_layer_db.get_layer(&trace.layer_name) {
                Ok(layer) => layer,
                Err(e) => {
                    eprintln!(
                        "[ROUTING DB EXPORT ERROR] Route on net {:?} references unknown layer '{}': {:?}",
                        trace.net_id, trace.layer_name, e
                    );
                    continue; // Skip routes with invalid layer references
                }
            };

            // Direct lineage: Material ID comes from the layer definition, not reverse Z-lookup!
            let material_id = layer_def.material_id;

            let segments: Vec<TraceSegment> = trace
                .segments
                .iter()
                .map(|line_seg| {
                    TraceSegment::new(
                        line_seg.start,
                        line_seg.end,
                        trace.cross_section.width_nm,
                        material_id, // DIRECT: From layer definition, not spatial guessing
                    )
                })
                .collect();

            net_segments
                .entry(trace.net_id)
                .or_default()
                .extend(segments);
        }

        net_segments.into_iter().collect()
    }

    /// **v0.2.3: Get parent segments for hierarchical legalization**
    ///
    /// Returns mutable parent interconnect segments with net IDs.
    /// These segments can be nudged during legalization.
    pub fn get_parent_segments_for_legalization(
        &self,
        routing_layer_db: &RoutingLayerDatabase,
    ) -> (Vec<TraceSegment>, Vec<NetId>) {
        let mut segments = Vec::new();
        let mut net_ids = Vec::new();

        for trace in &self.parent_interconnects {
            let layer_def = match routing_layer_db.get_layer(&trace.layer_name) {
                Ok(layer) => layer,
                Err(_) => continue,
            };

            let material_id = layer_def.material_id;

            for line_seg in &trace.segments {
                segments.push(TraceSegment::new(
                    line_seg.start,
                    line_seg.end,
                    trace.cross_section.width_nm,
                    material_id,
                ));
                net_ids.push(trace.net_id);
            }
        }

        (segments, net_ids)
    }

    /// **v0.2.3: Get child segments for hierarchical legalization**
    ///
    /// Returns immutable child instance segments with net IDs.
    /// These segments are marked as frozen and treated as static obstacles.
    pub fn get_child_segments_for_legalization(&self) -> (Vec<TraceSegment>, Vec<NetId>) {
        let mut segments = Vec::new();
        let mut net_ids = Vec::new();

        for ((_, net_id), segs) in &self.child_instance_routes {
            for seg in segs {
                // Mark child segments as frozen (immutable)
                let mut frozen_seg = seg.clone();
                frozen_seg.is_frozen = true;
                segments.push(frozen_seg);
                net_ids.push(*net_id);
            }
        }

        (segments, net_ids)
    }

    /// **v0.2.3: Update parent segments after legalization**
    ///
    /// Replaces parent interconnect segments with legalized versions.
    /// Maintains trace metadata (net_id, net_name, material, current, etc.)
    pub fn update_parent_segments_after_legalization(
        &mut self,
        legalized_segments: Vec<TraceSegment>,
        net_ids: Vec<NetId>,
        _routing_layer_db: &RoutingLayerDatabase,
    ) {
        if legalized_segments.len() != net_ids.len() {
            eprintln!(
                "[ROUTING DB WARNING] Segment/NetID count mismatch in legalization update: {} segments, {} net_ids",
                legalized_segments.len(),
                net_ids.len()
            );
            return;
        }

        // Group legalized segments by net
        let mut segments_by_net: FxHashMap<NetId, Vec<TraceSegment>> = FxHashMap::default();
        for (seg, net_id) in legalized_segments.into_iter().zip(net_ids.iter()) {
            segments_by_net.entry(*net_id).or_default().push(seg);
        }

        // Update each parent trace with legalized segments
        for trace in &mut self.parent_interconnects {
            if let Some(new_segments) = segments_by_net.remove(&trace.net_id) {
                // Convert TraceSegment back to LineSegment
                trace.segments = new_segments
                    .into_iter()
                    .map(|ts| crate::space::LineSegment::new(ts.start, ts.end))
                    .collect();

                eprintln!(
                    "[ROUTING DB] Updated net {:?} with {} legalized segments",
                    trace.net_id,
                    trace.segments.len()
                );
            }
        }
    }
}
