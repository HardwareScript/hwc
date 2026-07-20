use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::spatial_index::DynamicSpatialIndex;

use super::types::{RayPathQuery, TopologicalPath};
use super::TopologicalRouter;

impl TopologicalRouter {
    /// Build a complete path from start to target through a meeting point using Z-route.
    pub(crate) fn build_z_path(
        &self,
        start: Point3D,
        target: Point3D,
        meeting: Point3D,
        obstacles: &DynamicSpatialIndex,
        _board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        if self.segment_intersects_obstacle(start, meeting, obstacles)
            || self.segment_intersects_obstacle(meeting, target, obstacles)
        {
            return None;
        }

        let mut waypoints = vec![start];
        if start.x != meeting.x && start.y != meeting.y {
            waypoints.push(Point3D::new(meeting.x, start.y, start.z));
        }
        waypoints.push(meeting);
        if target.x != meeting.x && target.y != meeting.y {
            waypoints.push(Point3D::new(meeting.x, target.y, target.z));
        }
        waypoints.push(target);

        let total_length = waypoints
            .windows(2)
            .map(|w| w[0].manhattan_distance(&w[1]))
            .sum();

        Some(TopologicalPath {
            waypoints,
            total_length,
        })
    }

    /// Build a path from start to target using rays from both endpoints.
    pub(crate) fn build_path_from_rays(&self, q: RayPathQuery) -> Option<TopologicalPath> {
        let RayPathQuery {
            start,
            target,
            s_ray,
            t_ray,
            meeting,
            obstacles,
            board_bounds: _board_bounds,
        } = q;
        let mut waypoints = Vec::new();
        waypoints.push(start);

        let s_bend = if s_ray.direction.is_horizontal() {
            Point3D::new(meeting.x, start.y, start.z)
        } else {
            Point3D::new(start.x, meeting.y, start.z)
        };
        if s_bend != start && s_bend != meeting {
            waypoints.push(s_bend);
        }

        waypoints.push(meeting);

        let t_bend = if t_ray.direction.is_horizontal() {
            Point3D::new(meeting.x, target.y, target.z)
        } else {
            Point3D::new(target.x, meeting.y, target.z)
        };
        if t_bend != meeting && t_bend != target {
            waypoints.push(t_bend);
        }

        waypoints.push(target);

        for pair in waypoints.windows(2) {
            if self.segment_intersects_obstacle(pair[0], pair[1], obstacles) {
                return None;
            }
        }

        let total_length = waypoints
            .windows(2)
            .map(|w| w[0].manhattan_distance(&w[1]))
            .sum();

        Some(TopologicalPath {
            waypoints,
            total_length,
        })
    }

    /// Try a direct L-shaped or straight path between two points.
    pub(crate) fn try_direct_path(
        &self,
        start: Point3D,
        target: Point3D,
        obstacles: &DynamicSpatialIndex,
        _board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        eprintln!(
            "[TOPO try_direct_path] Attempting direct paths from ({},{},{}) to ({},{},{})",
            start.x, start.y, start.z, target.x, target.y, target.z
        );

        if start.x == target.x || start.y == target.y {
            let waypoints = vec![start, target];
            let collides = self.segment_intersects_obstacle(start, target, obstacles);
            eprintln!(
                "[TOPO try_direct_path] Straight line: collides={}",
                collides
            );
            if !collides {
                let total_length = start.manhattan_distance(&target);
                eprintln!("[TOPO try_direct_path] Returning straight path");
                return Some(TopologicalPath {
                    waypoints,
                    total_length,
                });
            }
        }

        let bend_hv = Point3D::new(target.x, start.y, start.z);
        let hv_seg1_collides = self.segment_intersects_obstacle(start, bend_hv, obstacles);
        let hv_seg2_collides = self.segment_intersects_obstacle(bend_hv, target, obstacles);
        eprintln!("[TOPO try_direct_path] L-shape (H then V): bend=({},{},{}), seg1_collides={}, seg2_collides={}",
            bend_hv.x, bend_hv.y, bend_hv.z, hv_seg1_collides, hv_seg2_collides);
        if !hv_seg1_collides && !hv_seg2_collides {
            let total_length = start.manhattan_distance(&target);
            eprintln!("[TOPO try_direct_path] Returning L-shape (H then V)");
            return Some(TopologicalPath {
                waypoints: vec![start, bend_hv, target],
                total_length,
            });
        }

        let bend_vh = Point3D::new(start.x, target.y, start.z);
        let vh_seg1_collides = self.segment_intersects_obstacle(start, bend_vh, obstacles);
        let vh_seg2_collides = self.segment_intersects_obstacle(bend_vh, target, obstacles);
        eprintln!("[TOPO try_direct_path] L-shape (V then H): bend=({},{},{}), seg1_collides={}, seg2_collides={}",
            bend_vh.x, bend_vh.y, bend_vh.z, vh_seg1_collides, vh_seg2_collides);
        if !vh_seg1_collides && !vh_seg2_collides {
            let total_length = start.manhattan_distance(&target);
            eprintln!("[TOPO try_direct_path] Returning L-shape (V then H)");
            return Some(TopologicalPath {
                waypoints: vec![start, bend_vh, target],
                total_length,
            });
        }

        eprintln!("[TOPO try_direct_path] No direct path found, returning None");
        None
    }

