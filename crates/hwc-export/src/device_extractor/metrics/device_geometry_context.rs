use clipper2_rust::{FillRule, Path64, Paths64, Point64};
use compact_str::CompactString;
use hwc_engine::space::{BindingPriority, PourMetadata};
use hwc_engine::{HardwareSpace, PhysicalQuantity};
use hwc_parser::ast::device::{ManifoldExpr, MetricExpression};
use rustc_hash::{FxHashMap, FxHashSet};

use super::vector::{ConductionFlux, Vector2D};
use super::EPSILON_0;
use super::terminal_geometry::TerminalGeometry;

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
    fn partition_terminal_geometries(
        terminal_pours: &FxHashMap<CompactString, Vec<PourMetadata>>,
    ) -> FxHashMap<CompactString, Paths64> {
        let mut terminal_paths: FxHashMap<CompactString, Paths64> = FxHashMap::default();
        let mut single_terminal_seeds: FxHashMap<CompactString, Vec<Vector2D>> = FxHashMap::default();
        let mut shared_pours: Vec<&PourMetadata> = Vec::new();

        for (term, pours) in terminal_pours {
            for pour in pours {
                let binding_term_count = pour
                    .device_binding
                    .as_ref()
                    .map(|b| b.terminals.len())
                    .unwrap_or(1);

                if binding_term_count >= 2 {
                    if !shared_pours.iter().any(|p| p.name == pour.name) {
                        shared_pours.push(pour);
                    }
                } else {
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

            if seeds.len() >= 2 {
                let mut channel_path = Path64::new();
                channel_path.push(Point64::new(bbox.min.x, bbox.min.y));
                channel_path.push(Point64::new(bbox.max.x, bbox.min.y));
                channel_path.push(Point64::new(bbox.max.x, bbox.max.y));
                channel_path.push(Point64::new(bbox.min.x, bbox.max.y));
                let mut channel_paths = Paths64::new();
                channel_paths.push(channel_path);

                let (term_a, seed_a) = &seeds[0];
                let (term_b, seed_b) = &seeds[1];
                let mid_x = (seed_a.x + seed_b.x) / 2.0;
                let mid_y = (seed_a.y + seed_b.y) / 2.0;
                let dx = seed_b.x - seed_a.x;
                let dy = seed_b.y - seed_a.y;
                let is_horizontal_flux = dx.abs() >= dy.abs();

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

        let active_diffusion = source_geom.convex_hull_envelope(drain_geom)?;

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
