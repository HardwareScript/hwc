use crate::geometry::transform::FixedTransform2D;
use crate::geometry::{BoundingBox, Point3D};

/// An Oriented Bounding Box (OBB) for rotated components.
/// Stores center, half-extents, and rotation angle.
#[derive(Clone, Debug)]
pub struct OrientedBoundingBox {
    pub center_x: i64,
    pub center_y: i64,
    pub half_width: i64,
    pub half_height: i64,
    pub rotation_deg: i64,
}

impl OrientedBoundingBox {
    pub fn new(
        center_x: i64,
        center_y: i64,
        half_width: i64,
        half_height: i64,
        rotation_deg: i64,
    ) -> Self {
        Self {
            center_x,
            center_y,
            half_width,
            half_height,
            rotation_deg,
        }
    }

    /// Check if a point is inside this OBB using SAT (Separating Axis Theorem)
    pub fn contains_point(&self, px: i64, py: i64) -> bool {
        // Transform point to local space (inverse rotation)
        let rad = (self.rotation_deg as f64) * std::f64::consts::PI / 180.0;
        let cos = rad.cos();
        let sin = rad.sin();
        let dx = px - self.center_x;
        let dy = py - self.center_y;
        let local_x = (dx as f64 * cos + dy as f64 * sin).abs();
        let local_y = (-dx as f64 * sin + dy as f64 * cos).abs();
        local_x <= self.half_width as f64 && local_y <= self.half_height as f64
    }

    /// Get the axis-aligned bounding box that encloses this OBB
    pub fn to_aabb(&self) -> BoundingBox {
        // Compute the 4 corners of the rotated rectangle and find min/max
        let rad = (self.rotation_deg as f64) * std::f64::consts::PI / 180.0;
        let cos = rad.cos();
        let sin = rad.sin();
        let hw = self.half_width as f64;
        let hh = self.half_height as f64;
        let corners = [
            (cos * hw - sin * hh, sin * hw + cos * hh),
            (-cos * hw - sin * hh, -sin * hw + cos * hh),
            (cos * hw + sin * hh, sin * hw - cos * hh),
            (-cos * hw + sin * hh, -sin * hw - cos * hh),
        ];
        let mut min_x = i64::MAX;
        let mut max_x = i64::MIN;
        let mut min_y = i64::MAX;
        let mut max_y = i64::MIN;
        for (cx, cy) in &corners {
            let x = self.center_x + *cx as i64;
            let y = self.center_y + *cy as i64;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        BoundingBox {
            min: Point3D::new(min_x, min_y, 0),
            max: Point3D::new(max_x, max_y, 0),
        }
    }
}

/// A local-coordinate OBVH (Oriented Bounding Volume Hierarchy) for a component type.
/// Each component definition is parsed exactly once into a ComponentStamp at origin [0, 0, 0].
/// Geometry is stored as analytical vectors (no grid-based stamping).
#[derive(Clone, Debug)]
pub struct ComponentStamp {
    /// Unique identifier for this stamp
    pub stamp_id: usize,
    /// Human-readable component name
    pub name: String,
    /// Local-coordinate axis-aligned bounding box
    pub local_bbox: BoundingBox,
    /// Local-coordinate OBB children (for rotated shapes)
    pub local_obb_children: Vec<OrientedBoundingBox>,
    /// Local-coordinate AABB children (for Manhattan shapes)
    pub local_aabb_children: Vec<BoundingBox>,
    /// Local-coordinate polygon vertices (fallback for non-standard shapes)
    pub local_polygons: Vec<Vec<Point3D>>,
    /// Pin positions in local coordinate space
    pub local_pin_offsets: Vec<(String, Point3D)>,
}

impl ComponentStamp {
    /// Create a new stamp from component definition data
    pub fn new(
        stamp_id: usize,
        name: String,
        local_bbox: BoundingBox,
        local_obb_children: Vec<OrientedBoundingBox>,
        local_aabb_children: Vec<BoundingBox>,
        local_polygons: Vec<Vec<Point3D>>,
        local_pin_offsets: Vec<(String, Point3D)>,
    ) -> Self {
        Self {
            stamp_id,
            name,
            local_bbox,
            local_obb_children,
            local_aabb_children,
            local_polygons,
            local_pin_offsets,
        }
    }

