use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::spatial_index::DynamicSpatialIndex;

use super::types::{RayDirection, RayIntersection, SearchRay};
use super::TopologicalRouter;

impl TopologicalRouter {
    /// Project rays from origin in all 4 cardinal directions.
    pub(crate) fn project_all_rays(
        &self,
        origin: Point3D,
        _obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
    ) -> Vec<SearchRay> {
        let directions = [
            RayDirection::North,
            RayDirection::South,
            RayDirection::East,
            RayDirection::West,
        ];

        directions
            .iter()
            .filter_map(|&dir| {
                let max_dist = self.max_ray_distance(origin, dir, board_bounds);
                if max_dist <= 0 {
                    return None;
                }
                Some(SearchRay {
                    origin,
                    direction: dir,
                    max_distance: max_dist,
                })
            })
            .collect()
    }

    /// Project a ray from origin in the given direction.
    /// Returns the first obstacle intersection using the Slab Method.
    pub fn project_ray(
        &self,
        origin: Point3D,
        direction: RayDirection,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
    ) -> Option<RayIntersection> {
        let max_dist = self.max_ray_distance(origin, direction, board_bounds);

        let ray_bbox = self.ray_bbox(origin, direction, max_dist);
        let candidates = self.query_all_obstacles(&ray_bbox, obstacles);

        let mut closest: Option<RayIntersection> = None;
        let mut min_dist = max_dist;
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;

        for seg in candidates {
            if self.exempt_net_ids.contains(&seg.net_id) {
                continue;
            }
            let seg_bbox = BoundingBox {
                min: Point3D::new(
                    seg.start.x.min(seg.end.x) - seg.width_nm / 2 - inflate,
                    seg.start.y.min(seg.end.y) - seg.width_nm / 2 - inflate,
                    seg.start.z.min(seg.end.z),
                ),
                max: Point3D::new(
                    seg.start.x.max(seg.end.x) + seg.width_nm / 2 + inflate,
                    seg.start.y.max(seg.end.y) + seg.width_nm / 2 + inflate,
                    seg.start.z.max(seg.end.z),
                ),
            };
            if let Some(dist) = self.slab_intersect(origin, direction, &seg_bbox) {
                if dist >= 0 && dist < min_dist {
                    min_dist = dist;
                    let point = match direction {
                        RayDirection::North => Point3D::new(origin.x, origin.y + dist, origin.z),
                        RayDirection::South => Point3D::new(origin.x, origin.y - dist, origin.z),
                        RayDirection::East => Point3D::new(origin.x + dist, origin.y, origin.z),
                        RayDirection::West => Point3D::new(origin.x - dist, origin.y, origin.z),
                    };
                    closest = Some(RayIntersection {
                        point,
                        distance: dist,
                        obstacle: seg_bbox,
                    });
                }
            }
        }

        if let Some(hit) = &closest {
            eprintln!(
                "[DEBUG RAY] Ray from {:?} dir {:?} hit obstacle {:?} at distance {}",
                origin, direction, hit.obstacle, hit.distance
            );
        }

        closest
    }

    /// Axis-Aligned Slab Method for ray-AABB intersection.
    #[inline]
    pub fn slab_intersect(
        &self,
        origin: Point3D,
        direction: RayDirection,
        aabb: &BoundingBox,
    ) -> Option<i64> {
        match direction {
            RayDirection::East => {
                if origin.y < aabb.min.y || origin.y > aabb.max.y {
                    return None;
                }
                if origin.x >= aabb.max.x {
                    return None;
                }
                let dist = aabb.min.x - origin.x;
                if dist > 0 {
                    Some(dist)
                } else {
                    Some(0)
                }
            }
            RayDirection::West => {
                if origin.y < aabb.min.y || origin.y > aabb.max.y {
                    return None;
                }
                if origin.x <= aabb.min.x {
                    return None;
                }
                let dist = origin.x - aabb.max.x;
                if dist > 0 {
                    Some(dist)
                } else {
                    Some(0)
                }
            }
            RayDirection::North => {
                if origin.x < aabb.min.x || origin.x > aabb.max.x {
                    return None;
                }
                if origin.y >= aabb.max.y {
                    return None;
                }
                let dist = aabb.min.y - origin.y;
                if dist > 0 {
                    Some(dist)
                } else {
                    Some(0)
                }
            }
            RayDirection::South => {
                if origin.x < aabb.min.x || origin.x > aabb.max.x {
                    return None;
                }
                if origin.y <= aabb.min.y {
                    return None;
                }
                let dist = origin.y - aabb.max.y;
                if dist > 0 {
                    Some(dist)
                } else {
                    Some(0)
                }
            }
        }
    }

    /// Find the intersection of two rays from different origins.
    pub fn find_ray_intersection(ray_a: &SearchRay, ray_b: &SearchRay) -> Option<Point3D> {
        let a_horiz = ray_a.direction.is_horizontal();
        let b_horiz = ray_b.direction.is_horizontal();

        if a_horiz == b_horiz {
            return None;
        }

        let (horiz_ray, vert_ray) = if a_horiz {
            (ray_a, ray_b)
        } else {
            (ray_b, ray_a)
        };

        let meeting = Point3D::new(vert_ray.origin.x, horiz_ray.origin.y, horiz_ray.origin.z);

        let h_dist = (meeting.x - horiz_ray.origin.x).abs();
        let v_dist = (meeting.y - vert_ray.origin.y).abs();

        if h_dist > horiz_ray.max_distance || v_dist > vert_ray.max_distance {
            let h_ok = match horiz_ray.direction {
                RayDirection::East => {
                    meeting.x >= horiz_ray.origin.x && h_dist <= horiz_ray.max_distance
                }
                RayDirection::West => {
                    meeting.x <= horiz_ray.origin.x && h_dist <= horiz_ray.max_distance
                }
                _ => false,
            };
            let v_ok = match vert_ray.direction {
                RayDirection::North => {
                    meeting.y >= vert_ray.origin.y && v_dist <= vert_ray.max_distance
                }
                RayDirection::South => {
                    meeting.y <= vert_ray.origin.y && v_dist <= vert_ray.max_distance
                }
                _ => false,
            };
            if !h_ok || !v_ok {
                return None;
            }
        }

        Some(meeting)
    }

