use clipper2_rust::{FillRule, Path64, Paths64, Point64};
use compact_str::CompactString;
use hwc_engine::space::PourMetadata;

use super::vector::Vector2D;

/// Represents the exact physical polygon manifold of a device terminal (Zero AABB Loss)
#[derive(Debug, Clone)]
pub struct TerminalGeometry {
    pub terminal_name: CompactString,
    pub paths: Paths64,
}

impl TerminalGeometry {
    /// Builds an exact union manifold from all pours bound to this terminal
    pub fn from_pours(terminal_name: &str, pours: &[PourMetadata]) -> Result<Self, String> {
        if pours.is_empty() {
            return Err(format!("Terminal '{}' has no physical pours bound", terminal_name));
        }

        let mut raw_paths = Paths64::new();
        for p in pours {
            if let Some(ref bbox) = p.bbox {
                let mut path = Path64::new();
                path.push(Point64::new(bbox.min.x, bbox.min.y));
                path.push(Point64::new(bbox.max.x, bbox.min.y));
                path.push(Point64::new(bbox.max.x, bbox.max.y));
                path.push(Point64::new(bbox.min.x, bbox.max.y));
                raw_paths.push(path);
            }
        }

        if raw_paths.is_empty() {
            return Err(format!("Terminal '{}' has no geometry bounding boxes", terminal_name));
        }

        let paths = clipper2_rust::union_64(&raw_paths, &Paths64::new(), FillRule::NonZero);

        Ok(Self {
            terminal_name: terminal_name.into(),
            paths,
        })
    }

    /// Creates a geometry directly from paths
    pub fn from_paths(terminal_name: &str, paths: Paths64) -> Self {
        Self {
            terminal_name: terminal_name.into(),
            paths,
        }
    }

    /// Computes the exact centroid of the polygon manifold using geometric moments
    pub fn centroid(&self) -> Vector2D {
        let mut total_area = 0.0;
        let mut cx_acc = 0.0;
        let mut cy_acc = 0.0;

        for path in &self.paths {
            let a = clipper2_rust::area(path);
            if a.abs() > 1e-6 {
                let mut cx = 0.0;
                let mut cy = 0.0;
                let len = path.len();
                for i in 0..len {
                    let p0 = path[i];
                    let p1 = path[(i + 1) % len];
                    let factor = (p0.x as f64 * p1.y as f64) - (p1.x as f64 * p0.y as f64);
                    cx += (p0.x as f64 + p1.x as f64) * factor;
                    cy += (p0.y as f64 + p1.y as f64) * factor;
                }
                cx /= 6.0 * a;
                cy /= 6.0 * a;

                total_area += a.abs();
                cx_acc += cx * a.abs();
                cy_acc += cy * a.abs();
            }
        }

        if total_area > 0.0 {
            Vector2D::new(cx_acc / total_area, cy_acc / total_area)
        } else {
            Vector2D::new(0.0, 0.0)
        }
    }

    /// Total surface area in square meters (1 m² = 10^18 nm²)
    pub fn area_m2(&self) -> f64 {
        let area_nm2: f64 = self.paths.iter().map(|p| clipper2_rust::area(p).abs()).sum();
        area_nm2 * 1e-18
    }

    /// Total surface area in square micrometers (1 μm² = 10^6 nm²)
    pub fn area_um2(&self) -> f64 {
        let area_nm2: f64 = self.paths.iter().map(|p| clipper2_rust::area(p).abs()).sum();
        area_nm2 * 1e-6
    }

    /// Total perimeter length in meters
    pub fn perimeter_m(&self) -> f64 {
        let mut p_nm = 0.0;
        for path in &self.paths {
            let len = path.len();
            for i in 0..len {
                let p0 = path[i];
                let p1 = path[(i + 1) % len];
                let dx = (p1.x - p0.x) as f64;
                let dy = (p1.y - p0.y) as f64;
                p_nm += dx.hypot(dy);
            }
        }
        p_nm * 1e-9
    }

    /// Total perimeter length in micrometers
    pub fn perimeter_um(&self) -> f64 {
        self.perimeter_m() * 1e6
    }

