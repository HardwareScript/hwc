//! Spatial queries and bounding box methods for EntityGraph.

use crate::geometry::BoundingBox;

use super::EntityGraph;

impl EntityGraph {
    /// Get the bounding box of all instances combined.
    pub fn total_bounding_box(&self) -> Option<BoundingBox> {
        let mut result: Option<BoundingBox> = None;

        let instances = self.scene.instances();
        for inst in instances {
            result = Some(match result {
                Some(r) => r.union(&inst.global_bbox),
                None => inst.global_bbox,
            });
        }

        for layer in &self.substrate_layers {
            result = Some(match result {
                Some(r) => r.union(&layer.bbox),
                None => layer.bbox,
            });
        }

        result
    }

    /// Query: is the point (x, y, z) inside any component's physical geometry?
    #[inline]
    pub fn is_point_occupied(&self, x: i64, y: i64, _z: i64) -> bool {
        for inst in self.scene.instances() {
            if x < inst.global_bbox.min.x
                || x > inst.global_bbox.max.x
                || y < inst.global_bbox.min.y
                || y > inst.global_bbox.max.y
            {
                continue;
            }
            if inst.test_collision_global(x, y) {
                return true;
            }
        }
        false
    }

    /// Configure physical layer Z-ranges on the spatial index for layered queries.
    pub fn set_spatial_layer_z_ranges(&mut self, z_ranges: &[(i64, i64)]) {
        self.spatial.set_layer_z_ranges(z_ranges);
    }

    /// Get a reference to all component metadata.
    pub fn get_component_metadata(
        &self,
    ) -> &[crate::geometry_router::substrate_types::ComponentMetadata] {
        &self.component_metadata
    }

    /// Query the global spatial index for elements within a bounding box.
    pub fn query_bbox(
        &self,
        bbox: &BoundingBox,
    ) -> Vec<crate::geometry_router::substrate_types::SubstrateLayer> {
        let candidates = self.spatial.query_bbox(bbox);
        let mut results = Vec::new();

        for cand in candidates {
            match cand.source {
                hwc_physics::spatial_index::SpatialEntitySource::SubstrateLayer { index } => {
                    if let Some(layer) = self.substrate_layers.get(index) {
                        results.push(layer.clone());
                    }
                }
                hwc_physics::spatial_index::SpatialEntitySource::RouteSegment {
                    net_idx,
                    seg_idx,
                } => {
                    if let Some((net_id, segments)) = self.routed_segments.get(net_idx) {
                        if let Some(seg) = segments.get(seg_idx) {
                            let seg_bbox = BoundingBox::new(seg.start, seg.end);
                            let layer = crate::geometry_router::substrate_types::SubstrateLayer::new(
                                seg.material_id,
                                net_id.raw(),
                                seg_bbox,
                                crate::geometry_router::substrate_types::SubstrateLayerType::Pour,
                            );
                            results.push(layer);
                        }
                    }
                }
                hwc_physics::spatial_index::SpatialEntitySource::ComponentInstance { .. } => {}
            }
        }

        if results.is_empty() && self.spatial.is_empty() {
            for layer in &self.substrate_layers {
                if layer.bbox.intersects(bbox) {
                    results.push(layer.clone());
                }
            }
        }

        results
    }
}