    /// Compute a 90-degree bend point from a collision.
    pub fn compute_bend_point(
        &self,
        collision: &RayIntersection,
        ray_direction: RayDirection,
        target: Point3D,
    ) -> Point3D {
        let step_back = self.trace_width_nm * 2;
        let perp_dirs = ray_direction.perpendicular();

        let stepped_back = match ray_direction {
            RayDirection::North => Point3D::new(
                collision.point.x,
                collision.point.y - step_back,
                collision.point.z,
            ),
            RayDirection::South => Point3D::new(
                collision.point.x,
                collision.point.y + step_back,
                collision.point.z,
            ),
            RayDirection::East => Point3D::new(
                collision.point.x - step_back,
                collision.point.y,
                collision.point.z,
            ),
            RayDirection::West => Point3D::new(
                collision.point.x + step_back,
                collision.point.y,
                collision.point.z,
            ),
        };

        for &dir in perp_dirs {
            let toward_target = match dir {
                RayDirection::North => target.y > stepped_back.y,
                RayDirection::South => target.y < stepped_back.y,
                RayDirection::East => target.x > stepped_back.x,
                RayDirection::West => target.x < stepped_back.x,
            };
            if toward_target {
                let offset = self.trace_width_nm * 3;
                return match dir {
                    RayDirection::North => {
                        Point3D::new(stepped_back.x, stepped_back.y + offset, stepped_back.z)
                    }
                    RayDirection::South => {
                        Point3D::new(stepped_back.x, stepped_back.y - offset, stepped_back.z)
                    }
                    RayDirection::East => {
                        Point3D::new(stepped_back.x + offset, stepped_back.y, stepped_back.z)
                    }
                    RayDirection::West => {
                        Point3D::new(stepped_back.x - offset, stepped_back.y, stepped_back.z)
                    }
                };
            }
        }

        let dir = perp_dirs[0];
        let offset = self.trace_width_nm * 3;
        match dir {
            RayDirection::North => {
                Point3D::new(stepped_back.x, stepped_back.y + offset, stepped_back.z)
            }
            RayDirection::South => {
                Point3D::new(stepped_back.x, stepped_back.y - offset, stepped_back.z)
            }
            RayDirection::East => {
                Point3D::new(stepped_back.x + offset, stepped_back.y, stepped_back.z)
            }
            RayDirection::West => {
                Point3D::new(stepped_back.x - offset, stepped_back.y, stepped_back.z)
            }
        }
    }

    /// Snap a 45-degree diagonal segment's length to the routing grid.
    pub fn snap_diagonal_length(&self, length: i64) -> i64 {
        let sin_45 = 707_106_781i64;
        let n = length / self.track_pitch_nm;
        if n == 0 {
            return self.track_pitch_nm;
        }
        let snapped = (n * self.track_pitch_nm * 1_000_000_000 / sin_45 / 1_000_000_000)
            * self.track_pitch_nm;
        snapped.max(self.track_pitch_nm)
    }

    /// Compute maximum ray distance before hitting board bounds.
    pub(crate) fn max_ray_distance(
        &self,
        origin: Point3D,
        direction: RayDirection,
        board_bounds: &BoundingBox,
    ) -> i64 {
        match direction {
            RayDirection::North => (board_bounds.max.y - origin.y).max(0),
            RayDirection::South => (origin.y - board_bounds.min.y).max(0),
            RayDirection::East => (board_bounds.max.x - origin.x).max(0),
            RayDirection::West => (origin.x - board_bounds.min.x).max(0),
        }
    }

    /// Build a bounding box that covers the ray's path for spatial queries.
    pub(crate) fn ray_bbox(
        &self,
        origin: Point3D,
        direction: RayDirection,
        max_dist: i64,
    ) -> BoundingBox {
        let half_w = self.trace_width_nm;
        match direction {
            RayDirection::North => BoundingBox {
                min: Point3D::new(origin.x - half_w, origin.y, origin.z),
                max: Point3D::new(origin.x + half_w, origin.y + max_dist, origin.z),
            },
            RayDirection::South => BoundingBox {
                min: Point3D::new(origin.x - half_w, origin.y - max_dist, origin.z),
                max: Point3D::new(origin.x + half_w, origin.y, origin.z),
            },
            RayDirection::East => BoundingBox {
                min: Point3D::new(origin.x, origin.y - half_w, origin.z),
                max: Point3D::new(origin.x + max_dist, origin.y + half_w, origin.z),
            },
            RayDirection::West => BoundingBox {
                min: Point3D::new(origin.x - max_dist, origin.y - half_w, origin.z),
                max: Point3D::new(origin.x, origin.y + half_w, origin.z),
            },
        }
    }
}