    /// Find parallel ray pairs to find 2-bend (Z-shape) paths with horizontal parallel rays.
    pub(crate) fn try_parallel_horizontal(
        &self,
        start: Point3D,
        target: Point3D,
        s_ray: &super::types::SearchRay,
        t_ray: &super::types::SearchRay,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        if s_ray.origin.y == t_ray.origin.y {
            return None;
        }

        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;

        let s_min_x = if s_ray.direction == super::types::RayDirection::East {
            s_ray.origin.x
        } else {
            s_ray.origin.x - s_ray.max_distance
        };
        let s_max_x = if s_ray.direction == super::types::RayDirection::East {
            s_ray.origin.x + s_ray.max_distance
        } else {
            s_ray.origin.x
        };

        let t_min_x = if t_ray.direction == super::types::RayDirection::East {
            t_ray.origin.x
        } else {
            t_ray.origin.x - t_ray.max_distance
        };
        let t_max_x = if t_ray.direction == super::types::RayDirection::East {
            t_ray.origin.x + t_ray.max_distance
        } else {
            t_ray.origin.x
        };

        let overlap_min_x = s_min_x.max(t_min_x).max(board_bounds.min.x);
        let overlap_max_x = s_max_x.min(t_max_x).min(board_bounds.max.x);

        if overlap_min_x > overlap_max_x {
            return None;
        }

        let mut candidates = vec![start.x, target.x];

        let query_box = BoundingBox {
            min: Point3D::new(overlap_min_x, start.y.min(target.y), start.z),
            max: Point3D::new(overlap_max_x, start.y.max(target.y), start.z),
        };
        for seg in self.query_all_obstacles(&query_box, obstacles) {
            if self.exempt_net_ids.contains(&seg.net_id) {
                continue;
            }
            let obs_min_x = seg.start.x.min(seg.end.x) - seg.width_nm / 2;
            let obs_max_x = seg.start.x.max(seg.end.x) + seg.width_nm / 2;
            candidates.push(obs_min_x - inflate);
            candidates.push(obs_max_x + inflate);
        }

        candidates.retain(|&x| x >= overlap_min_x && x <= overlap_max_x);
        candidates.sort_unstable();
        candidates.dedup();

        for x in candidates {
            let p1 = Point3D::new(x, start.y, start.z);
            let p2 = Point3D::new(x, target.y, start.z);

            if self.point_in_obstacle(p1, obstacles) || self.point_in_obstacle(p2, obstacles) {
                continue;
            }

            if !self.segment_intersects_obstacle(start, p1, obstacles)
                && !self.segment_intersects_obstacle(p1, p2, obstacles)
                && !self.segment_intersects_obstacle(p2, target, obstacles)
            {
                let waypoints = vec![start, p1, p2, target];
                let total_length: i64 = waypoints
                    .windows(2)
                    .map(|w| w[0].manhattan_distance(&w[1]))
                    .sum();

                return Some(TopologicalPath {
                    waypoints,
                    total_length,
                });
            }
        }

        None
    }

    /// Find parallel ray pairs to find 2-bend (Z-shape) paths with vertical parallel rays.
    pub(crate) fn try_parallel_vertical(
        &self,
        start: Point3D,
        target: Point3D,
        s_ray: &super::types::SearchRay,
        t_ray: &super::types::SearchRay,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        if s_ray.origin.x == t_ray.origin.x {
            return None;
        }

        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;

        let s_min_y = if s_ray.direction == super::types::RayDirection::North {
            s_ray.origin.y
        } else {
            s_ray.origin.y - s_ray.max_distance
        };
        let s_max_y = if s_ray.direction == super::types::RayDirection::North {
            s_ray.origin.y + s_ray.max_distance
        } else {
            s_ray.origin.y
        };

        let t_min_y = if t_ray.direction == super::types::RayDirection::North {
            t_ray.origin.y
        } else {
            t_ray.origin.y - t_ray.max_distance
        };
        let t_max_y = if t_ray.direction == super::types::RayDirection::North {
            t_ray.origin.y + t_ray.max_distance
        } else {
            t_ray.origin.y
        };

        let overlap_min_y = s_min_y.max(t_min_y).max(board_bounds.min.y);
        let overlap_max_y = s_max_y.min(t_max_y).min(board_bounds.max.y);

        if overlap_min_y > overlap_max_y {
            return None;
        }

        let mut candidates = vec![start.y, target.y];

        let query_box = BoundingBox {
            min: Point3D::new(start.x.min(target.x), overlap_min_y, start.z),
            max: Point3D::new(start.x.max(target.x), overlap_max_y, start.z),
        };
        for seg in self.query_all_obstacles(&query_box, obstacles) {
            if self.exempt_net_ids.contains(&seg.net_id) {
                continue;
            }
            let obs_min_y = seg.start.y.min(seg.end.y) - seg.width_nm / 2;
            let obs_max_y = seg.start.y.max(seg.end.y) + seg.width_nm / 2;
            candidates.push(obs_min_y - inflate - 1);
            candidates.push(obs_max_y + inflate + 1);
        }

        candidates.retain(|&y| y >= overlap_min_y && y <= overlap_max_y);
        candidates.sort_unstable();
        candidates.dedup();

        for y in candidates {
            let p1 = Point3D::new(start.x, y, start.z);
            let p2 = Point3D::new(target.x, y, start.z);

            if self.point_in_obstacle(p1, obstacles) || self.point_in_obstacle(p2, obstacles) {
                continue;
            }

            if !self.segment_intersects_obstacle(start, p1, obstacles)
                && !self.segment_intersects_obstacle(p1, p2, obstacles)
                && !self.segment_intersects_obstacle(p2, target, obstacles)
            {
                let waypoints = vec![start, p1, p2, target];
                let total_length: i64 = waypoints
                    .windows(2)
                    .map(|w| w[0].manhattan_distance(&w[1]))
                    .sum();

                return Some(TopologicalPath {
                    waypoints,
                    total_length,
                });
            }
        }

        None
    }
}
