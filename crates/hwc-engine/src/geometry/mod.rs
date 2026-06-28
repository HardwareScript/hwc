//! Fixed-point geometry types for deterministic spatial operations.
//!
//! All coordinates use i64 nanometers to guarantee 100% determinism across
//! all CPU architectures. No floating-point math is used in spatial calculations.

use std::fmt;

/// Fixed-point 3D coordinate in nanometers.
///
/// Uses i64 for perfect reproducibility across all platforms.
/// No floating-point rounding errors.
///
/// **Coordinate Order**: X, Y, Z (matches parser convention)
/// - X: Horizontal (left to right)
/// - Y: Vertical (direction depends on origin)
/// - Z: Layer (1=top, 2=inner, 3=bottom)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Point3D {
    pub x: i64, // Horizontal (nanometers)
    pub y: i64, // Vertical (nanometers)
    pub z: i64, // Layer (nanometers)
}

impl Point3D {
    /// Create a new point with nanometer coordinates in X, Y, Z order.
    #[inline]
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    /// Create a point from millimeter coordinates in X, Y, Z order.
    ///
    /// Converts mm to nanometers (1mm = 1,000,000nm).
    #[inline]
    pub fn from_mm(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: (x * 1_000_000.0) as i64,
            y: (y * 1_000_000.0) as i64,
            z: (z * 1_000_000.0) as i64,
        }
    }

    /// Convert point to millimeter coordinates in X, Y, Z order.
    ///
    /// FIX A: INTEGER-NANOMETER EXPORT
    /// Uses precise division to avoid float precision errors.
    /// Instead of: (2200000 as f64 / 1_000_000.0) = 2.199999 (WRONG!)
    /// We ensure: 2200000 / 1_000_000 = 2.2 exactly
    #[inline]
    pub fn to_mm(&self) -> (f64, f64, f64) {
        // Use integer division first, then convert remainder
        // This ensures exact conversion for values that divide evenly
        fn nm_to_mm_precise(nm: i64) -> f64 {
            let mm_whole = nm / 1_000_000;
            let nm_remainder = nm % 1_000_000;
            mm_whole as f64 + (nm_remainder as f64 / 1_000_000.0)
        }

        (
            nm_to_mm_precise(self.x),
            nm_to_mm_precise(self.y),
            nm_to_mm_precise(self.z),
        )
    }

    /// Calculate Manhattan distance to another point.
    ///
    /// Returns the sum of absolute differences in each dimension.
    #[inline]
    pub fn manhattan_distance(&self, other: &Point3D) -> i64 {
        (self.x - other.x).abs() + (self.y - other.y).abs() + (self.z - other.z).abs()
    }

    /// Move in a Manhattan direction by a given distance.
    #[inline]
    pub fn move_direction(&self, dir: Direction, distance_nm: i64) -> Point3D {
        match dir {
            Direction::North => Point3D::new(self.x, self.y + distance_nm, self.z),
            Direction::South => Point3D::new(self.x, self.y - distance_nm, self.z),
            Direction::East => Point3D::new(self.x + distance_nm, self.y, self.z),
            Direction::West => Point3D::new(self.x - distance_nm, self.y, self.z),
            Direction::Up => Point3D::new(self.x, self.y, self.z + distance_nm),
            Direction::Down => Point3D::new(self.x, self.y, self.z - distance_nm),
        }
    }
}

impl fmt::Display for Point3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (x, y, z) = self.to_mm();
        write!(f, "[{:.3}mm, {:.3}mm, {:.3}mm]", x, y, z)
    }
}

/// Simple 2D integer point for pad shapes and polygons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point2D {
    pub x: i64,
    pub y: i64,
}

impl Point2D {
    pub fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// Simple polygon (list of 2D points)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polygon {
    pub points: Vec<Point2D>,
}

impl Polygon {
    pub fn new(points: Vec<Point2D>) -> Self {
        Self { points }
    }
}

/// Manhattan routing directions (axis-aligned only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North, // +Y
    South, // -Y
    East,  // +X
    West,  // -X
    Up,    // +Z
    Down,  // -Z
}

impl Direction {
    /// Get the opposite direction.
    #[inline]
    pub const fn opposite(&self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }

