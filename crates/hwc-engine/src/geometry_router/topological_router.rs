use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::spatial_index::DynamicSpatialIndex;
use rustc_hash::FxHashMap;

/// Direction of a ray projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RayDirection {
    North, // +Y
    South, // -Y
    East,  // +X
    West,  // -X
}

impl RayDirection {
    #[inline]
    fn perpendicular(self) -> &'static [RayDirection] {
        match self {
            RayDirection::North | RayDirection::South => &[RayDirection::East, RayDirection::West],
            RayDirection::East | RayDirection::West => &[RayDirection::North, RayDirection::South],
        }
    }

    #[inline]
    fn is_horizontal(self) -> bool {
        matches!(self, RayDirection::East | RayDirection::West)
    }
}

/// A search ray projected from a point in a cardinal direction.
#[derive(Clone, Debug)]
pub struct SearchRay {
    pub origin: Point3D,
    pub direction: RayDirection,
    /// Maximum distance the ray can travel (board bounds)
    pub max_distance: i64,
}

/// An intersection between a ray and an obstacle AABB.
#[derive(Clone, Debug)]
pub struct RayIntersection {
    /// The exact coordinate where the ray hits the obstacle
    pub point: Point3D,
    /// Distance from ray origin to intersection
    pub distance: i64,
    /// The obstacle that was hit
    pub obstacle: BoundingBox,
}

/// A routed path consisting of orthogonal segments.
#[derive(Clone, Debug)]
pub struct TopologicalPath {
    pub waypoints: Vec<Point3D>,
    pub total_length: i64,
}

/// The Topological Line-Search Router.
///
/// Projects orthogonal search rays from start and target ports.
/// Uses the Axis-Aligned Slab Method for O(log N) ray-AABB intersection.
/// When rays collide with obstacles, it bends at 90 or 135 degrees.
pub struct TopologicalRouter {
    /// Width of routing traces in nanometers
    pub trace_width_nm: i64,
    /// Preferred routing direction per layer (true = horizontal)
    pub layer_prefer_horizontal: bool,
    /// Track pitch for grid snapping
    pub track_pitch_nm: i64,
}

impl TopologicalRouter {
    pub fn new(trace_width_nm: i64, track_pitch_nm: i64) -> Self {
        Self {
            trace_width_nm,
            layer_prefer_horizontal: true,
            track_pitch_nm,
        }
    }

    /// Route from start to target using topological line-search.
    /// Projects rays from both endpoints and finds intersecting open space.
    pub fn route(
        &self,
        start: Point3D,
        target: Point3D,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        if start == target {
            return Some(TopologicalPath {
                waypoints: vec![start],
                total_length: 0,
            });
        }

        let start_rays = self.project_all_rays(start, obstacles, board_bounds);
        let target_rays = self.project_all_rays(target, obstacles, board_bounds);

        // eprintln!(
        //     "[DEBUG TOPO] Routing {:?} -> {:?}. Start rays: {}, Target rays: {}",
        //     start, target, start_rays.len(), target_rays.len()
        // );

        // Try direct orthogonal connection first (L-shape or straight)
        if let Some(path) = self.try_direct_path(start, target, obstacles, board_bounds) {
            // eprintln!("[DEBUG TOPO] Found direct path with length {}", path.total_length);
            return Some(path);
        }

        // Find pairs of rays (one from start, one from target) that intersect
        let mut best_path: Option<TopologicalPath> = None;
        let mut best_length = i64::MAX;

        for s_ray in &start_rays {
            for t_ray in &target_rays {
                if let Some(meeting) = Self::find_ray_intersection(s_ray, t_ray) {
                    // Verify the meeting point is not inside an obstacle
                    if self.point_in_obstacle(meeting, obstacles) {
                        // eprintln!("[DEBUG TOPO] Meeting point {:?} is in obstacle", meeting);
                        continue;
                    }
                    // Build path: start -> bend_point_start -> meeting -> bend_point_target -> target
                    if let Some(path) = self.build_path_from_rays(
                        start, target, s_ray, t_ray, meeting, obstacles, board_bounds,
                    ) {
                        // eprintln!("[DEBUG TOPO] Found intersection path with length {}", path.total_length);
                        if path.total_length < best_length {
                            best_length = path.total_length;
                            best_path = Some(path);
                        }
                    }
                }
            }
        }

        // Also try perpendicular ray pairs that share an axis
        if best_path.is_none() {
            for s_ray in &start_rays {
                for t_ray in &target_rays {
                    if s_ray.direction == t_ray.direction {
                        continue;
                    }
                    if s_ray.direction.is_horizontal() == t_ray.direction.is_horizontal() {
                        continue;
                    }
                    // Horizontal + Vertical always intersect
                    let meeting = Point3D::new(
                        if t_ray.direction.is_horizontal() {
                            t_ray.origin.x
                        } else {
                            s_ray.origin.x
                        },
                        if s_ray.direction.is_horizontal() {
                            s_ray.origin.y
                        } else {
                            t_ray.origin.y
                        },
                        start.z,
                    );
                    if self.point_in_obstacle(meeting, obstacles) {
                        continue;
                    }
                    if let Some(path) = self.build_z_path(start, target, meeting, obstacles, board_bounds) {
                        if path.total_length < best_length {
                            best_length = path.total_length;
                            best_path = Some(path);
                        }
                    }
                }
            }
        }

        best_path
    }

