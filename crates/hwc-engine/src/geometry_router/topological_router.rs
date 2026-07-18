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
    /// Minimum clearance from obstacles (Minkowski inflation).
    /// Added to trace_width_nm / 2 for collision boundary calculations.
    pub min_clearance_nm: i64,
    /// Net IDs to exempt from collision checks (e.g., start/goal pads).
    exempt_net_ids: Vec<usize>,
}

impl TopologicalRouter {
    /// Create a new router with required clearance parameter.
    ///
    /// `min_clearance_nm` is the PDK minimum trace spacing — it must come from
    /// the fabrication constraints (never silently default to 0). The clearance
    /// is used for Minkowski sum inflation: collision boundaries are expanded
    /// by `trace_width_nm / 2 + min_clearance_nm`.
    pub fn new(trace_width_nm: i64, track_pitch_nm: i64, min_clearance_nm: i64) -> Self {
        Self {
            trace_width_nm,
            layer_prefer_horizontal: true,
            track_pitch_nm,
            min_clearance_nm,
            exempt_net_ids: Vec::new(),
        }
    }

    /// Query the spatial index for obstacle candidates overlapping a bounding box.
    ///
    /// The DynamicSpatialIndex now uses per-layer sorted vectors with binary search,
    /// so queries only scan the relevant physical layer(s) — no R*-tree, no f64 conversion.
    fn query_all_obstacles(
        &self,
        bbox: &BoundingBox,
        dynamic: &DynamicSpatialIndex,
    ) -> Vec<crate::geometry_router::spatial_index::IndexedSegment> {
        dynamic.query_bbox(bbox).into_iter().cloned().collect()
    }