    /// Check if this direction is horizontal (X or Y axis).
    #[inline]
    pub const fn is_horizontal(&self) -> bool {
        matches!(
            self,
            Direction::North | Direction::South | Direction::East | Direction::West
        )
    }

    /// Check if this direction is vertical (Z axis).
    #[inline]
    pub const fn is_vertical(&self) -> bool {
        matches!(self, Direction::Up | Direction::Down)
    }
}

/// Axis-Aligned Bounding Box (integer coordinates).
///
/// Used for collision detection and spatial queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundingBox {
    pub min: Point3D,
    pub max: Point3D,
}

/// Edge enum for relative positioning (Sprint 3, Task 3.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    Front,
    Back,
    MinZ,
    MaxZ,
}

impl Edge {
    pub fn direction_vector(&self) -> (i64, i64, i64) {
        match self {
            Edge::Left => (-1, 0, 0),
            Edge::Right => (1, 0, 0),
            Edge::Top => (0, 1, 0),
            Edge::Bottom => (0, -1, 0),
            Edge::Front => (0, 0, -1),
            Edge::Back => (0, 0, 1),
            Edge::MinZ => (0, 0, -1),
            Edge::MaxZ => (0, 0, 1),
        }
    }
}

impl BoundingBox {
    /// Create a new bounding box from min and max points.
    #[inline]
    pub const fn new(min: Point3D, max: Point3D) -> Self {
        Self { min, max }
    }

    /// Create a bounding box from a point and size.
    #[inline]
    pub fn from_point(point: Point3D, size_nm: i64) -> Self {
        Self {
            min: point,
            max: Point3D::new(point.x + size_nm, point.y + size_nm, point.z + size_nm),
        }
    }

    /// Create a bounding box from two arbitrary points.
    ///
    /// Automatically determines min and max.
    #[inline]
    pub fn from_points(a: Point3D, b: Point3D) -> Self {
        Self {
            min: Point3D::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            max: Point3D::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        }
    }

    /// Check if this bounding box intersects another (including boundaries).
    #[inline]
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }

    /// Check if this bounding box contains a point.
    #[inline]
    pub fn contains(&self, point: Point3D) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Check if this bounding box entirely contains another.
    #[inline]
    pub fn contains_bbox(&self, other: &BoundingBox) -> bool {
        other.min.x >= self.min.x
            && other.max.x <= self.max.x
            && other.min.y >= self.min.y
            && other.max.y <= self.max.y
            && other.min.z >= self.min.z
            && other.max.z <= self.max.z
    }

    /// Expand the bounding box by a margin in all directions.
    #[inline]
    pub fn expand(&self, margin_nm: i64) -> BoundingBox {
        BoundingBox {
            min: Point3D::new(
                self.min.x - margin_nm,
                self.min.y - margin_nm,
                self.min.z - margin_nm,
            ),
            max: Point3D::new(
                self.max.x + margin_nm,
                self.max.y + margin_nm,
                self.max.z + margin_nm,
            ),
        }
    }

    /// Compute the union of two bounding boxes.
    ///
    /// Returns a new bounding box that contains both input boxes.
    /// Used for merging overlapping regions in array shared terminal merging.
    #[inline]
    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min: Point3D::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Point3D::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    /// Calculate the volume of the bounding box in cubic nanometers.
    #[inline]
    pub fn volume(&self) -> i64 {
        let width = (self.max.x - self.min.x).abs();
        let height = (self.max.y - self.min.y).abs();
        let depth = (self.max.z - self.min.z).abs();
        width * height * depth
    }

    /// Get the center point of a specific edge (Sprint 3, Task 3.1)
    ///
    /// Returns the center point of the specified edge face.
    /// This is used for relative positioning: `at M1.right + 1mm`
    ///
    /// **GAP1 FIX (v0.1.7)**: Changed to return the MIN corner of each edge face
    /// instead of the center. This ensures coordinate inheritance works correctly.
    ///
    /// Physical Reality: When you say "place this next to the last one," you want
    /// components to line up at their base (min Y, min Z), not float at different
    /// heights based on their center points.
    ///
    /// Before (center-based):
    /// - Adder[0] at (5mm, 5mm, 1mm) → edge.right returns (13mm, 7mm, 1.25mm)
    /// - Adder[1] at last.right + 2mm → placed at (15mm, 7mm, 1.25mm) ❌ WRONG Y!
    ///
    /// After (min-corner-based):
    /// - Adder[0] at (5mm, 5mm, 1mm) → edge.right returns (13mm, 5mm, 1mm)
    /// - Adder[1] at last.right + 2mm → placed at (15mm, 5mm, 1mm) ✅ CORRECT!
    ///
    /// Edge definitions (now return MIN corner of each face):
    /// - Left: min X face (min corner: min X, min Y, min Z)
    /// - Right: max X face (min corner: max X, min Y, min Z)
    /// - Top: max Y face (min corner: min X, max Y, min Z)
    /// - Bottom: min Y face (min corner: min X, min Y, min Z)
    /// - Front: min Z face (min corner: min X, min Y, min Z)
    /// - Back: max Z face (min corner: min X, min Y, max Z)
    #[inline]
    pub fn edge_point(&self, edge: Edge) -> Point3D {
        match edge {
            Edge::Left => Point3D::new(self.min.x, self.min.y, self.min.z),
            Edge::Right => Point3D::new(self.max.x, self.min.y, self.min.z),
            Edge::Top => Point3D::new(self.min.x, self.max.y, self.min.z),
            Edge::Bottom => Point3D::new(self.min.x, self.min.y, self.min.z),
            Edge::Front => Point3D::new(self.min.x, self.min.y, self.min.z),
            Edge::Back => Point3D::new(self.min.x, self.min.y, self.max.z),
            Edge::MinZ => Point3D::new(self.min.x, self.min.y, self.min.z),
            Edge::MaxZ => Point3D::new(self.min.x, self.min.y, self.max.z),
        }
    }

    /// Calculate Manhattan distance to another bounding box.
    #[inline]
    pub fn manhattan_distance(&self, other: &BoundingBox) -> i64 {
        let dx = (self.min.x - other.max.x).max(0).max(other.min.x - self.max.x);
        let dy = (self.min.y - other.max.y).max(0).max(other.min.y - self.max.y);
        let dz = (self.min.z - other.max.z).max(0).max(other.min.z - self.max.z);
        dx + dy + dz
    }
}