    /// Try a direct L-shaped or straight path between two points.
    fn try_direct_path(
        &self,
        start: Point3D,
        target: Point3D,
        obstacles: &DynamicSpatialIndex,
        _board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        // Straight line (same X or same Y)
        if start.x == target.x || start.y == target.y {
            let waypoints = vec![start, target];
            if !self.segment_intersects_obstacle(start, target, obstacles) {
                let total_length = start.manhattan_distance(&target);
                return Some(TopologicalPath { waypoints, total_length });
            }
        }

        // L-shape: horizontal then vertical
        let bend_hv = Point3D::new(target.x, start.y, start.z);
        if !self.segment_intersects_obstacle(start, bend_hv, obstacles)
            && !self.segment_intersects_obstacle(bend_hv, target, obstacles)
        {
            let total_length = start.manhattan_distance(&target);
            return Some(TopologicalPath {
                waypoints: vec![start, bend_hv, target],
                total_length,
            });
        }

        // L-shape: vertical then horizontal
        let bend_vh = Point3D::new(start.x, target.y, start.z);
        if !self.segment_intersects_obstacle(start, bend_vh, obstacles)
            && !self.segment_intersects_obstacle(bend_vh, target, obstacles)
        {
            let total_length = start.manhattan_distance(&target);
            return Some(TopologicalPath {
                waypoints: vec![start, bend_vh, target],
                total_length,
            });
        }

        None
    }

    /// Project rays from origin in all 4 cardinal directions.
    fn project_all_rays(
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

        // Build a bounding box along the ray path for the spatial query
        let ray_bbox = self.ray_bbox(origin, direction, max_dist);
        let candidates = obstacles.query_bbox(&ray_bbox);

        let mut closest: Option<RayIntersection> = None;
        let mut min_dist = max_dist;

        for seg in candidates {
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
    /// Returns the distance to intersection, or None if no intersection.
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
                    // Origin is inside the AABB
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

    /// Compute a 90-degree bend point from a collision.
    /// Steps back slightly and projects perpendicular rays to find a clear path around.
    pub fn compute_bend_point(
        &self,
        collision: &RayIntersection,
        ray_direction: RayDirection,
        target: Point3D,
    ) -> Point3D {
        let step_back = self.trace_width_nm * 2;
        let perp_dirs = ray_direction.perpendicular();

        // Step back from collision along the ray direction
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

        // Choose perpendicular direction toward target
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
                    RayDirection::North => Point3D::new(stepped_back.x, stepped_back.y + offset, stepped_back.z),
                    RayDirection::South => Point3D::new(stepped_back.x, stepped_back.y - offset, stepped_back.z),
                    RayDirection::East => Point3D::new(stepped_back.x + offset, stepped_back.y, stepped_back.z),
                    RayDirection::West => Point3D::new(stepped_back.x - offset, stepped_back.y, stepped_back.z),
                };
            }
        }

