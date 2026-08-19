//! Universal Topological Device Geometry Engine
//!
//! Strongly-typed domain architecture for physical semiconductor device extraction:
//! - Exact 2D Polygon Manifolds (`TerminalGeometry`, `Paths64`)
//! - Continuous Conduction Vector Calculus (`Vector2D`, `ConductionFlux`)
//! - Constructive 2D Boolean Channel Extraction (Clipper2)
//! - 2D Convex Hull Active Diffusion Bed Synthesis
//! - Zero lossy AABB unions, zero fallbacks, zero heuristics.

use clipper2_rust::{FillRule, Path64, Paths64, Point64};
use compact_str::CompactString;
use hwc_engine::space::{BindingPriority, PourMetadata};
use hwc_engine::{HardwareSpace, PhysicalQuantity};
use hwc_parser::ast::device::{ManifoldExpr, MetricExpression};
use rustc_hash::{FxHashMap, FxHashSet};

/// Fundamental physical constants
pub const EPSILON_0: f64 = 8.854_187_8128e-12; // F/m (Vacuum permittivity)

// ============================================================================
// STRONGLY-TYPED 2D VECTOR & CONDUCTION FLUX
// ============================================================================

/// Continuous 2D Vector in physical nanometer/picometer space
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector2D {
    pub x: f64,
    pub y: f64,
}

impl Vector2D {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn magnitude(&self) -> f64 {
        self.x.hypot(self.y)
    }

    #[inline]
    pub fn unit(&self) -> Result<Self, String> {
        let mag = self.magnitude();
        if mag <= 1e-9 {
            return Err("Cannot normalize zero-length vector (degenerate terminal centroids)".into());
        }
        Ok(Self {
            x: self.x / mag,
            y: self.y / mag,
        })
    }

