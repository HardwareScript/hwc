use std::fmt;

/// Bounding box edge for relative positioning and anchor resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Get the unit direction vector (dx, dy, dz) for this edge.
    pub fn direction_vector(&self) -> (i64, i64, i64) {
        match self {
            Edge::Left => (-1, 0, 0),
            Edge::Right => (1, 0, 0),
            Edge::Top => (0, 1, 0),
            Edge::Bottom => (0, -1, 0),
            Edge::Front | Edge::MaxZ => (0, 0, 1),
            Edge::Back | Edge::MinZ => (0, 0, -1),
        }
    }
}

/// Fixed-point 3D coordinate in nanometers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Point3D {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl Point3D {
    #[inline]
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn manhattan_distance(&self, other: &Point3D) -> i64 {
        (self.x - other.x).abs() + (self.y - other.y).abs() + (self.z - other.z).abs()
    }

    #[inline]
    pub fn offset(&self, dx: i64, dy: i64, dz: i64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    pub fn move_direction(&self, dir: Direction, dist: i64) -> Self {
        match dir {
            Direction::North => Self::new(self.x, self.y + dist, self.z),
            Direction::South => Self::new(self.x, self.y - dist, self.z),
            Direction::East => Self::new(self.x + dist, self.y, self.z),
            Direction::West => Self::new(self.x - dist, self.y, self.z),
            Direction::Up => Self::new(self.x, self.y, self.z + dist),
            Direction::Down => Self::new(self.x, self.y, self.z - dist),
        }
    }
}

impl fmt::Display for Point3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}nm, {}nm, {}nm]", self.x, self.y, self.z)
    }
}

/// Fixed-point 2D coordinate in nanometers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Point2D {
    pub x: i64,
    pub y: i64,
}

impl Point2D {
    #[inline]
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// 2D Polygon represented by a sequence of points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Polygon {
    pub points: Vec<Point2D>,
}

impl Polygon {
    pub fn new(points: Vec<Point2D>) -> Self {
        Self { points }
    }
}

/// Manhattan routing directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

/// Axis-Aligned Bounding Box (integer coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BoundingBox {
    pub min: Point3D,
    pub max: Point3D,
}

impl BoundingBox {
    #[inline]
    pub const fn new(min: Point3D, max: Point3D) -> Self {
        Self { min, max }
    }

    pub fn from_point(p: Point3D, margin: i64) -> Self {
        Self {
            min: Point3D::new(p.x - margin, p.y - margin, p.z - margin),
            max: Point3D::new(p.x + margin, p.y + margin, p.z + margin),
        }
    }

    #[inline]
    pub fn contains(&self, p: Point3D) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    #[inline]
    pub fn expand(&self, margin: i64) -> Self {
        Self {
            min: Point3D::new(
                self.min.x - margin,
                self.min.y - margin,
                self.min.z - margin,
            ),
            max: Point3D::new(
                self.max.x + margin,
                self.max.y + margin,
                self.max.z + margin,
            ),
        }
    }

    #[inline]
    pub fn inflate(&self, margin: i64) -> Self {
        self.expand(margin)
    }