    /// Measure the physical extent of the manifold along an arbitrary unit direction vector in base SI meters (m)
    pub fn span_along_vector(&self, unit_vec: Vector2D) -> f64 {
        let mut min_proj = f64::INFINITY;
        let mut max_proj = f64::NEG_INFINITY;

        for path in &self.paths {
            for pt in path {
                let proj = (pt.x as f64) * unit_vec.x + (pt.y as f64) * unit_vec.y;
                min_proj = min_proj.min(proj);
                max_proj = max_proj.max(proj);
            }
        }

        if min_proj.is_infinite() || max_proj.is_infinite() {
            0.0
        } else {
            (max_proj - min_proj).max(0.0) * 1e-9
        }
    }

    /// Measure the physical extent in micrometers (μm)
    pub fn span_along_vector_um(&self, unit_vec: Vector2D) -> f64 {
        self.span_along_vector(unit_vec) * 1e6
    }

    /// Performs exact 2D polygon intersection with another terminal geometry
    pub fn intersect(&self, other: &Self) -> Self {
        let paths = clipper2_rust::intersect_64(&self.paths, &other.paths, FillRule::NonZero);

        Self {
            terminal_name: format!("{}_∩_{}", self.terminal_name, other.terminal_name).into(),
            paths,
        }
    }

    /// Performs exact 2D polygon union with another terminal geometry
    pub fn union(&self, other: &Self) -> Self {
        let mut combined = Paths64::new();
        combined.extend(self.paths.clone());
        combined.extend(other.paths.clone());

        let paths = clipper2_rust::union_64(&combined, &Paths64::new(), FillRule::NonZero);

        Self {
            terminal_name: format!("{}_∪_{}", self.terminal_name, other.terminal_name).into(),
            paths,
        }
    }

    /// Performs exact 2D polygon difference: self - other
    pub fn difference(&self, other: &Self) -> Self {
        let paths = clipper2_rust::difference_64(&self.paths, &other.paths, FillRule::NonZero);

        Self {
            terminal_name: format!("{}_\\_{}", self.terminal_name, other.terminal_name).into(),
            paths,
        }
    }

    /// Computes the 2D convex hull envelope encompassing both terminal geometries.
    pub fn convex_hull_envelope(&self, other: &Self) -> Result<Self, String> {
        let mut points: Vec<Point64> = Vec::new();
        for p in &self.paths {
            for pt in p {
                points.push(*pt);
            }
        }
        for p in &other.paths {
            for pt in p {
                points.push(*pt);
            }
        }

        if points.is_empty() {
            return Err("Cannot compute convex hull of empty geometries".into());
        }

        points.sort_by(|a, b| a.x.cmp(&b.x).then_with(|| a.y.cmp(&b.y)));
        points.dedup();

        if points.len() <= 2 {
            let mut path = Path64::new();
            for pt in points {
                path.push(pt);
            }
            let mut paths = Paths64::new();
            paths.push(path);
            return Ok(Self {
                terminal_name: format!("hull({}_{})", self.terminal_name, other.terminal_name).into(),
                paths,
            });
        }

        let cross_product = |o: Point64, a: Point64, b: Point64| -> i128 {
            (a.x as i128 - o.x as i128) * (b.y as i128 - o.y as i128)
                - (a.y as i128 - o.y as i128) * (b.x as i128 - o.x as i128)
        };

        let mut lower: Vec<Point64> = Vec::new();
        for &p in &points {
            while lower.len() >= 2
                && cross_product(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0
            {
                lower.pop();
            }
            lower.push(p);
        }

        let mut upper: Vec<Point64> = Vec::new();
        for &p in points.iter().rev() {
            while upper.len() >= 2
                && cross_product(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0
            {
                upper.pop();
            }
            upper.push(p);
        }

        lower.pop();
        upper.pop();
        lower.extend(upper);

        let mut path = Path64::new();
        for pt in lower {
            path.push(pt);
        }

        let mut paths = Paths64::new();
        paths.push(path);

        Ok(Self {
            terminal_name: format!("hull({}_{})", self.terminal_name, other.terminal_name).into(),
            paths,
        })
    }
}
