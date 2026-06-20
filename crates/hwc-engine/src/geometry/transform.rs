//! Deterministic 2D affine transforms using pure i128 intermediate arithmetic.
//!
//! All trigonometric values are pre-scaled by 10^9 for fixed-point precision.
//! No floating-point math is used in the core transform path.

/// Simple 2D axis-aligned bounding box (no Z axis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundingBox2D {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

impl BoundingBox2D {
    #[inline]
    pub const fn new(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    #[inline]
    pub fn contains_point(&self, x: i64, y: i64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    #[inline]
    pub fn intersects(&self, other: &BoundingBox2D) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    #[inline]
    pub fn expand(&self, margin: i64) -> BoundingBox2D {
        BoundingBox2D {
            min_x: self.min_x - margin,
            min_y: self.min_y - margin,
            max_x: self.max_x + margin,
            max_y: self.max_y + margin,
        }
    }

    #[inline]
    pub fn union(&self, other: &BoundingBox2D) -> BoundingBox2D {
        BoundingBox2D {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    #[inline]
    pub const fn width(&self) -> i64 {
        self.max_x - self.min_x
    }

    #[inline]
    pub const fn height(&self) -> i64 {
        self.max_y - self.min_y
    }
}

/// Deterministic 2D affine transform using pure i128 intermediate arithmetic.
/// Scaled by 10^9 for trigonometric precision. No floating-point in core path.
/// glam is FORBIDDEN in the core pathfinder/collision/transform engine.
#[derive(Clone, Debug)]
pub struct FixedTransform2D {
    pub tx_pm: i64,     // Translation X in picometers
    pub ty_pm: i64,     // Translation Y in picometers
    pub cos_scale: i64, // cos(angle) * SCALE_FACTOR
    pub sin_scale: i64, // sin(angle) * SCALE_FACTOR
}

impl FixedTransform2D {
    /// Scale factor for trigonometric values: 10^9
    pub const SCALE_FACTOR: i128 = 1_000_000_000;

    /// Identity transform (no rotation, no translation)
    #[inline]
    pub const fn identity() -> Self {
        Self {
            tx_pm: 0,
            ty_pm: 0,
            cos_scale: 1_000_000_000,
            sin_scale: 0,
        }
    }

    /// Create from translation only (no rotation)
    #[inline]
    pub const fn from_translation(tx_pm: i64, ty_pm: i64) -> Self {
        Self {
            tx_pm,
            ty_pm,
            cos_scale: 1_000_000_000,
            sin_scale: 0,
        }
    }

    /// Create from rotation only (no translation)
    /// Accepts degrees as i64, maps to fixed cos/sin via lookup table
    pub fn from_rotation(degrees: i64) -> Self {
        let (cos_fixed, sin_fixed) = Self::lookup_trig(degrees);
        Self {
            tx_pm: 0,
            ty_pm: 0,
            cos_scale: cos_fixed,
            sin_scale: sin_fixed,
        }
    }

    /// Create from translation + rotation
    pub fn new(tx_pm: i64, ty_pm: i64, degrees: i64) -> Self {
        let (cos_fixed, sin_fixed) = Self::lookup_trig(degrees);
        Self {
            tx_pm,
            ty_pm,
            cos_scale: cos_fixed,
            sin_scale: sin_fixed,
        }
    }

    /// Transform a 2D point using i128 intermediate arithmetic.
    ///
    /// Promotes to i128 BEFORE multiplication to prevent overflow on 200mm PCBs
    /// where x=2e11 * cos_fixed=7.07e8 = 1.414e20 which exceeds i64::MAX.
    #[inline]
    pub fn transform_point(&self, x: i64, y: i64) -> (i64, i64) {
        let x_128 = x as i128;
        let y_128 = y as i128;
        let cos_128 = self.cos_scale as i128;
        let sin_128 = self.sin_scale as i128;

        let rx = (x_128 * cos_128 - y_128 * sin_128) / Self::SCALE_FACTOR;
        let ry = (x_128 * sin_128 + y_128 * cos_128) / Self::SCALE_FACTOR;

        ((rx as i64).wrapping_add(self.tx_pm), (ry as i64).wrapping_add(self.ty_pm))
    }

    /// Transform a 2D bounding box by transforming all 4 corners
    /// and recomputing the axis-aligned bounding box.
    pub fn transform_bbox_2d(&self, bbox: &BoundingBox2D) -> BoundingBox2D {
        let corners = [
            self.transform_point(bbox.min_x, bbox.min_y),
            self.transform_point(bbox.max_x, bbox.min_y),
            self.transform_point(bbox.min_x, bbox.max_y),
            self.transform_point(bbox.max_x, bbox.max_y),
        ];

        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;

        for (cx, cy) in &corners {
            if *cx < min_x {
                min_x = *cx;
            }
            if *cy < min_y {
                min_y = *cy;
            }
            if *cx > max_x {
                max_x = *cx;
            }
            if *cy > max_y {
                max_y = *cy;
            }
        }

        BoundingBox2D::new(min_x, min_y, max_x, max_y)
    }

    /// Compose two transforms: self × other
    /// Apply other first, then self
    pub fn then(&self, other: &FixedTransform2D) -> FixedTransform2D {
        // Compose rotation matrices:
        // self: [cos_s, -sin_s; sin_s, cos_s]
        // other: [cos_o, -sin_o; sin_o, cos_o]
        // Combined: [cos_s*cos_o - sin_s*sin_o, -(cos_s*sin_o + sin_s*cos_o);
        //            sin_s*cos_o + cos_s*sin_o,  cos_s*cos_o - sin_s*sin_o]

        let cos_s = self.cos_scale as i128;
        let sin_s = self.sin_scale as i128;
        let cos_o = other.cos_scale as i128;
        let sin_o = other.sin_scale as i128;

        let cos_combined = (cos_s * cos_o - sin_s * sin_o) / Self::SCALE_FACTOR;
        let sin_combined = (sin_s * cos_o + cos_s * sin_o) / Self::SCALE_FACTOR;

        // Compose translations:
        // Apply other first: (x, y) -> (x*cos_o - y*sin_o + tx_o, x*sin_o + y*cos_o + ty_o)
        // Then apply self: result -> (rx*cos_s - ry*sin_s + tx_s, rx*sin_s + ry*cos_s + ty_s)
        // Net translation for origin (0,0):
        //   other: (tx_o, ty_o)
        //   self applied to (tx_o, ty_o): (tx_o*cos_s - ty_o*sin_s + tx_s, tx_o*sin_s + ty_o*cos_s + ty_s)
        let tx_o = other.tx_pm as i128;
        let ty_o = other.ty_pm as i128;
        let tx_s = self.tx_pm as i128;
        let ty_s = self.ty_pm as i128;

        let tx_combined = (tx_o * cos_s - ty_o * sin_s) / Self::SCALE_FACTOR + tx_s;
        let ty_combined = (tx_o * sin_s + ty_o * cos_s) / Self::SCALE_FACTOR + ty_s;

        FixedTransform2D {
            tx_pm: tx_combined as i64,
            ty_pm: ty_combined as i64,
            cos_scale: cos_combined as i64,
            sin_scale: sin_combined as i64,
        }
    }

    /// Get inverse transform
    pub fn inverse(&self) -> FixedTransform2D {
        let cos_128 = self.cos_scale as i128;
        let sin_128 = self.sin_scale as i128;

        // Inverse rotation matrix is transpose (cos, -sin; sin, cos) for orthogonal
        // cos(-angle) = cos(angle), sin(-angle) = -sin(angle)
        let cos_inv = cos_128;
        let sin_inv = -sin_128;

        // Inverse translation: -R^(-1) * t
        let tx_128 = self.tx_pm as i128;
        let ty_128 = self.ty_pm as i128;

        let tx_inv = -(tx_128 * cos_inv - ty_128 * sin_inv) / Self::SCALE_FACTOR;
        let ty_inv = -(tx_128 * sin_inv + ty_128 * cos_inv) / Self::SCALE_FACTOR;

        FixedTransform2D {
            tx_pm: tx_inv as i64,
            ty_pm: ty_inv as i64,
            cos_scale: cos_inv as i64,
            sin_scale: sin_inv as i64,
        }
    }

    /// Trigonometric lookup table for standard angles
    /// Returns (cos_fixed, sin_fixed) for the given degree angle
    /// Only supports: 0, 45, 90, 135, 180, 225, 270, 315
    /// Other angles default to (SCALE_FACTOR, 0) = no rotation
    fn lookup_trig(degrees: i64) -> (i64, i64) {
        let normalized = degrees.rem_euclid(360);
        match normalized {
            0 => (1_000_000_000, 0),
            45 => (707_106_781, 707_106_781),
            90 => (0, 1_000_000_000),
            135 => (-707_106_781, 707_106_781),
            180 => (-1_000_000_000, 0),
            225 => (-707_106_781, -707_106_781),
            270 => (0, -1_000_000_000),
            315 => (707_106_781, -707_106_781),
            _ => (1_000_000_000, 0),
        }
    }
}
