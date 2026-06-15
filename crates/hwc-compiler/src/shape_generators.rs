use clipper2_rust::{Path64, Point64};

pub fn square_contour(size_nm: i64) -> Path64 {
    let half = size_nm / 2;
    let mut contour = Path64::new();
    contour.push(Point64::new(-half, -half));
    contour.push(Point64::new(half, -half));
    contour.push(Point64::new(half, half));
    contour.push(Point64::new(-half, half));
    contour
}

pub fn circle_contour(diameter_nm: i64, segments: u32) -> Path64 {
    let radius = diameter_nm / 2;
    let mut contour = Path64::new();
    for i in 0..segments {
        let angle = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
        let x = (radius as f64 * angle.cos()) as i64;
        let y = (radius as f64 * angle.sin()) as i64;
        contour.push(Point64::new(x, y));
    }
    contour
}

pub fn hexagon_contour(diameter_nm: i64) -> Path64 {
    let half = diameter_nm / 2;
    let quarter = diameter_nm / 4;
    let height_quarter = (diameter_nm as f64 * 0.433) as i64;
    let mut contour = Path64::new();
    contour.push(Point64::new(-half, 0));
    contour.push(Point64::new(-quarter, height_quarter));
    contour.push(Point64::new(quarter, height_quarter));
    contour.push(Point64::new(half, 0));
    contour.push(Point64::new(quarter, -height_quarter));
    contour.push(Point64::new(-quarter, -height_quarter));
    contour
}

/// Mathematically generates a non-intersecting star polygon.
///
/// Alternates between outer and inner radius vertices to create a star shape.
/// The contour is always non-self-intersecting as long as `inner_radius_nm < outer_radius_nm`.
pub fn star_generator_contour(
    outer_radius_nm: i64,
    inner_radius_nm: i64,
    points_count: usize,
) -> Path64 {
    let mut contour = Path64::new();
    let total_vertices = points_count * 2;
    for i in 0..total_vertices {
        let angle = (i as f64 / total_vertices as f64) * 2.0 * std::f64::consts::PI;
        let r = if i % 2 == 0 { outer_radius_nm } else { inner_radius_nm };
        let x = (r as f64 * angle.cos()) as i64;
        let y = (r as f64 * angle.sin()) as i64;
        contour.push(Point64::new(x, y));
    }
    contour
}

/// Mathematically generates a non-intersecting gear polygon.
///
/// Each tooth has 4 vertices: tip, flat-top, root, flat-bottom.
/// The contour is always non-self-intersecting as long as `inner_radius_nm < outer_radius_nm`.
pub fn gear_generator_contour(
    outer_radius_nm: i64,
    inner_radius_nm: i64,
    teeth_count: usize,
) -> Path64 {
    let mut contour = Path64::new();
    let total_steps = teeth_count * 4; // 4 steps per tooth: tip, flat, root, flat
    for i in 0..total_steps {
        let angle = (i as f64 / total_steps as f64) * 2.0 * std::f64::consts::PI;
        let r = match i % 4 {
            0 | 1 => outer_radius_nm,
            _ => inner_radius_nm,
        };
        let x = (r as f64 * angle.cos()) as i64;
        let y = (r as f64 * angle.sin()) as i64;
        contour.push(Point64::new(x, y));
    }
    contour
}