    /// Create a simple rectangular stamp (most common case)
    pub fn rectangle(stamp_id: usize, name: String, width_nm: i64, height_nm: i64) -> Self {
        let local_bbox =
            BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(width_nm, height_nm, 0));
        let local_aabb_children = vec![local_bbox];
        Self::new(
            stamp_id,
            name,
            local_bbox,
            Vec::new(),
            local_aabb_children,
            Vec::new(),
            Vec::new(),
        )
    }
}

/// A lightweight instance of a component placed in world space.
/// References a shared ComponentStamp and applies a FixedTransform2D.
/// All bounding boxes are pre-transformed into global world-coordinate space
/// at placement time, eliminating lossy on-demand inverse transforms.
#[derive(Clone, Debug)]
pub struct ComponentInstance {
    /// Unique instance identifier
    pub instance_id: usize,
    /// Reference to the shared ComponentStamp
    pub stamp_id: usize,
    /// The transform from local to global space
    pub transform: FixedTransform2D,
    /// Logical net bindings (indices into the netlist)
    pub net_bindings: Vec<usize>,

    // === Pre-transformed global bounding boxes ===
    /// Global bounding box in world-coordinate space
    pub global_bbox: BoundingBox,
    /// Global OBB children (for rotated shapes)
    pub global_obb_children: Vec<OrientedBoundingBox>,
    /// Global AABB children (for Manhattan shapes)
    pub global_aabb_children: Vec<BoundingBox>,
}

impl ComponentInstance {
    /// Create a new instance by forward-transforming the stamp's local bounding
    /// volumes into global world-coordinate space ONCE. This eliminates the need
    /// for lossy on-demand inverse transforms during pathfinding and DRC hot-paths.
    pub fn new(
        instance_id: usize,
        stamp: &ComponentStamp,
        transform: FixedTransform2D,
        net_bindings: Vec<usize>,
    ) -> Self {
        // Transform local bounding box forward into global space
        let global_bbox = Self::transform_bbox_to_global(&stamp.local_bbox, &transform);

        // Transform OBB children
        let global_obb_children = stamp
            .local_obb_children
            .iter()
            .map(|obb| Self::transform_obb_to_global(obb, &transform))
            .collect();

        // Transform AABB children
        let global_aabb_children = stamp
            .local_aabb_children
            .iter()
            .map(|aabb| Self::transform_bbox_to_global(aabb, &transform))
            .collect();

        Self {
            instance_id,
            stamp_id: stamp.stamp_id,
            transform,
            net_bindings,
            global_bbox,
            global_obb_children,
            global_aabb_children,
        }
    }

    /// Transform a local AABB to global space using the instance's transform
    fn transform_bbox_to_global(
        local_bbox: &BoundingBox,
        transform: &FixedTransform2D,
    ) -> BoundingBox {
        let (x1, y1) = transform.transform_point(local_bbox.min.x, local_bbox.min.y);
        let (x2, y2) = transform.transform_point(local_bbox.max.x, local_bbox.min.y);
        let (x3, y3) = transform.transform_point(local_bbox.min.x, local_bbox.max.y);
        let (x4, y4) = transform.transform_point(local_bbox.max.x, local_bbox.max.y);

        let min_x = x1.min(x2).min(x3).min(x4);
        let max_x = x1.max(x2).max(x3).max(x4);
        let min_y = y1.min(y2).min(y3).min(y4);
        let max_y = y1.max(y2).max(y3).max(y4);

        BoundingBox::new(Point3D::new(min_x, min_y, 0), Point3D::new(max_x, max_y, 0))
    }

    /// Transform a local OBB to global space
    fn transform_obb_to_global(
        local_obb: &OrientedBoundingBox,
        transform: &FixedTransform2D,
    ) -> OrientedBoundingBox {
        let (cx, cy) = transform.transform_point(local_obb.center_x, local_obb.center_y);
        let global_rotation = {
            let cos = transform.cos_scale as f64 / 1_000_000_000.0;
            let sin = transform.sin_scale as f64 / 1_000_000_000.0;
            let local_rad = (local_obb.rotation_deg as f64) * std::f64::consts::PI / 180.0;
            let total_rad = local_rad + sin.atan2(cos);
            (total_rad * 180.0 / std::f64::consts::PI) as i64
        };
        OrientedBoundingBox::new(
            cx,
            cy,
            local_obb.half_width,
            local_obb.half_height,
            global_rotation,
        )
    }

