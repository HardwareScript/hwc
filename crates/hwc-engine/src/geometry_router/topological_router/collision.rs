use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::spatial_index::DynamicSpatialIndex;

use super::TopologicalRouter;

impl TopologicalRouter {
    /// Query the spatial index for obstacle candidates overlapping a bounding box.
    pub(crate) fn query_all_obstacles(
        &self,
        bbox: &BoundingBox,
        dynamic: &DynamicSpatialIndex,
    ) -> Vec<crate::geometry_router::spatial_index::IndexedSegment> {
        dynamic.query_bbox(bbox).into_iter().cloned().collect()
    }

    /// Check if a point is inside any obstacle.
    /// v0.1.9.1 FIX: Only inflate X/Y for 2D routing, not Z.
    pub(crate) fn point_in_obstacle(
        &self,
        point: Point3D,
        obstacles: &DynamicSpatialIndex,
    ) -> bool {
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;
        // v0.1.9.1 FIX: Only expand X/Y, not Z
        let bbox = BoundingBox {
            min: Point3D::new(point.x - inflate, point.y - inflate, point.z),
            max: Point3D::new(point.x + inflate, point.y + inflate, point.z),
        };
        let candidates = self.query_all_obstacles(&bbox, obstacles);
        for seg in candidates {
            if self.exempt_net_ids.contains(&(seg.net_id.raw() as usize)) {
                continue;
            }
            let seg_bbox = BoundingBox {
                min: Point3D::new(
                    seg.start.x.min(seg.end.x) - seg.width_nm / 2,
                    seg.start.y.min(seg.end.y) - seg.width_nm / 2,
                    seg.start.z.min(seg.end.z),
                ),
                max: Point3D::new(
                    seg.start.x.max(seg.end.x) + seg.width_nm / 2,
                    seg.start.y.max(seg.end.y) + seg.width_nm / 2,
                    seg.start.z.max(seg.end.z),
                ),
            };
            if seg_bbox.contains(point) {
                return true;
            }
        }
        false
    }

    /// Check if a segment between two points intersects any obstacle.
    /// Uses Minkowski sum inflation: inflate_by = trace_width_nm / 2 + min_clearance_nm
    /// NOTE: For single-layer 2D routing, we only inflate X/Y dimensions, not Z.
    ///
    /// VERTICAL VIA EXEMPTION (v0.2.0):
    /// If the segment is vertical (start.z != end.z), it represents a via transition.
    /// Vias are exempt from collision checks with obstacles on intermediate Z layers,
    /// as they must pass through non-routable dielectric layers by definition.
    pub(crate) fn segment_intersects_obstacle(
        &self,
        a: Point3D,
        b: Point3D,
        obstacles: &DynamicSpatialIndex,
    ) -> bool {
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;

        let route_z_min = a.z.min(b.z);
        let route_z_max = a.z.max(b.z);

        // VERTICAL VIA EXEMPTION: Check if this is a vertical via
        let is_vertical_via = a.z != b.z;

        let segment_bbox = BoundingBox {
            min: Point3D::new(a.x.min(b.x), a.y.min(b.y), route_z_min),
            max: Point3D::new(a.x.max(b.x), a.y.max(b.y), route_z_max),
        };

        // v0.1.9.1 FIX: Only expand X/Y for query, not Z.
        // For single-layer 2D routing, all traces and obstacles are at the same Z (middle_z).
        // Expanding Z by inflate causes false collisions with obstacles at different layers.
        let query_bbox = BoundingBox {
            min: Point3D::new(
                segment_bbox.min.x - inflate,
                segment_bbox.min.y - inflate,
                segment_bbox.min.z, // Don't inflate Z
            ),
            max: Point3D::new(
                segment_bbox.max.x + inflate,
                segment_bbox.max.y + inflate,
                segment_bbox.max.z, // Don't inflate Z
            ),
        };

        let candidates = self.query_all_obstacles(&query_bbox, obstacles);

        for seg in candidates.iter() {
            if self.exempt_net_ids.contains(&(seg.net_id.raw() as usize)) {
                continue;
            }

            let obs_z_min = seg.start.z.min(seg.end.z);
            let obs_z_max = seg.start.z.max(seg.end.z);

            if route_z_min > obs_z_max || obs_z_min > route_z_max {
                continue;
            }

            // VERTICAL VIA EXEMPTION: If this is a vertical via and the obstacle is on
            // an intermediate layer (not at the via endpoints), skip collision check.
            // This allows vias to pass through non-routable dielectric layers.
            if is_vertical_via {
                let _is_intermediate_layer = (obs_z_min > route_z_min && obs_z_max < route_z_max)
                    || (obs_z_min >= route_z_min
                        && obs_z_max <= route_z_max
                        && obs_z_min != route_z_min
                        && obs_z_max != route_z_max);

                // Only check collision if obstacle is on the same layer as via endpoints
                if obs_z_min != route_z_min && obs_z_max != route_z_max {
                    continue; // Skip intermediate layer obstacles
                }
            }

            let obs_bbox = BoundingBox {
                min: Point3D::new(
                    seg.start.x.min(seg.end.x) - seg.width_nm / 2,
                    seg.start.y.min(seg.end.y) - seg.width_nm / 2,
                    obs_z_min,
                ),
                max: Point3D::new(
                    seg.start.x.max(seg.end.x) + seg.width_nm / 2,
                    seg.start.y.max(seg.end.y) + seg.width_nm / 2,
                    obs_z_max,
                ),
            };

            // v0.1.9.1 FIX: Only inflate X/Y, not Z, for single-layer 2D routing
            let inflated_segment = BoundingBox {
                min: Point3D::new(
                    segment_bbox.min.x - inflate,
                    segment_bbox.min.y - inflate,
                    segment_bbox.min.z, // Don't inflate Z
                ),
                max: Point3D::new(
                    segment_bbox.max.x + inflate,
                    segment_bbox.max.y + inflate,
                    segment_bbox.max.z, // Don't inflate Z
                ),
            };

            let x_overlaps = inflated_segment.min.x <= obs_bbox.max.x
                && inflated_segment.max.x >= obs_bbox.min.x;
            let y_overlaps = inflated_segment.min.y <= obs_bbox.max.y
                && inflated_segment.max.y >= obs_bbox.min.y;
            let z_overlaps = inflated_segment.min.z <= obs_bbox.max.z
                && inflated_segment.max.z >= obs_bbox.min.z;

            if x_overlaps && y_overlaps && z_overlaps {
                return true;
            }
        }

        false
    }
}
