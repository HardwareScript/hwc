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
    pub(crate) fn point_in_obstacle(
        &self,
        point: Point3D,
        obstacles: &DynamicSpatialIndex,
    ) -> bool {
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;
        let bbox = BoundingBox::from_point(point, inflate);
        let candidates = self.query_all_obstacles(&bbox, obstacles);
        for seg in candidates {
            if self.exempt_net_ids.contains(&seg.net_id) {
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
    pub(crate) fn segment_intersects_obstacle(
        &self,
        a: Point3D,
        b: Point3D,
        obstacles: &DynamicSpatialIndex,
    ) -> bool {
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;

        let route_z_min = a.z.min(b.z);
        let route_z_max = a.z.max(b.z);

        let segment_bbox = BoundingBox {
            min: Point3D::new(a.x.min(b.x), a.y.min(b.y), route_z_min),
            max: Point3D::new(a.x.max(b.x), a.y.max(b.y), route_z_max),
        };

        let query_bbox = segment_bbox.expand(inflate);

        eprintln!(
            "[TOPO COLLISION] Checking segment ({},{},{}) to ({},{},{}) with inflate={}nm",
            a.x, a.y, a.z, b.x, b.y, b.z, inflate
        );
        eprintln!(
            "[TOPO COLLISION] Segment bbox: ({},{},{}) to ({},{},{})",
            segment_bbox.min.x,
            segment_bbox.min.y,
            segment_bbox.min.z,
            segment_bbox.max.x,
            segment_bbox.max.y,
            segment_bbox.max.z
        );

        let candidates = self.query_all_obstacles(&query_bbox, obstacles);

        for (idx, seg) in candidates.iter().enumerate() {
            if self.exempt_net_ids.contains(&seg.net_id) {
                eprintln!(
                    "[TOPO COLLISION]   Obstacle {}: SKIPPED (exempt net_id={})",
                    idx, seg.net_id
                );
                continue;
            }

            let obs_z_min = seg.start.z.min(seg.end.z);
            let obs_z_max = seg.start.z.max(seg.end.z);

            if route_z_min > obs_z_max || obs_z_min > route_z_max {
                continue;
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

            eprintln!(
                "[TOPO COLLISION]   Obstacle {}: net_id={}, bbox=({},{},{}) to ({},{},{})",
                idx,
                seg.net_id,
                obs_bbox.min.x,
                obs_bbox.min.y,
                obs_bbox.min.z,
                obs_bbox.max.x,
                obs_bbox.max.y,
                obs_bbox.max.z
            );

            let inflated_segment = segment_bbox.expand(inflate);

            let x_overlaps = inflated_segment.min.x <= obs_bbox.max.x
                && inflated_segment.max.x >= obs_bbox.min.x;
            let y_overlaps = inflated_segment.min.y <= obs_bbox.max.y
                && inflated_segment.max.y >= obs_bbox.min.y;
            let z_overlaps = inflated_segment.min.z <= obs_bbox.max.z
                && inflated_segment.max.z >= obs_bbox.min.z;

            if x_overlaps && y_overlaps && z_overlaps {
                eprintln!(
                    "[TOPO COLLISION]   COLLISION DETECTED with Obstacle {}",
                    idx
                );
                eprintln!(
                    "[TOPO COLLISION]      Inflated segment: ({},{},{}) to ({},{},{})",
                    inflated_segment.min.x,
                    inflated_segment.min.y,
                    inflated_segment.min.z,
                    inflated_segment.max.x,
                    inflated_segment.max.y,
                    inflated_segment.max.z
                );
                return true;
            }
        }

        eprintln!("[TOPO COLLISION] No collisions detected");
        false
    }
}