impl fmt::Display for BoundingBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BBox[{} to {}]", self.min, self.max)
    }
}

/// Manhattan-routed trace segment (integer coordinates).
///
/// Represents a single segment of a routed trace with width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceSegment {
    pub start: Point3D,
    pub end: Point3D,
    pub width_nm: i64,
}

impl TraceSegment {
    /// Create a new trace segment.
    #[inline]
    pub const fn new(start: Point3D, end: Point3D, width_nm: i64) -> Self {
        Self {
            start,
            end,
            width_nm,
        }
    }

    /// Calculate the bounding box of this trace segment.
    #[inline]
    pub fn bounding_box(&self) -> BoundingBox {
        let half_width = self.width_nm / 2;

        BoundingBox {
            min: Point3D::new(
                self.start.x.min(self.end.x) - half_width,
                self.start.y.min(self.end.y) - half_width,
                self.start.z.min(self.end.z) - half_width,
            ),
            max: Point3D::new(
                self.start.x.max(self.end.x) + half_width,
                self.start.y.max(self.end.y) + half_width,
                self.start.z.max(self.end.z) + half_width,
            ),
        }
    }

    /// Calculate the Manhattan length of this segment.
    #[inline]
    pub fn length(&self) -> i64 {
        self.start.manhattan_distance(&self.end)
    }

    /// Check if this segment is horizontal (same Y and Z).
    #[inline]
    pub fn is_horizontal(&self) -> bool {
        self.start.y == self.end.y && self.start.z == self.end.z
    }

    /// Check if this segment is vertical (same X and Z).
    #[inline]
    pub fn is_vertical(&self) -> bool {
        self.start.x == self.end.x && self.start.z == self.end.z
    }

    /// Check if this segment is a via (same X and Y, different Z).
    #[inline]
    pub fn is_via(&self) -> bool {
        self.start.x == self.end.x && self.start.y == self.end.y
    }
}

pub mod entity_ids;
pub mod transform;

pub use entity_ids::{
    ComponentGraphId, EntityId, GeometryGraphId, JunctionGraphId, NetGraphId, PinGraphId,
    RouteGraphId,
};
pub use transform::{BoundingBox2D, FixedTransform2D};