    /// Inflate the bounding box in X and Y only (Sprint 3.10 - Native DRC).
    #[inline]
    pub fn inflate_xy(&self, margin: i64) -> Self {
        Self {
            min: Point3D::new(self.min.x - margin, self.min.y - margin, self.min.z),
            max: Point3D::new(self.max.x + margin, self.max.y + margin, self.max.z),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.min.x >= self.max.x || self.min.y >= self.max.y || self.min.z >= self.max.z
    }

    pub fn center(&self) -> Point3D {
        Point3D::new(
            (self.min.x + self.max.x) / 2,
            (self.min.y + self.max.y) / 2,
            (self.min.z + self.max.z) / 2,
        )
    }

    #[inline]
    pub fn center_x(&self) -> i64 {
        (self.min.x + self.max.x) / 2
    }

    #[inline]
    pub fn center_y(&self) -> i64 {
        (self.min.y + self.max.y) / 2
    }

    #[inline]
    pub fn center_z(&self) -> i64 {
        (self.min.z + self.max.z) / 2
    }

    pub fn volume(&self) -> i128 {
        if self.is_empty() {
            return 0;
        }
        (self.max.x - self.min.x) as i128
            * (self.max.y - self.min.y) as i128
            * (self.max.z - self.min.z) as i128
    }

    pub fn distance_to(&self, other: &BoundingBox) -> i64 {
        // Calculate gap in each dimension (0 if overlapping)
        let dx = (self.min.x - other.max.x)
            .max(other.min.x - self.max.x)
            .max(0);
        let dy = (self.min.y - other.max.y)
            .max(other.min.y - self.max.y)
            .max(0);
        let dz = (self.min.z - other.max.z)
            .max(other.min.z - self.max.z)
            .max(0);

        // Return Chebyshev distance (L∞ norm - maximum gap in any dimension)
        // This is the correct metric for axis-aligned clearance checking
        dx.max(dy).max(dz)
    }

    #[inline]
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }

    #[inline]
    pub fn intersection(&self, other: &BoundingBox) -> Option<BoundingBox> {
        let min_x = self.min.x.max(other.min.x);
        let max_x = self.max.x.min(other.max.x);
        let min_y = self.min.y.max(other.min.y);
        let max_y = self.max.y.min(other.max.y);
        let min_z = self.min.z.max(other.min.z);
        let max_z = self.max.z.min(other.max.z);

        if min_x < max_x && min_y < max_y && min_z < max_z {
            Some(BoundingBox::new(
                Point3D::new(min_x, min_y, min_z),
                Point3D::new(max_x, max_y, max_z),
            ))
        } else {
            None
        }
    }

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

    /// Get the center point of a specific edge face.
    ///
    /// For X/Y edges returns the midpoint of that face (at the bounding box center for other axes).
    /// For Z edges returns the center of the min/max Z face.
    pub fn edge_point(&self, edge: Edge) -> Point3D {
        let c = self.center();
        match edge {
            Edge::Left => Point3D::new(self.min.x, c.y, c.z),
            Edge::Right => Point3D::new(self.max.x, c.y, c.z),
            Edge::Top => Point3D::new(c.x, self.max.y, c.z),
            Edge::Bottom => Point3D::new(c.x, self.min.y, c.z),
            Edge::Front => Point3D::new(c.x, c.y, self.max.z),
            Edge::Back => Point3D::new(c.x, c.y, self.min.z),
            Edge::MinZ => Point3D::new(c.x, c.y, self.min.z),
            Edge::MaxZ => Point3D::new(c.x, c.y, self.max.z),
        }
    }
}

/// Manhattan-routed trace segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceSegment {
    pub start: Point3D,
    pub end: Point3D,
    pub width_nm: i64,
    /// Material ID for thickness/electrical lookup (0 = Air, default)
    pub material_id: u8,
}

impl TraceSegment {
    #[inline]
    pub const fn new(start: Point3D, end: Point3D, width_nm: i64, material_id: u8) -> Self {
        Self {
            start,
            end,
            width_nm,
            material_id,
        }
    }

    pub fn bounding_box(&self) -> BoundingBox {
        let half_width = self.width_nm / 2;
        BoundingBox {
            min: Point3D::new(
                self.start.x.min(self.end.x) - half_width,
                self.start.y.min(self.end.y) - half_width,
                self.start.z.min(self.end.z),
            ),
            max: Point3D::new(
                self.start.x.max(self.end.x) + half_width,
                self.start.y.max(self.end.y) + half_width,
                self.start.z.max(self.end.z),
            ),
        }
    }

    pub fn is_horizontal(&self) -> bool {
        self.start.y == self.end.y && self.start.z == self.end.z
    }

    pub fn is_vertical(&self) -> bool {
        self.start.x == self.end.x && self.start.z == self.end.z
    }

    pub fn length(&self) -> i64 {
        (self.start.x - self.end.x).abs()
            + (self.start.y - self.end.y).abs()
            + (self.start.z - self.end.z).abs()
    }
}