    /// Route with entity exemptions to prevent start/goal self-collision.
    ///
    /// When routing from a pad, the router should not detect the pad itself
    /// as an obstacle. This method shifts the ray origin outward by 1nm to
    /// clear the pad boundary before projecting search rays.
    pub fn route_with_exemptions(
        &self,
        start: Point3D,
        target: Point3D,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
        exempt_net_ids: &[usize],
    ) -> Option<TopologicalPath> {
        let router = TopologicalRouter {
            trace_width_nm: self.trace_width_nm,
            layer_prefer_horizontal: self.layer_prefer_horizontal,
            track_pitch_nm: self.track_pitch_nm,
            min_clearance_nm: self.min_clearance_nm,
            exempt_net_ids: exempt_net_ids.to_vec(),
        };

        // Shift ray origins outward by 1nm to clear pad boundaries
        let escape_nm = 1;
        let start_shifted = Point3D::new(start.x + escape_nm, start.y + escape_nm, start.z);
        let target_shifted = Point3D::new(target.x - escape_nm, target.y - escape_nm, target.z);

        // Try routing with shifted origins first
        if let Some(path) = router.route(start_shifted, target_shifted, obstacles, board_bounds) {
            // Restore original start/target in the path
            let mut waypoints = path.waypoints;
            if let Some(first) = waypoints.first_mut() {
                *first = start;
            }
            if let Some(last) = waypoints.last_mut() {
                *last = target;
            }
            return Some(TopologicalPath {
                waypoints,
                total_length: path.total_length,
            });
        }

        // Fallback: try with original positions
        router.route(start, target, obstacles, board_bounds)
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
                        start,
                        target,
                        s_ray,
                        t_ray,
                        meeting,
                        obstacles,
                        board_bounds,
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
                    if let Some(path) =
                        self.build_z_path(start, target, meeting, obstacles, board_bounds)
                    {
                        if path.total_length < best_length {
                            best_length = path.total_length;
                            best_path = Some(path);
                        }
                    }
                }
            }
        }

        // Try parallel ray pairs to find 2-bend (Z-shape) paths
        if best_path.is_none() {
            let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;
            for s_ray in &start_rays {
                for t_ray in &target_rays {
                    if s_ray.direction.is_horizontal() == t_ray.direction.is_horizontal() {
                        // Parallel rays!
                        if s_ray.direction.is_horizontal() {
                            // Both are horizontal (East/West)
                            // s_ray is at Y = s_ray.origin.y
                            // t_ray is at Y = t_ray.origin.y
                            if s_ray.origin.y == t_ray.origin.y {
                                continue;
                            }
                            
                            // Find the valid X-range where both horizontal rays overlap
                            let s_min_x = if s_ray.direction == RayDirection::East {
                                s_ray.origin.x
                            } else {
                                s_ray.origin.x - s_ray.max_distance
                            };
                            let s_max_x = if s_ray.direction == RayDirection::East {
                                s_ray.origin.x + s_ray.max_distance
                            } else {
                                s_ray.origin.x
                            };
                            
                            let t_min_x = if t_ray.direction == RayDirection::East {
                                t_ray.origin.x
                            } else {
                                t_ray.origin.x - t_ray.max_distance
                            };
                            let t_max_x = if t_ray.direction == RayDirection::East {
                                t_ray.origin.x + t_ray.max_distance
                            } else {
                                t_ray.origin.x
                            };
                            
                            let overlap_min_x = s_min_x.max(t_min_x).max(board_bounds.min.x);
                            let overlap_max_x = s_max_x.min(t_max_x).min(board_bounds.max.x);
                            
                            if overlap_min_x <= overlap_max_x {
                                // Collect candidate X coordinates
                                let mut candidates = vec![start.x, target.x];
                                
                                // Also collect X coordinates from obstacles (with inflation)
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
                                
                                // De-duplicate candidates and filter to overlap range
                                candidates.retain(|&x| x >= overlap_min_x && x <= overlap_max_x);
                                candidates.sort_unstable();
                                candidates.dedup();
                                
                                for x in candidates {
                                    let p1 = Point3D::new(x, start.y, start.z);
                                    let p2 = Point3D::new(x, target.y, start.z);
                                    
                                    // Check if meeting points are inside obstacles
                                    if self.point_in_obstacle(p1, obstacles) || self.point_in_obstacle(p2, obstacles) {
                                        continue;
                                    }
                                    
                                    // Check all three segments for collisions
                                    if !self.segment_intersects_obstacle(start, p1, obstacles)
                                        && !self.segment_intersects_obstacle(p1, p2, obstacles)
                                        && !self.segment_intersects_obstacle(p2, target, obstacles)
                                    {
                                        let waypoints = vec![start, p1, p2, target];
                                        let total_length: i64 = waypoints.windows(2)
                                            .map(|w| w[0].manhattan_distance(&w[1]))
                                            .sum();
                                            
                                        if total_length < best_length {
                                            best_length = total_length;
                                            best_path = Some(TopologicalPath {
                                                waypoints,
                                                total_length,
                                            });
                                        }
                                    }
                                }
                            }
                        } else {
                            // Both are vertical (North/South)
                            // s_ray is at X = s_ray.origin.x
                            // t_ray is at X = t_ray.origin.x
                            if s_ray.origin.x == t_ray.origin.x {
                                continue;
                            }
                            
                            // Find the valid Y-range where both vertical rays overlap
                            let s_min_y = if s_ray.direction == RayDirection::North {
                                s_ray.origin.y
                            } else {
                                s_ray.origin.y - s_ray.max_distance
                            };
                            let s_max_y = if s_ray.direction == RayDirection::North {
                                s_ray.origin.y + s_ray.max_distance
                            } else {
                                s_ray.origin.y
                            };
                            
                            let t_min_y = if t_ray.direction == RayDirection::North {
                                t_ray.origin.y
                            } else {
                                t_ray.origin.y - t_ray.max_distance
                            };
                            let t_max_y = if t_ray.direction == RayDirection::North {
                                t_ray.origin.y + t_ray.max_distance
                            } else {
                                t_ray.origin.y
                            };
                            
                            let overlap_min_y = s_min_y.max(t_min_y).max(board_bounds.min.y);
                            let overlap_max_y = s_max_y.min(t_max_y).min(board_bounds.max.y);
                            
                            if overlap_min_y <= overlap_max_y {
                                // Collect candidate Y coordinates
                                let mut candidates = vec![start.y, target.y];
                                
                                // Also collect Y coordinates from obstacles (with inflation)
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
                                    // +1 / -1 to step just off the inflated boundary.
                                    // The collision checker uses <= so a candidate AT the boundary
                                    // always counts as a collision — we must be strictly outside.
                                    candidates.push(obs_min_y - inflate - 1);
                                    candidates.push(obs_max_y + inflate + 1);
                                }
                                
                                // De-duplicate candidates and filter to overlap range
                                candidates.retain(|&y| y >= overlap_min_y && y <= overlap_max_y);
                                candidates.sort_unstable();
                                candidates.dedup();
                                
                                for y in candidates {
                                    let p1 = Point3D::new(start.x, y, start.z);
                                    let p2 = Point3D::new(target.x, y, start.z);
                                    
                                    // Check if meeting points are inside obstacles
                                    if self.point_in_obstacle(p1, obstacles) || self.point_in_obstacle(p2, obstacles) {
                                        continue;
                                    }
                                    
                                    // Check all three segments for collisions
                                    if !self.segment_intersects_obstacle(start, p1, obstacles)
                                        && !self.segment_intersects_obstacle(p1, p2, obstacles)
                                        && !self.segment_intersects_obstacle(p2, target, obstacles)
                                    {
                                        let waypoints = vec![start, p1, p2, target];
                                        let total_length: i64 = waypoints.windows(2)
                                            .map(|w| w[0].manhattan_distance(&w[1]))
                                            .sum();
                                            
                                        if total_length < best_length {
                                            best_length = total_length;
                                            best_path = Some(TopologicalPath {
                                                waypoints,
                                                total_length,
                                            });
                                        }
                                    }
                                }
                            }
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
                return Some(TopologicalPath {
                    waypoints,
                    total_length,
                });
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
        let candidates = self.query_all_obstacles(&ray_bbox, obstacles);

        let mut closest: Option<RayIntersection> = None;
        let mut min_dist = max_dist;
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;

        for seg in candidates {
            // Skip segments belonging to exempt nets
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

        // Fallback: use first perpendicular direction
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
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;
        let bbox = BoundingBox::from_point(point, inflate);
        let candidates = self.query_all_obstacles(&bbox, obstacles);
        for seg in candidates {
            // Skip segments belonging to exempt nets
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
    /// v0.1.9: Added proper Z-axis collision detection for 2.5D routing
    fn segment_intersects_obstacle(
        &self,
        a: Point3D,
        b: Point3D,
        obstacles: &DynamicSpatialIndex,
    ) -> bool {
        let inflate = self.trace_width_nm / 2 + self.min_clearance_nm;
        
        // v0.1.9 CRITICAL FIX: The spatial index is 2D (X-Y only), so we need to:
        // 1. Query with a 2D bbox (Z is ignored by the R-tree)
        // 2. Manually filter results by Z-range overlap
        let route_z_min = a.z.min(b.z);
        let route_z_max = a.z.max(b.z);
        
        let bbox = BoundingBox {
            min: Point3D::new(a.x.min(b.x) - inflate, a.y.min(b.y) - inflate, route_z_min),
            max: Point3D::new(a.x.max(b.x) + inflate, a.y.max(b.y) + inflate, route_z_max),
        };

        eprintln!(
            "[TOPO COLLISION] Checking segment ({},{},{}) to ({},{},{}) with inflate={}nm",
            a.x, a.y, a.z, b.x, b.y, b.z, inflate
        );
        eprintln!(
            "[TOPO COLLISION] Query bbox: ({},{},{}) to ({},{},{})",
            bbox.min.x, bbox.min.y, bbox.min.z, bbox.max.x, bbox.max.y, bbox.max.z
        );

        let candidates = self.query_all_obstacles(&bbox, obstacles);
        // eprintln!("[TOPO COLLISION] Found {} candidate obstacles (2D query, will filter by Z)", candidates.len());
        
        for (idx, seg) in candidates.iter().enumerate() {
            // Skip segments belonging to exempt nets
            if self.exempt_net_ids.contains(&seg.net_id) {
                eprintln!("[TOPO COLLISION]   Obstacle {}: SKIPPED (exempt net_id={})", idx, seg.net_id);
                continue;
            }
            
            // v0.1.9: CRITICAL FIX - Check Z-axis overlap FIRST (before building seg_bbox)
            // The obstacle's Z-range is stored directly in seg.start.z and seg.end.z
            let obs_z_min = seg.start.z.min(seg.end.z);
            let obs_z_max = seg.start.z.max(seg.end.z);
            
            // Z-range intersection test: ranges overlap if (start1 <= end2) AND (start2 <= end1)
            let z_overlaps = route_z_min <= obs_z_max && obs_z_min <= route_z_max;
            eprintln!(
                "[TOPO COLLISION]   Obstacle {}: net_id={}, Z-check: route_z=[{},{}], obs_z=[{},{}], overlaps={}",
                idx, seg.net_id, route_z_min, route_z_max, obs_z_min, obs_z_max, z_overlaps
            );
            if !z_overlaps {
                // eprintln!("[TOPO COLLISION]   SKIPPED (no Z-overlap)");
                continue; // No Z-overlap, skip this obstacle
            }
            
            let seg_bbox = BoundingBox {
                min: Point3D::new(
                    seg.start.x.min(seg.end.x) - seg.width_nm / 2 - inflate,
                    seg.start.y.min(seg.end.y) - seg.width_nm / 2 - inflate,
                    obs_z_min,
                ),
                max: Point3D::new(
                    seg.start.x.max(seg.end.x) + seg.width_nm / 2 + inflate,
                    seg.start.y.max(seg.end.y) + seg.width_nm / 2 + inflate,
                    obs_z_max,
                ),
            };

            eprintln!(
                "[TOPO COLLISION]   Obstacle {}: bbox=({},{},{}) to ({},{},{})",
                idx,
                seg_bbox.min.x, seg_bbox.min.y, seg_bbox.min.z,
                seg_bbox.max.x, seg_bbox.max.y, seg_bbox.max.z
            );

            // Axis-aligned segment intersection check (X-Y plane)
            if a.y == b.y {
                // Horizontal segment
                let seg_y_min = seg_bbox.min.y;
                let seg_y_max = seg_bbox.max.y;
                eprintln!(
                    "[TOPO COLLISION]   Horizontal route at Y={}, obstacle Y=[{},{}]",
                    a.y, seg_y_min, seg_y_max
                );
                if a.y >= seg_y_min && a.y <= seg_y_max {
                    let seg_x_min = seg_bbox.min.x;
                    let seg_x_max = seg_bbox.max.x;
                    let route_x_min = a.x.min(b.x);
                    let route_x_max = a.x.max(b.x);
                    eprintln!(
                        "[TOPO COLLISION]   Y-overlap! route_x=[{},{}], obs_x=[{},{}]",
                        route_x_min, route_x_max, seg_x_min, seg_x_max
                    );
                    if route_x_max >= seg_x_min && route_x_min <= seg_x_max {
                        // eprintln!("[TOPO COLLISION]   ❌ COLLISION DETECTED!");
                        return true;
                    }
                }
            } else if a.x == b.x {
                // Vertical segment
                let seg_x_min = seg_bbox.min.x;
                let seg_x_max = seg_bbox.max.x;
                eprintln!(
                    "[TOPO COLLISION]   Vertical route at X={}, obstacle X=[{},{}]",
                    a.x, seg_x_min, seg_x_max
                );
                if a.x >= seg_x_min && a.x <= seg_x_max {
                    let seg_y_min = seg_bbox.min.y;
                    let seg_y_max = seg_bbox.max.y;
                    let route_y_min = a.y.min(b.y);
                    let route_y_max = a.y.max(b.y);
                    eprintln!(
                        "[TOPO COLLISION]   X-overlap! route_y=[{},{}], obs_y=[{},{}]",
                        route_y_min, route_y_max, seg_y_min, seg_y_max
                    );
                    if route_y_max >= seg_y_min && route_y_min <= seg_y_max {
                        // eprintln!("[TOPO COLLISION]   ❌ COLLISION DETECTED!");
                        return true;
                    }
                }
            }
        }

        // eprintln!("[TOPO COLLISION] ✅ No collisions detected");
        false
    }

    /// Compute maximum ray distance before hitting board bounds.
    fn max_ray_distance(
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
    min_clearance_nm: i64,
) -> FxHashMap<RayDirection, Point3D> {
    let mut result = FxHashMap::default();
    let router = TopologicalRouter::new(trace_width_nm, trace_width_nm, min_clearance_nm);

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
                    RayDirection::North => {
                        Point3D::new(origin.x, intersection.point.y - margin, origin.z)
                    }
                    RayDirection::South => {
                        Point3D::new(origin.x, intersection.point.y + margin, origin.z)
                    }
                    RayDirection::East => {
                        Point3D::new(intersection.point.x - margin, origin.y, origin.z)
                    }
                    RayDirection::West => {
                        Point3D::new(intersection.point.x + margin, origin.y, origin.z)
                    }
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