        // Fallback: use first perpendicular direction
        let dir = perp_dirs[0];
        let offset = self.trace_width_nm * 3;
        match dir {
            RayDirection::North => Point3D::new(stepped_back.x, stepped_back.y + offset, stepped_back.z),
            RayDirection::South => Point3D::new(stepped_back.x, stepped_back.y - offset, stepped_back.z),
            RayDirection::East => Point3D::new(stepped_back.x + offset, stepped_back.y, stepped_back.z),
            RayDirection::West => Point3D::new(stepped_back.x - offset, stepped_back.y, stepped_back.z),
        }
    }

    /// Snap a 45-degree diagonal segment's length to the routing grid.
    /// L_snapped = round(N * track_pitch / sin(45°))
    pub fn snap_diagonal_length(&self, length: i64) -> i64 {
        let sin_45 = 707_106_781i64; // sin(45°) * 10^9
        let n = length / self.track_pitch_nm;
        if n == 0 {
            return self.track_pitch_nm;
        }
        let snapped = (n * self.track_pitch_nm * 1_000_000_000 / sin_45 / 1_000_000_000)
            * self.track_pitch_nm;
        snapped.max(self.track_pitch_nm)
    }

    /// Find the intersection of two rays from different origins.
    /// Two orthogonal rays (one horizontal, one vertical) always intersect.
    pub fn find_ray_intersection(ray_a: &SearchRay, ray_b: &SearchRay) -> Option<Point3D> {
        let a_horiz = ray_a.direction.is_horizontal();
        let b_horiz = ray_b.direction.is_horizontal();

        if a_horiz == b_horiz {
            return None; // Parallel rays don't intersect at a single point
        }

        let (horiz_ray, vert_ray) = if a_horiz {
            (ray_a, ray_b)
        } else {
            (ray_b, ray_a)
        };

        // Horizontal ray is at Y = horiz_ray.origin.y
        // Vertical ray is at X = vert_ray.origin.x
        let meeting = Point3D::new(vert_ray.origin.x, horiz_ray.origin.y, horiz_ray.origin.z);

        // Verify the meeting point is within both ray extents
        let h_dist = (meeting.x - horiz_ray.origin.x).abs();
        let v_dist = (meeting.y - vert_ray.origin.y).abs();

        if h_dist > horiz_ray.max_distance || v_dist > vert_ray.max_distance {
            // Check bounds more carefully based on direction
            let h_ok = match horiz_ray.direction {
                RayDirection::East => meeting.x >= horiz_ray.origin.x && h_dist <= horiz_ray.max_distance,
                RayDirection::West => meeting.x <= horiz_ray.origin.x && h_dist <= horiz_ray.max_distance,
                _ => false,
            };
            let v_ok = match vert_ray.direction {
                RayDirection::North => meeting.y >= vert_ray.origin.y && v_dist <= vert_ray.max_distance,
                RayDirection::South => meeting.y <= vert_ray.origin.y && v_dist <= vert_ray.max_distance,
                _ => false,
            };
            if !h_ok || !v_ok {
                return None;
            }
        }

        Some(meeting)
    }

    /// Build a complete path from start to target through a meeting point using Z-route.
    fn build_z_path(
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
    fn build_path_from_rays(
        &self,
        start: Point3D,
        target: Point3D,
        s_ray: &SearchRay,
        t_ray: &SearchRay,
        meeting: Point3D,
        obstacles: &DynamicSpatialIndex,
        _board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        let mut waypoints = Vec::new();
        waypoints.push(start);

        // If start ray doesn't go directly to meeting, add a bend
        let s_bend = if s_ray.direction.is_horizontal() {
            Point3D::new(meeting.x, start.y, start.z)
        } else {
            Point3D::new(start.x, meeting.y, start.z)
        };
        if s_bend != start && s_bend != meeting {
            waypoints.push(s_bend);
        }

        waypoints.push(meeting);

        // If target ray doesn't come directly from meeting, add a bend
        let t_bend = if t_ray.direction.is_horizontal() {
            Point3D::new(meeting.x, target.y, target.z)
        } else {
            Point3D::new(target.x, meeting.y, target.z)
        };
        if t_bend != meeting && t_bend != target {
            waypoints.push(t_bend);
        }

        waypoints.push(target);

        // Verify no segments intersect obstacles
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

    /// Check if a point is inside any obstacle.
    fn point_in_obstacle(&self, point: Point3D, obstacles: &DynamicSpatialIndex) -> bool {
        let bbox = BoundingBox::from_point(point, 1);
        let candidates = obstacles.query_bbox(&bbox);
        for seg in candidates {
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
    fn segment_intersects_obstacle(
        &self,
        a: Point3D,
        b: Point3D,
        obstacles: &DynamicSpatialIndex,
    ) -> bool {
        let bbox = BoundingBox {
            min: Point3D::new(
                a.x.min(b.x) - self.trace_width_nm,
                a.y.min(b.y) - self.trace_width_nm,
                a.z.min(b.z),
            ),
            max: Point3D::new(
                a.x.max(b.x) + self.trace_width_nm,
                a.y.max(b.y) + self.trace_width_nm,
                a.z.max(b.z),
            ),
        };

        let candidates = obstacles.query_bbox(&bbox);
        for seg in candidates {
            let seg_bbox = BoundingBox {
                min: Point3D::new(
                    seg.start.x.min(seg.end.x) - seg.width_nm / 2 - self.trace_width_nm,
                    seg.start.y.min(seg.end.y) - seg.width_nm / 2 - self.trace_width_nm,
                    seg.start.z.min(seg.end.z),
                ),
                max: Point3D::new(
                    seg.start.x.max(seg.end.x) + seg.width_nm / 2 + self.trace_width_nm,
                    seg.start.y.max(seg.end.y) + seg.width_nm / 2 + self.trace_width_nm,
                    seg.start.z.max(seg.end.z),
                ),
            };

            // Axis-aligned segment intersection check
            if a.y == b.y {
                // Horizontal segment
                let seg_y_min = seg_bbox.min.y;
                let seg_y_max = seg_bbox.max.y;
                if a.y >= seg_y_min && a.y <= seg_y_max {
                    let seg_x_min = seg_bbox.min.x;
                    let seg_x_max = seg_bbox.max.x;
                    let route_x_min = a.x.min(b.x);
                    let route_x_max = a.x.max(b.x);
                    if route_x_max >= seg_x_min && route_x_min <= seg_x_max {
                        return true;
                    }
                }
            } else if a.x == b.x {
                // Vertical segment
                let seg_x_min = seg_bbox.min.x;
                let seg_x_max = seg_bbox.max.x;
                if a.x >= seg_x_min && a.x <= seg_x_max {
                    let seg_y_min = seg_bbox.min.y;
                    let seg_y_max = seg_bbox.max.y;
                    let route_y_min = a.y.min(b.y);
                    let route_y_max = a.y.max(b.y);
                    if route_y_max >= seg_y_min && route_y_min <= seg_y_max {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Compute maximum ray distance before hitting board bounds.
    fn max_ray_distance(&self, origin: Point3D, direction: RayDirection, board_bounds: &BoundingBox) -> i64 {
        match direction {
            RayDirection::North => (board_bounds.max.y - origin.y).max(0),
            RayDirection::South => (origin.y - board_bounds.min.y).max(0),
            RayDirection::East => (board_bounds.max.x - origin.x).max(0),
            RayDirection::West => (origin.x - board_bounds.min.x).max(0),
        }
    }

    /// Build a bounding box that covers the ray's path for spatial queries.
    fn ray_bbox(&self, origin: Point3D, direction: RayDirection, max_dist: i64) -> BoundingBox {
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

/// Find open-space waypoints by expanding from a point in cardinal directions.
/// Returns the farthest unobstructed point in each direction.
pub fn expand_from_point(
    origin: Point3D,
    obstacles: &DynamicSpatialIndex,
    board_bounds: &BoundingBox,
    trace_width_nm: i64,
) -> FxHashMap<RayDirection, Point3D> {
    let mut result = FxHashMap::default();
    let router = TopologicalRouter::new(trace_width_nm, trace_width_nm);

    let directions = [
        RayDirection::North,
        RayDirection::South,
        RayDirection::East,
        RayDirection::West,
    ];

    for &dir in &directions {
        let max_dist = match dir {
            RayDirection::North => (board_bounds.max.y - origin.y).max(0),
            RayDirection::South => (origin.y - board_bounds.min.y).max(0),
            RayDirection::East => (board_bounds.max.x - origin.x).max(0),
            RayDirection::West => (origin.x - board_bounds.min.x).max(0),
        };

        if max_dist <= 0 {
            continue;
        }

        let farthest = match router.project_ray(origin, dir, obstacles, board_bounds) {
            Some(intersection) => {
                // Step back from obstacle
                let margin = trace_width_nm;
                match dir {
                    RayDirection::North => Point3D::new(origin.x, intersection.point.y - margin, origin.z),
                    RayDirection::South => Point3D::new(origin.x, intersection.point.y + margin, origin.z),
                    RayDirection::East => Point3D::new(intersection.point.x - margin, origin.y, origin.z),
                    RayDirection::West => Point3D::new(intersection.point.x + margin, origin.y, origin.z),
                }
            }
            None => match dir {
                RayDirection::North => Point3D::new(origin.x, board_bounds.max.y, origin.z),
                RayDirection::South => Point3D::new(origin.x, board_bounds.min.y, origin.z),
                RayDirection::East => Point3D::new(board_bounds.max.x, origin.y, origin.z),
                RayDirection::West => Point3D::new(board_bounds.min.x, origin.y, origin.z),
            },
        };

        result.insert(dir, farthest);
    }

    result
}
