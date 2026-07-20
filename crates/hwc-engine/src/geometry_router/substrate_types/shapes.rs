use clipper2_rust::{Path64, Paths64};

use super::CapType;

#[derive(Debug, Clone, PartialEq)]
pub enum SubstrateLayerShape {
    Rect,
    Circle {
        radius: i64,
    },
    Polygon {
        outer_contour: Path64,
        holes: Paths64,
        segments: u32,
    },
    Tube {
        outer_diameter: u32,
        inner_diameter: u32,
        pad_diameter: u32,
        segments: u32,
        top_cap: CapType,
        bottom_cap: CapType,
        bottom_outer_diameter: Option<u32>,
    },
}

impl SubstrateLayerShape {
    pub fn cylinder(diameter_nm: i64, segments: u32) -> Self {
        let radius = diameter_nm / 2;
        let mut contour = Path64::new();
        for i in 0..segments {
            let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
            let x = (radius as f64 * angle.cos()) as i64;
            let y = (radius as f64 * angle.sin()) as i64;
            contour.push(clipper2_rust::Point64::new(x, y));
        }
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments,
        }
    }

    pub fn square(size_nm: i64) -> Self {
        let half = size_nm / 2;
        let contour = vec![
            clipper2_rust::Point64::new(-half, -half),
            clipper2_rust::Point64::new(half, -half),
            clipper2_rust::Point64::new(half, half),
            clipper2_rust::Point64::new(-half, half),
        ];
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 4,
        }
    }

    pub fn rect(width_nm: i64, height_nm: i64) -> Self {
        let half_w = width_nm / 2;
        let half_h = height_nm / 2;
        let contour = vec![
            clipper2_rust::Point64::new(-half_w, -half_h),
            clipper2_rust::Point64::new(half_w, -half_h),
            clipper2_rust::Point64::new(half_w, half_h),
            clipper2_rust::Point64::new(-half_w, half_h),
        ];
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 4,
        }
    }

    pub fn hexagon(size_nm: i64) -> Self {
        let half = size_nm / 2;
        let quarter = size_nm / 4;
        let height_quarter = (size_nm as f64 * 0.433) as i64;
        let contour = vec![
            clipper2_rust::Point64::new(-half, 0),
            clipper2_rust::Point64::new(-quarter, height_quarter),
            clipper2_rust::Point64::new(quarter, height_quarter),
            clipper2_rust::Point64::new(half, 0),
            clipper2_rust::Point64::new(quarter, -height_quarter),
            clipper2_rust::Point64::new(-quarter, -height_quarter),
        ];
        SubstrateLayerShape::Polygon {
            outer_contour: contour,
            holes: Paths64::new(),
            segments: 6,
        }
    }
}

pub(super) fn point_in_polygon(px: i64, py: i64, contour: &Path64) -> bool {
    let n = contour.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let yi = contour[i].y;
        let yj = contour[j].y;
        let xi = contour[i].x;
        let xj = contour[j].x;

        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}
