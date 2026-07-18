use crate::geometry::{Point2D, Point3D};
use compact_str::CompactString;

/// Pad shape with dimensional data for stamp parsing
#[derive(Debug, Clone)]
pub enum PadShape {
    Rectangle {
        width_nm: i64,
        height_nm: i64,
    },
    Circle {
        diameter_nm: i64,
    },
    Obround {
        width_nm: i64,
        height_nm: i64,
    },
    Polygon {
        points: Vec<Point2D>,
    },
    RoundedRect {
        width_nm: i64,
        height_nm: i64,
        corner_radius_nm: i64,
    },
}

/// Pin data for a baked component
#[derive(Debug, Clone)]
pub struct BakedPin {
    pub name: CompactString,
    pub local_offset: Point3D,
    pub pad_shape: PadShape,
}

/// A fully baked component definition ready for stamp generation
#[derive(Debug, Clone)]
pub struct BakedComponent {
    pub name: CompactString,
    pub width_nm: i64,
    pub height_nm: i64,
    pub pins: Vec<BakedPin>,
}

impl PadShape {
    /// Compute an approximate bounding box (width, height) in nanometers.
    pub fn bounding_box(&self) -> (i64, i64) {
        match self {
            PadShape::Rectangle {
                width_nm,
                height_nm,
            } => (*width_nm, *height_nm),
            PadShape::Circle { diameter_nm } => (*diameter_nm, *diameter_nm),
            PadShape::Obround {
                width_nm,
                height_nm,
            } => (*width_nm, *height_nm),
            PadShape::RoundedRect {
                width_nm,
                height_nm,
                ..
            } => (*width_nm, *height_nm),
            PadShape::Polygon { points } => {
                let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
                (max_x - min_x, max_y - min_y)
            }
        }
    }
}

/// Bake a component definition into a BakedComponent
pub fn bake_component_definition(
    name: CompactString,
    width_nm: i64,
    height_nm: i64,
    pins: Vec<BakedPin>,
) -> BakedComponent {
    BakedComponent {
        name,
        width_nm,
        height_nm,
        pins,
    }
}