    /// Fast, non-allocating world-space collision check using pre-calculated bounds.
    /// Returns true if the point (gx, gy) is inside this component's physical geometry.
    #[inline]
    pub fn test_collision_global(&self, gx: i64, gy: i64) -> bool {
        // Fast AABB rejection test first
        if !self.global_bbox_contains(gx, gy) {
            return false;
        }
        // Check AABB children (Manhattan shapes)
        for aabb in &self.global_aabb_children {
            if aabb_contains_point(aabb, gx, gy) {
                return true;
            }
        }
        // Check OBB children (rotated shapes) — SAT-based
        for obb in &self.global_obb_children {
            if obb.contains_point(gx, gy) {
                return true;
            }
        }
        false
    }

    /// Check if a point is inside the global bounding box (fast rejection)
    fn global_bbox_contains(&self, gx: i64, gy: i64) -> bool {
        gx >= self.global_bbox.min.x
            && gx <= self.global_bbox.max.x
            && gy >= self.global_bbox.min.y
            && gy <= self.global_bbox.max.y
    }
}

/// Check if a point is inside an axis-aligned bounding box
fn aabb_contains_point(bbox: &BoundingBox, x: i64, y: i64) -> bool {
    x >= bbox.min.x && x <= bbox.max.x && y >= bbox.min.y && y <= bbox.max.y
}

/// A registry of all component stamps and instances in the design.
/// This is the master scene graph — the single source of truth for
/// component geometry and placement in the Entity Graph.
pub struct SceneGraph {
    stamps: Vec<ComponentStamp>,
    instances: Vec<ComponentInstance>,
    stamp_name_index: rustc_hash::FxHashMap<String, usize>,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self {
            stamps: Vec::new(),
            instances: Vec::new(),
            stamp_name_index: rustc_hash::FxHashMap::default(),
        }
    }

    /// Register a component stamp (parsed exactly once at origin)
    pub fn register_stamp(&mut self, stamp: ComponentStamp) -> usize {
        let id = self.stamps.len();
        self.stamp_name_index.insert(stamp.name.clone(), id);
        self.stamps.push(stamp);
        id
    }

    /// Get a stamp by ID
    pub fn get_stamp(&self, stamp_id: usize) -> Option<&ComponentStamp> {
        self.stamps.get(stamp_id)
    }

    /// Get a stamp by name
    pub fn get_stamp_by_name(&self, name: &str) -> Option<&ComponentStamp> {
        self.stamp_name_index.get(name).map(|&id| &self.stamps[id])
    }

    /// Place a component instance (forward-transform bounding boxes once)
    pub fn place_instance(
        &mut self,
        stamp_id: usize,
        transform: FixedTransform2D,
        net_bindings: Vec<usize>,
    ) -> Option<usize> {
        let stamp = self.stamps.get(stamp_id)?;
        let instance_id = self.instances.len();
        let instance = ComponentInstance::new(instance_id, stamp, transform, net_bindings);
        self.instances.push(instance);
        Some(instance_id)
    }

    /// Get an instance by ID
    pub fn get_instance(&self, instance_id: usize) -> Option<&ComponentInstance> {
        self.instances.get(instance_id)
    }

    /// Get all instances
    pub fn instances(&self) -> &[ComponentInstance] {
        &self.instances
    }

    /// Get all stamps
    pub fn stamps(&self) -> &[ComponentStamp] {
        &self.stamps
    }

    /// Get number of stamps
    pub fn stamp_count(&self) -> usize {
        self.stamps.len()
    }

    /// Get number of instances
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Estimate memory usage in bytes
    pub fn estimate_memory_bytes(&self) -> usize {
        let stamp_mem: usize = self
            .stamps
            .iter()
            .map(|s| {
                std::mem::size_of::<ComponentStamp>()
                    + s.local_obb_children.len() * std::mem::size_of::<OrientedBoundingBox>()
                    + s.local_aabb_children.len() * std::mem::size_of::<BoundingBox>()
                    + s.local_polygons
                        .iter()
                        .map(|p| p.len() * std::mem::size_of::<Point3D>())
                        .sum::<usize>()
            })
            .sum();
        let instance_mem: usize = self.instances.len() * std::mem::size_of::<ComponentInstance>();
        stamp_mem + instance_mem
    }
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}