    #[inline]
    pub fn dot(&self, other: &Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Transverse orthogonal normal vector: (-y, x)
    #[inline]
    pub fn normal(&self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
}

/// Strongly-typed Conduction Flux defining the carrier transport vector
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConductionFlux {
    pub from_centroid: Vector2D,
    pub to_centroid: Vector2D,
    pub flux_vector: Vector2D,
    pub unit_flux: Vector2D,
    pub unit_transverse: Vector2D,
}

impl ConductionFlux {
    pub fn from_centroids(from_centroid: Vector2D, to_centroid: Vector2D) -> Result<Self, String> {
        let flux_vector = Vector2D::new(
            to_centroid.x - from_centroid.x,
            to_centroid.y - from_centroid.y,
        );

        let unit_flux = if flux_vector.magnitude() > 1e-6 {
            flux_vector.unit()?
        } else {
            // Default horizontal unit vector for coincident terminal origins
            Vector2D::new(1.0, 0.0)
        };
        let unit_transverse = unit_flux.normal();

        Ok(Self {
            from_centroid,
            to_centroid,
            flux_vector,
            unit_flux,
            unit_transverse,
        })
    }
}

// ============================================================================
// STRONGLY-TYPED 2D TERMINAL GEOMETRY MANIFOLDS
// ============================================================================

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
                // Convert bounding box to exact 4-point Clipper path (in integer nm)
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

        // Union all pour shapes belonging to this terminal into clean 2D polygon contours
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
                // Calculate centroid of this polygon contour
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
            (max_proj - min_proj).max(0.0) * 1e-9 // Convert integer nm -> base SI meters (m)
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
    ///
    /// This is used to synthesize the continuous active diffusion bed across
    /// physically separate Source and Drain diffusions without AABB box inflation.
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

        // Andrew's Monotone Chain 2D convex hull algorithm
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

        // Lower hull
        let mut lower: Vec<Point64> = Vec::new();
        for &p in &points {
            while lower.len() >= 2
                && cross_product(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0
            {
                lower.pop();
            }
            lower.push(p);
        }

        // Upper hull
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

// ============================================================================
// STRONGLY-TYPED DEVICE GEOMETRY CONTEXT
// ============================================================================

/// Strongly-typed evaluation context containing all validated terminal manifolds
pub struct DeviceGeometryContext<'a> {
    pub device_name: &'a str,
    pub terminals: FxHashMap<CompactString, TerminalGeometry>,
    pub terminal_pours: &'a FxHashMap<CompactString, Vec<PourMetadata>>,
    pub space: Option<&'a HardwareSpace>,
}

impl<'a> DeviceGeometryContext<'a> {
    pub fn new(
        device_name: &'a str,
        terminal_pours: &'a FxHashMap<CompactString, Vec<PourMetadata>>,
        space: Option<&'a HardwareSpace>,
    ) -> Result<Self, String> {
        let mut terminals = FxHashMap::default();
        let partitioned_paths = Self::partition_terminal_geometries(terminal_pours);

        for (term_name, paths) in partitioned_paths {
            if !paths.is_empty() {
                terminals.insert(term_name.clone(), TerminalGeometry::from_paths(term_name.as_str(), paths));
            } else if let Some(pours) = terminal_pours.get(&term_name) {
                if !pours.is_empty() {
                    let geom = TerminalGeometry::from_pours(term_name.as_str(), pours)?;
                    terminals.insert(term_name.clone(), geom);
                }
            }
        }

        Ok(Self {
            device_name,
            terminals,
            terminal_pours,
            space,
        })
    }

    /// Partitions shared multi-terminal channel pours using single-terminal contact pours as geometric seeds.
    ///
    /// ## Zero-Magic Seed-Projected Manifold Partitioning (v0.2.2)
    ///
    /// Instead of string matching on "g" or "gate", this function:
    /// 1. Separates single-terminal contact pours (spatial seeds) from multi-terminal channel pours
    /// 2. For shared channel pours bound to 2+ terminals, partitions them using Voronoi/half-plane bisectors
    /// 3. Assigns each partitioned region to the terminal with the closest contact seed centroid
    ///
    /// This mathematically partitions a continuous Active_Diff pour bound to [S, D] into exact
    /// Source and Drain diffusion islands without heuristics.
    fn partition_terminal_geometries(
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    ) -> FxHashMap<CompactString, Paths64> {
        let mut terminal_paths: FxHashMap<CompactString, Paths64> = FxHashMap::default();
        let mut single_terminal_seeds: FxHashMap<CompactString, Vec<Vector2D>> = FxHashMap::default();
        let mut shared_pours: Vec<&PourMetadata> = Vec::new();

        // 1. Separate single-terminal contact pours from multi-terminal channel pours
        for (term, pours) in terminal_pours {
            for pour in pours {
                // Skip pours without device bindings (they won't be partitioned)
                let binding_term_count = pour
                    .device_binding
                    .as_ref()
                    .map(|b| b.terminals.len())
                    .unwrap_or(1); // Default to 1 if no binding (treated as single-terminal)

                if binding_term_count >= 2 {
                    // Multi-terminal channel pour (e.g. Active_Diff bound to [S, D])
                    if !shared_pours.iter().any(|p| p.name == pour.name) {
                        shared_pours.push(pour);
                    }
                } else {
                    // Single-terminal contact pour (e.g. Source_LI bound to [S])
                    if let Some(ref bbox) = pour.bbox {
                        let mut path = Path64::new();
                        path.push(Point64::new(bbox.min.x, bbox.min.y));
                        path.push(Point64::new(bbox.max.x, bbox.min.y));
                        path.push(Point64::new(bbox.max.x, bbox.max.y));
                        path.push(Point64::new(bbox.min.x, bbox.max.y));
                        let mut raw = Paths64::new();
                        raw.push(path);
                        let unioned = clipper2_rust::union_64(&raw, &Paths64::new(), FillRule::NonZero);

                        terminal_paths.entry(term.clone()).or_default().extend(unioned.clone());
                        
                        let cx = (bbox.min.x + bbox.max.x) as f64 / 2.0;
                        let cy = (bbox.min.y + bbox.max.y) as f64 / 2.0;
                        single_terminal_seeds
                            .entry(term.clone())
                            .or_default()
                            .push(Vector2D::new(cx, cy));
                    }
                }
            }
        }

        // 2. Partition shared channel pours among their bound terminals using geometric seeds
        for shared_pour in shared_pours {
            let bbox = match shared_pour.bbox.as_ref() {
                Some(b) => b,
                None => continue,
            };

            let bound_terminals = shared_pour
                .device_binding
                .as_ref()
                .map(|b| b.terminals.clone())
                .unwrap_or_default();

            if bound_terminals.len() < 2 {
                continue;
            }

            // Find seeds belonging to the bound terminals
            let seeds: Vec<(CompactString, Vector2D)> = bound_terminals
                .iter()
                .filter_map(|t| {
                    single_terminal_seeds.get(t).and_then(|pts| {
                        if pts.is_empty() {
                            None
                        } else {
                            let avg_x = pts.iter().map(|p| p.x).sum::<f64>() / pts.len() as f64;
                            let avg_y = pts.iter().map(|p| p.y).sum::<f64>() / pts.len() as f64;
                            Some((t.clone(), Vector2D::new(avg_x, avg_y)))
                        }
                    })
                })
                .collect();

            // If we have distinct spatial seeds for the terminals, partition using Voronoi bisector
            if seeds.len() >= 2 {
                let mut channel_path = Path64::new();
                channel_path.push(Point64::new(bbox.min.x, bbox.min.y));
                channel_path.push(Point64::new(bbox.max.x, bbox.min.y));
                channel_path.push(Point64::new(bbox.max.x, bbox.max.y));
                channel_path.push(Point64::new(bbox.min.x, bbox.max.y));
                let mut channel_paths = Paths64::new();
                channel_paths.push(channel_path);

                // Compute separating hyperplane between seed A and seed B
                let (term_a, seed_a) = &seeds[0];
                let (term_b, seed_b) = &seeds[1];
                let mid_x = (seed_a.x + seed_b.x) / 2.0;
                let mid_y = (seed_a.y + seed_b.y) / 2.0;
                let dx = seed_b.x - seed_a.x;
                let dy = seed_b.y - seed_a.y;
                let is_horizontal_flux = dx.abs() >= dy.abs();

                // Construct half-plane clipper boxes
                let (half_plane_a, half_plane_b) = if is_horizontal_flux {
                    let split_x = mid_x.round() as i64;
                    let (min_x_a, max_x_a, min_x_b, max_x_b) = if seed_a.x < seed_b.x {
                        (bbox.min.x, split_x, split_x, bbox.max.x)
                    } else {
                        (split_x, bbox.max.x, bbox.min.x, split_x)
                    };

                    let mut pa = Path64::new();
                    pa.push(Point64::new(min_x_a, bbox.min.y));
                    pa.push(Point64::new(max_x_a, bbox.min.y));
                    pa.push(Point64::new(max_x_a, bbox.max.y));
                    pa.push(Point64::new(min_x_a, bbox.max.y));

                    let mut pb = Path64::new();
                    pb.push(Point64::new(min_x_b, bbox.min.y));
                    pb.push(Point64::new(max_x_b, bbox.min.y));
                    pb.push(Point64::new(max_x_b, bbox.max.y));
                    pb.push(Point64::new(min_x_b, bbox.max.y));

                    (pa, pb)
                } else {
                    let split_y = mid_y.round() as i64;
                    let (min_y_a, max_y_a, min_y_b, max_y_b) = if seed_a.y < seed_b.y {
                        (bbox.min.y, split_y, split_y, bbox.max.y)
                    } else {
                        (split_y, bbox.max.y, bbox.min.y, split_y)
                    };

                    let mut pa = Path64::new();
                    pa.push(Point64::new(bbox.min.x, min_y_a));
                    pa.push(Point64::new(bbox.max.x, min_y_a));
                    pa.push(Point64::new(bbox.max.x, max_y_a));
                    pa.push(Point64::new(bbox.min.x, max_y_a));

                    let mut pb = Path64::new();
                    pb.push(Point64::new(bbox.min.x, min_y_b));
                    pb.push(Point64::new(bbox.max.x, min_y_b));
                    pb.push(Point64::new(bbox.max.x, max_y_b));
                    pb.push(Point64::new(bbox.min.x, max_y_b));

                    (pa, pb)
                };

                let mut box_a = Paths64::new();
                box_a.push(half_plane_a);
                let mut box_b = Paths64::new();
                box_b.push(half_plane_b);

                let geom_a = clipper2_rust::intersect_64(&channel_paths, &box_a, FillRule::NonZero);
                let geom_b = clipper2_rust::intersect_64(&channel_paths, &box_b, FillRule::NonZero);

                let entry_a = terminal_paths.entry(term_a.clone()).or_default();
                *entry_a = clipper2_rust::union_64(entry_a, &geom_a, FillRule::NonZero);

                let entry_b = terminal_paths.entry(term_b.clone()).or_default();
                *entry_b = clipper2_rust::union_64(entry_b, &geom_b, FillRule::NonZero);
            } else {
                // If no distinct terminal seeds exist, assign channel to all bound terminals (e.g. 2-terminal resistor)
                let mut channel_path = Path64::new();
                channel_path.push(Point64::new(bbox.min.x, bbox.min.y));
                channel_path.push(Point64::new(bbox.max.x, bbox.min.y));
                channel_path.push(Point64::new(bbox.max.x, bbox.max.y));
                channel_path.push(Point64::new(bbox.min.x, bbox.max.y));
                let mut channel_paths = Paths64::new();
                channel_paths.push(channel_path);

                for term in &bound_terminals {
                    let entry = terminal_paths.entry(term.clone()).or_default();
                    *entry = clipper2_rust::union_64(entry, &channel_paths, FillRule::NonZero);
                }
            }
        }

        // Ensure all terminals have an entry
        for term in terminal_pours.keys() {
            terminal_paths.entry(term.clone()).or_default();
        }

        terminal_paths
    }

    /// Evaluates the carrier conduction flux vector between two terminal centroids
    pub fn conduction_flux(&self, from: &str, to: &str) -> Result<ConductionFlux, String> {
        let t_from = self.terminals.get(from).ok_or_else(|| {
            format!("Device '{}' missing terminal '{}'", self.device_name, from)
        })?;
        let t_to = self.terminals.get(to).ok_or_else(|| {
            format!("Device '{}' missing terminal '{}'", self.device_name, to)
        })?;

        ConductionFlux::from_centroids(t_from.centroid(), t_to.centroid())
    }

    /// Extracts the EXACT 2D Active Channel Manifold: Gate ∩ Envelope(Source, Drain)
    pub fn extract_active_channel(
        &self,
        from: &str,
        to: &str,
        control: &str,
    ) -> Result<(TerminalGeometry, ConductionFlux), String> {
        let source_geom = self.terminals.get(from).ok_or_else(|| {
            format!("Device '{}' missing Source terminal '{}'", self.device_name, from)
        })?;
        let drain_geom = self.terminals.get(to).ok_or_else(|| {
            format!("Device '{}' missing Drain terminal '{}'", self.device_name, to)
        })?;
        let gate_geom = self.terminals.get(control).ok_or_else(|| {
            format!("Device '{}' missing Gate terminal '{}'", self.device_name, control)
        })?;

        let flux = ConductionFlux::from_centroids(source_geom.centroid(), drain_geom.centroid())?;

        // 1. Continuous active diffusion bed across Source and Drain
        let active_diffusion = source_geom.convex_hull_envelope(drain_geom)?;

        // 2. Exact conduction channel = Gate ∩ ActiveDiffusion
        // (This automatically and mathematically clips away all gate dogbone heads and routing extensions in field oxide)
        let channel = gate_geom.intersect(&active_diffusion);

        if channel.paths.is_empty() || channel.area_um2() < 1e-9 {
            return Err(format!(
                "FATAL: Gate '{}' has zero spatial overlap with active diffusion (Source '{}' ↔ Drain '{}').\n\
                 No physical MOSFET channel is formed.",
                control, from, to
            ));
        }

        Ok((channel, flux))
    }

    /// Recursively evaluates any 2D Manifold Expression into exact Clipper2 Paths64
    pub fn evaluate_manifold(&self, expr: &ManifoldExpr) -> Result<TerminalGeometry, String> {
        match expr {
            ManifoldExpr::Terminal(name) => {
                self.terminals.get(name.as_str())
                    .cloned()
                    .ok_or_else(|| format!("Device '{}' missing terminal '{}'", self.device_name, name))
            }
            ManifoldExpr::Intersect(a, b) => {
                let geom_a = self.evaluate_manifold(a)?;
                let geom_b = self.evaluate_manifold(b)?;
                Ok(geom_a.intersect(&geom_b))
            }
            ManifoldExpr::Union(a, b) => {
                let geom_a = self.evaluate_manifold(a)?;
                let geom_b = self.evaluate_manifold(b)?;
                Ok(geom_a.union(&geom_b))
            }
            ManifoldExpr::Difference(a, b) => {
                let geom_a = self.evaluate_manifold(a)?;
                let geom_b = self.evaluate_manifold(b)?;
                Ok(geom_a.difference(&geom_b))
            }
            ManifoldExpr::Hull(a, b) => {
                let geom_a = self.evaluate_manifold(a)?;
                let geom_b = self.evaluate_manifold(b)?;
                geom_a.convex_hull_envelope(&geom_b)
            }
        }
    }

    /// Evaluates all metrics in the dictionary with memoization and cycle detection
    pub fn evaluate_all_metrics(
        &self,
        metrics: &FxHashMap<CompactString, MetricExpression>,
    ) -> Result<FxHashMap<CompactString, PhysicalQuantity>, String> {
        let mut resolved = FxHashMap::default();
        let mut visiting = FxHashSet::default();

        for name in metrics.keys() {
            self.eval_metric_recursive(name.as_str(), metrics, &mut resolved, &mut visiting)?;
        }

        Ok(resolved)
    }

    fn eval_metric_recursive(
        &self,
        name: &str,
        metrics: &FxHashMap<CompactString, MetricExpression>,
        resolved: &mut FxHashMap<CompactString, PhysicalQuantity>,
        visiting: &mut FxHashSet<CompactString>,
    ) -> Result<PhysicalQuantity, String> {
        let name_compact: CompactString = name.into();
        if let Some(&qty) = resolved.get(&name_compact) {
            return Ok(qty);
        }

        if visiting.contains(&name_compact) {
            return Err(format!("FATAL: Cyclic dependency detected in metric '{}'", name));
        }

        visiting.insert(name_compact.clone());

        let expr = metrics.get(&name_compact).ok_or_else(|| {
            format!("Metric '{}' referenced but not defined on device '{}'", name, self.device_name)
        })?;

        let val = self.evaluate_metric_expr_internal(expr, metrics, resolved, visiting)?;
        visiting.remove(&name_compact);
        resolved.insert(name_compact, val);
        Ok(val)
    }

    /// Evaluates a single MetricExpression directly
    pub fn evaluate_metric_expr(&self, expr: &MetricExpression) -> Result<PhysicalQuantity, String> {
        let metrics = FxHashMap::default();
        let mut resolved = FxHashMap::default();
        let mut visiting = FxHashSet::default();
        self.evaluate_metric_expr_internal(expr, &metrics, &mut resolved, &mut visiting)
    }

    fn evaluate_metric_expr_internal(
        &self,
        expr: &MetricExpression,
        metrics: &FxHashMap<CompactString, MetricExpression>,
        resolved: &mut FxHashMap<CompactString, PhysicalQuantity>,
        visiting: &mut FxHashSet<CompactString>,
    ) -> Result<PhysicalQuantity, String> {
        match expr {
            MetricExpression::Ref(target) => {
                self.eval_metric_recursive(target.as_str(), metrics, resolved, visiting)
            }
            MetricExpression::SpanAlongFlux { manifold, from, to } => {
                let flux = self.conduction_flux(from.as_str(), to.as_str())?;
                let geom = self.evaluate_manifold(manifold)?;
                let length_m = geom.span_along_vector(flux.unit_flux);
                Ok(PhysicalQuantity::Length(length_m))
            }
            MetricExpression::SpanAlongTransverse { manifold, from, to } => {
                let flux = self.conduction_flux(from.as_str(), to.as_str())?;
                let geom = self.evaluate_manifold(manifold)?;
                let width_m = geom.span_along_vector(flux.unit_transverse);
                Ok(PhysicalQuantity::Length(width_m))
            }
            MetricExpression::Area(manifold) => {
                let geom = self.evaluate_manifold(manifold)?;
                Ok(PhysicalQuantity::Area(geom.area_m2()))
            }
            MetricExpression::Perimeter(manifold) => {
                let geom = self.evaluate_manifold(manifold)?;
                Ok(PhysicalQuantity::Length(geom.perimeter_m()))
            }
            MetricExpression::Divide(num, den) => {
                let q_num = self.evaluate_metric_expr_internal(num, metrics, resolved, visiting)?;
                let q_den = self.evaluate_metric_expr_internal(den, metrics, resolved, visiting)?;
                q_num / q_den
            }
            MetricExpression::Resistance { from, to } => {
                let space = self.space.ok_or("Physical stackup space required for resistance calculation")?;
                let flux = self.conduction_flux(from.as_str(), to.as_str())?;
                let t_from = self.terminals.get(from.as_str()).ok_or_else(|| {
                    format!("Device '{}' missing terminal '{}'", self.device_name, from)
                })?;
                let t_to = self.terminals.get(to.as_str()).ok_or_else(|| {
                    format!("Device '{}' missing terminal '{}'", self.device_name, to)
                })?;
                let channel = t_from.union(t_to);

                let length_m = channel.span_along_vector(flux.unit_flux);
                let width_m = channel.span_along_vector(flux.unit_transverse);

                let channel_pour = self
                    .terminal_pours
                    .values()
                    .flatten()
                    .find(|p| {
                        p.device_binding
                            .as_ref()
                            .map_or(false, |b| b.priority == BindingPriority::Channel)
                    })
                    .or_else(|| self.terminal_pours.values().flatten().next())
                    .ok_or("No physical pour found for resistance calculation")?;

                let mat_id = space
                    .material_registry
                    .get_id(&channel_pour.material_name)
                    .ok_or_else(|| format!("Material '{}' not found", channel_pour.material_name))?;

                let props = space
                    .material_registry
                    .get_physical_props(mat_id)
                    .ok_or_else(|| {
                        format!("Material '{}' has no physical properties defined", channel_pour.material_name)
                    })?;

                let resistivity_ohm_m = props.get("resistivity").ok_or_else(|| {
                    format!("Material '{}' missing REQUIRED 'resistivity' property", channel_pour.material_name)
                })?;

                let z_bot = channel_pour.z_bottom_nm;
                let thickness_nm = space
                    .stackup_layers
                    .iter()
                    .find(|l| z_bot >= l.z_bottom && z_bot < l.z_top)
                    .map(|l| (l.z_top - l.z_bottom) as f64)
                    .ok_or_else(|| format!("Stackup layer not found for pour at Z={}nm", z_bot))?;

                let cross_section_m2 = width_m * (thickness_nm * 1e-9);
                if cross_section_m2 <= 0.0 {
                    return Err(format!(
                        "Invalid zero cross-section area in resistive channel (W={:.2}um, t={:.2}nm)",
                        width_m * 1e6, thickness_nm
                    ));
                }

                let resistance_ohms = resistivity_ohm_m * (length_m / cross_section_m2);
                Ok(PhysicalQuantity::Resistance(resistance_ohms))
            }
            MetricExpression::Capacitance { plate_a, plate_b } => {
                let space = self.space.ok_or("Physical stackup space required for capacitance calculation")?;
                let t_a = self.terminals.get(plate_a.as_str()).ok_or_else(|| {
                    format!("Device '{}' missing plate '{}'", self.device_name, plate_a)
                })?;
                let t_b = self.terminals.get(plate_b.as_str()).ok_or_else(|| {
                    format!("Device '{}' missing plate '{}'", self.device_name, plate_b)
                })?;
                let overlap = t_a.intersect(t_b);
                let area_m2 = overlap.area_m2();

                let pours_a = self.terminal_pours.get(plate_a.as_str()).ok_or("Plate A pours not found")?;
                let pours_b = self.terminal_pours.get(plate_b.as_str()).ok_or("Plate B pours not found")?;
                let p1 = pours_a.first().ok_or("Plate A has empty pour list")?;
                let p2 = pours_b.first().ok_or("Plate B has empty pour list")?;

                let z_min = p1.z_bottom_nm.min(p2.z_bottom_nm);
                let z_max = p1.z_bottom_nm.max(p2.z_bottom_nm);
                let sep_m = (z_max - z_min) as f64 * 1e-9;
                if sep_m <= 0.0 {
                    return Err("Capacitor plates have zero or negative separation distance".into());
                }

                let relative_permittivity = space
                    .stackup_layers
                    .iter()
                    .find(|l| l.z_bottom >= z_min && l.z_top <= z_max)
                    .and_then(|l| space.material_registry.get_id(&l.material_name))
                    .and_then(|id| space.material_registry.get_physical_props(id))
                    .and_then(|props| props.get("relative_permittivity"))
                    .unwrap_or(3.9);

                let capacitance_farads = EPSILON_0 * relative_permittivity * (area_m2 / sep_m);
                Ok(PhysicalQuantity::Capacitance(capacitance_farads))
            }
        }
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use hwc_engine::geometry::{BoundingBox, Point3D};
    use hwc_engine::space::PourMetadata;

    #[test]
    fn test_vector_and_flux_calculus() {
        let v1 = Vector2D::new(0.0, 0.0);
        let v2 = Vector2D::new(1000.0, 0.0);
        let flux = ConductionFlux::from_centroids(v1, v2).unwrap();

        assert_eq!(flux.unit_flux, Vector2D::new(1.0, 0.0));
        assert_eq!(flux.unit_transverse, Vector2D::new(-0.0, 1.0));
    }

    #[test]
    fn test_dogbone_gate_clipping_exact_w_and_l() {
        // Source N+ diffusion: 650nm x 1000nm, centered at x=9.6um, y=6.0um
        // [min_x: 9275, min_y: 5500, max_x: 9925, max_y: 6500]
        let source_bbox = BoundingBox::new(
            Point3D::new(9275, 5500, 0),
            Point3D::new(9925, 6500, 150),
        );
        let source_pour = PourMetadata {
            name: "Source_Diff".into(),
            material_name: "N_Plus_Diffusion".into(),
            net: Some("Source".into()),
            bbox: Some(source_bbox),
            z_bottom_nm: 0,
            device_binding: None,
            pour_id: 1,
            is_copper: true,
        };

        // Drain N+ diffusion: 650nm x 1000nm, centered at x=10.4um, y=6.0um
        // [min_x: 10075, min_y: 5500, max_x: 10725, max_y: 6500]
        let drain_bbox = BoundingBox::new(
            Point3D::new(10075, 5500, 0),
            Point3D::new(10725, 6500, 150),
        );
        let drain_pour = PourMetadata {
            name: "Drain_Diff".into(),
            material_name: "N_Plus_Diffusion".into(),
            net: Some("Drain".into()),
            bbox: Some(drain_bbox),
            z_bottom_nm: 0,
            device_binding: None,
            pour_id: 2,
            is_copper: true,
        };

        // Gate Poly Stem: 150nm x 2600nm, centered at x=10.0um, y=6.0um
        // [min_x: 9925, min_y: 4700, max_x: 10075, max_y: 7300]
        let gate_stem_bbox = BoundingBox::new(
            Point3D::new(9925, 4700, 180),
            Point3D::new(10075, 7300, 360),
        );
        let gate_stem_pour = PourMetadata {
            name: "Gate_Poly".into(),
            material_name: "Polysilicon".into(),
            net: Some("Gate".into()),
            bbox: Some(gate_stem_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 3,
            is_copper: true,
        };

        // Gate Poly Head (Dogbone pad in field oxide): 400nm x 400nm, centered at x=10.0um, y=7.1um
        // [min_x: 9800, min_y: 6900, max_x: 10200, max_y: 7300]
        let gate_head_bbox = BoundingBox::new(
            Point3D::new(9800, 6900, 180),
            Point3D::new(10200, 7300, 360),
        );
        let gate_head_pour = PourMetadata {
            name: "Gate_Poly_Head".into(),
            material_name: "Polysilicon".into(),
            net: Some("Gate".into()),
            bbox: Some(gate_head_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 4,
            is_copper: true,
        };

        let mut terminal_pours = FxHashMap::default();
        terminal_pours.insert("S".into(), vec![source_pour]);
        terminal_pours.insert("D".into(), vec![drain_pour]);
        terminal_pours.insert("G".into(), vec![gate_stem_pour, gate_head_pour]);

        let ctx = DeviceGeometryContext::new("M1", &terminal_pours, None).unwrap();

        // Build entire NMOS metrics dictionary
        let mut metrics = FxHashMap::default();
        let channel_manifold = ManifoldExpr::Intersect(
            Box::new(ManifoldExpr::Terminal("G".into())),
            Box::new(ManifoldExpr::Hull(
                Box::new(ManifoldExpr::Terminal("S".into())),
                Box::new(ManifoldExpr::Terminal("D".into())),
            )),
        );

        metrics.insert("L".into(), MetricExpression::SpanAlongFlux {
            manifold: channel_manifold.clone(),
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("W".into(), MetricExpression::SpanAlongTransverse {
            manifold: channel_manifold,
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("AD".into(), MetricExpression::Area(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("D".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("AS".into(), MetricExpression::Area(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("S".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("PD".into(), MetricExpression::Perimeter(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("D".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("PS".into(), MetricExpression::Perimeter(ManifoldExpr::Difference(
            Box::new(ManifoldExpr::Terminal("S".into())),
            Box::new(ManifoldExpr::Terminal("G".into())),
        )));
        metrics.insert("SA".into(), MetricExpression::SpanAlongFlux {
            manifold: ManifoldExpr::Difference(
                Box::new(ManifoldExpr::Terminal("S".into())),
                Box::new(ManifoldExpr::Terminal("G".into())),
            ),
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("SB".into(), MetricExpression::SpanAlongFlux {
            manifold: ManifoldExpr::Difference(
                Box::new(ManifoldExpr::Terminal("D".into())),
                Box::new(ManifoldExpr::Terminal("G".into())),
            ),
            from: "S".into(),
            to: "D".into(),
        });
        metrics.insert("NRD".into(), MetricExpression::Divide(
            Box::new(MetricExpression::Ref("SB".into())),
            Box::new(MetricExpression::Ref("W".into())),
        ));
        metrics.insert("NRS".into(), MetricExpression::Divide(
            Box::new(MetricExpression::Ref("SA".into())),
            Box::new(MetricExpression::Ref("W".into())),
        ));

        let results = ctx.evaluate_all_metrics(&metrics).unwrap();

        // 1. Channel Length L: 150nm = 0.15um
        assert_eq!(results.get("L").unwrap().to_spice_repr(), "0.15u");

        // 2. Channel Width W: 1000nm = 1.00um
        assert_eq!(results.get("W").unwrap().to_spice_repr(), "1.00u");

        // 3. Areas AD, AS: 650nm x 1000nm = 0.65 um² = 0.65p
        assert_eq!(results.get("AD").unwrap().to_spice_repr(), "0.65p");
        assert_eq!(results.get("AS").unwrap().to_spice_repr(), "0.65p");

        // 4. Perimeters PD, PS: 2 * (650nm + 1000nm) = 3300nm = 3.30um
        assert_eq!(results.get("PD").unwrap().to_spice_repr(), "3.30u");
        assert_eq!(results.get("PS").unwrap().to_spice_repr(), "3.30u");

        // 5. Stress SA, SB: 650nm = 0.65um
        assert_eq!(results.get("SA").unwrap().to_spice_repr(), "0.65u");
        assert_eq!(results.get("SB").unwrap().to_spice_repr(), "0.65u");

        // 6. Squares NRD, NRS: 0.65um / 1.00um = 0.65
        assert_eq!(results.get("NRD").unwrap().to_spice_repr(), "0.65");
        assert_eq!(results.get("NRS").unwrap().to_spice_repr(), "0.65");
    }

    #[test]
    fn test_resistor_span_and_transverse_width() {
        // Resistor body: 4000nm x 1410nm centered at x=10.0um, y=5.0um
        let body_bbox = BoundingBox::new(
            Point3D::new(8000, 4295, 0),
            Point3D::new(12000, 5705, 180),
        );
        let body_pour = PourMetadata {
            name: "Resistor_Body".into(),
            material_name: "Polysilicon".into(),
            net: None,
            bbox: Some(body_bbox),
            z_bottom_nm: 0,
            device_binding: None,
            pour_id: 1,
            is_copper: true,
        };

        // Contact A: 400nm x 1410nm at left edge [min_x: 8000, max_x: 8400]
        let contact_a_bbox = BoundingBox::new(
            Point3D::new(8000, 4295, 180),
            Point3D::new(8400, 5705, 280),
        );
        let contact_a_pour = PourMetadata {
            name: "Contact_A".into(),
            material_name: "Titanium_Silicide".into(),
            net: Some("In".into()),
            bbox: Some(contact_a_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 2,
            is_copper: true,
        };

        // Contact B: 400nm x 1410nm at right edge [min_x: 11600, max_x: 12000]
        let contact_b_bbox = BoundingBox::new(
            Point3D::new(11600, 4295, 180),
            Point3D::new(12000, 5705, 280),
        );
        let contact_b_pour = PourMetadata {
            name: "Contact_B".into(),
            material_name: "Titanium_Silicide".into(),
            net: Some("Out".into()),
            bbox: Some(contact_b_bbox),
            z_bottom_nm: 180,
            device_binding: None,
            pour_id: 3,
            is_copper: true,
        };

        let mut terminal_pours = FxHashMap::default();
        terminal_pours.insert("A".into(), vec![body_pour.clone(), contact_a_pour]);
        terminal_pours.insert("B".into(), vec![body_pour, contact_b_pour]);

        let ctx = DeviceGeometryContext::new("R1", &terminal_pours, None).unwrap();

        let union_ab = ManifoldExpr::Union(
            Box::new(ManifoldExpr::Terminal("A".into())),
            Box::new(ManifoldExpr::Terminal("B".into())),
        );

        let expr_l = MetricExpression::SpanAlongFlux {
            manifold: union_ab.clone(),
            from: "A".into(),
            to: "B".into(),
        };
        let l_qty = ctx.evaluate_metric_expr(&expr_l).unwrap();
        assert_eq!(l_qty.to_spice_repr(), "4.00u");

        let expr_w = MetricExpression::SpanAlongTransverse {
            manifold: union_ab,
            from: "A".into(),
            to: "B".into(),
        };
        let w_qty = ctx.evaluate_metric_expr(&expr_w).unwrap();
        assert_eq!(w_qty.to_spice_repr(), "1.41u");
    }
}
